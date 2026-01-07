// Cryptographic utilities

use base64::Engine;
use sha2::{Digest, Sha256};

/// SHA-256 hex digest (lowercase).
pub fn sha256_hex(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

/// SHA-256 base64 digest (STANDARD).
pub fn sha256_base64(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    let digest = hasher.finalize();
    base64::engine::general_purpose::STANDARD.encode(digest)
}

/// Compute a safe fingerprint for a secret (hash only; never store the raw secret).
pub fn secret_fingerprint(input: &str) -> String {
    sha256_base64(input.as_bytes())
}
