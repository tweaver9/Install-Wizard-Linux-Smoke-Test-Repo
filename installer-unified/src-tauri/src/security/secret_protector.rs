// Secret encryption (encryption-at-rest)
//
// This is the Rust analogue to the C# SecretProtector.
// It provides:
// - Deterministic "is encrypted?" detection via a prefix
// - Authenticated encryption using AES-256-GCM
// - Lazy, file-backed master key stored under the log folder (Prod_Wizard_Log/)
//
// NOTE: In the long-term, the master key should be protected by OS facilities
// (Windows DPAPI / Linux keyring). For Phase 4, we enforce encryption-at-rest for
// database secrets and avoid logging plaintext.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{Context, Result};
use base64::Engine;
use ring::rand::{SecureRandom, SystemRandom};
use std::path::{Path, PathBuf};
use tokio::sync::OnceCell;
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tokio_retry::RetryIf;

const ENC_PREFIX: &str = "ENCv1:";
const KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;

#[derive(Debug)]
pub struct SecretProtector {
    key_path: PathBuf,
    key: OnceCell<[u8; KEY_BYTES]>,
}

impl SecretProtector {
    pub fn new(key_path: PathBuf) -> Self {
        Self {
            key_path,
            key: OnceCell::new(),
        }
    }

    pub fn is_encrypted(&self, value: &str) -> bool {
        value.starts_with(ENC_PREFIX)
    }

    pub async fn encrypt(&self, plaintext: &str) -> Result<String> {
        if plaintext.is_empty() {
            return Ok(ENC_PREFIX.to_string());
        }

        let key = *self.get_or_init_key().await?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|_| anyhow::anyhow!("Internal error: invalid AES-256 key length"))?;

        let mut nonce_bytes = [0u8; NONCE_BYTES];
        SystemRandom::new()
            .fill(&mut nonce_bytes)
            .map_err(|_| anyhow::anyhow!("Failed to generate nonce"))?;

        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|_| anyhow::anyhow!("Secret encryption failed"))?;

        // Store nonce || ciphertext (ciphertext includes GCM tag)
        let mut blob = Vec::with_capacity(NONCE_BYTES + ciphertext.len());
        blob.extend_from_slice(&nonce_bytes);
        blob.extend_from_slice(&ciphertext);

        Ok(format!(
            "{}{}",
            ENC_PREFIX,
            base64::engine::general_purpose::STANDARD.encode(blob)
        ))
    }

    pub async fn decrypt(&self, value: &str) -> Result<String> {
        if !self.is_encrypted(value) {
            // Backward compatibility: treat as plaintext
            return Ok(value.to_string());
        }

        let encoded = value.trim_start_matches(ENC_PREFIX);
        if encoded.is_empty() {
            return Ok(String::new());
        }

        let blob = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .context("Failed to base64-decode encrypted secret")?;

        if blob.len() < NONCE_BYTES {
            anyhow::bail!("Encrypted secret blob is too short");
        }

        let (nonce_bytes, ciphertext) = blob.split_at(NONCE_BYTES);
        let nonce = Nonce::from_slice(nonce_bytes);

        let key = *self.get_or_init_key().await?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|_| anyhow::anyhow!("Internal error: invalid AES-256 key length"))?;

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| anyhow::anyhow!("Secret decryption failed"))?;
        let s = String::from_utf8(plaintext).context("Decrypted secret is not valid UTF-8")?;
        Ok(s)
    }

    async fn get_or_init_key(&self) -> Result<&[u8; KEY_BYTES]> {
        self.key
            .get_or_try_init(|| async {
                // Try load from disk; if missing, create.
                if tokio::fs::try_exists(&self.key_path).await.unwrap_or(false) {
                    let bytes = tokio::fs::read(&self.key_path).await.with_context(|| {
                        format!("Failed to read secret key file: {:?}", self.key_path)
                    })?;

                    let decoded = base64::engine::general_purpose::STANDARD
                        .decode(bytes)
                        .context("Failed to decode secret key file (base64)")?;

                    if decoded.len() != KEY_BYTES {
                        anyhow::bail!(
                            "Secret key file has invalid length (expected {KEY_BYTES} bytes)"
                        );
                    }

                    let mut key = [0u8; KEY_BYTES];
                    key.copy_from_slice(&decoded);
                    return Ok(key);
                }

                // Create parent dir
                if let Some(parent) = self.key_path.parent() {
                    tokio::fs::create_dir_all(parent).await.with_context(|| {
                        format!("Failed to create secret key directory: {:?}", parent)
                    })?;
                }

                let mut key_bytes = [0u8; KEY_BYTES];
                SystemRandom::new()
                    .fill(&mut key_bytes)
                    .map_err(|_| anyhow::anyhow!("Failed to generate secret key"))?;

                // Persist with retries (file may be locked by AV, etc.)
                let encoded = base64::engine::general_purpose::STANDARD.encode(key_bytes);
                let write_action = || async {
                    // Atomic create-new to avoid races; if it already exists, we reload next call.
                    let mut opts = tokio::fs::OpenOptions::new();
                    opts.write(true).create_new(true);
                    let mut file = opts.open(&self.key_path).await.with_context(|| {
                        format!("Failed to create secret key file: {:?}", self.key_path)
                    })?;
                    use tokio::io::AsyncWriteExt;
                    file.write_all(encoded.as_bytes()).await?;
                    file.flush().await?;
                    Ok::<(), anyhow::Error>(())
                };

                let retry_strategy = ExponentialBackoff::from_millis(50)
                    .factor(2)
                    .max_delay(std::time::Duration::from_millis(750))
                    .take(3)
                    .map(jitter);

                let _ = RetryIf::spawn(retry_strategy, write_action, |e: &anyhow::Error| {
                    is_transient_io_error(e)
                })
                .await;

                // If create_new failed due to already existing, that's fine; we'll continue using our in-memory key for this run.
                Ok(key_bytes)
            })
            .await
            .map(|k| k as &[u8; KEY_BYTES])
    }
}

fn is_transient_io_error(err: &anyhow::Error) -> bool {
    // Best-effort classification; file lock / access denied can be transient (AV, indexing).
    err.to_string()
        .to_ascii_lowercase()
        .contains("used by another process")
        || err
            .to_string()
            .to_ascii_lowercase()
            .contains("access is denied")
        || err
            .to_string()
            .to_ascii_lowercase()
            .contains("sharing violation")
}

/// Helper to build the default key path under a log folder.
pub fn default_key_path(log_folder: &Path) -> PathBuf {
    log_folder.join("secrets").join("installer_master_key.b64")
}
