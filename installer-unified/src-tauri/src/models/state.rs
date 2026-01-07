// Application state (in-memory)
//
// NOTE: This is NOT persisted; it holds runtime-only context (e.g. the last-used config DB
// connection string) so commands like `get_setup_status` can work without requiring the UI to
// resend secrets on every call.

use tokio::sync::Mutex;

#[derive(Debug, Default)]
pub struct AppState {
    inner: Mutex<AppStateInner>,
}

#[derive(Debug, Default)]
struct AppStateInner {
    config_db_connection_string: Option<String>,
    config_db_engine: Option<String>, // "sqlserver" | "postgres"
    config_db_engine_version: Option<String>, // "2022" | "17" etc
}

impl AppState {
    pub async fn set_config_db(
        &self,
        engine: String,
        engine_version: String,
        connection_string: String,
    ) {
        let mut inner = self.inner.lock().await;
        inner.config_db_engine = Some(engine);
        inner.config_db_engine_version = Some(engine_version);
        inner.config_db_connection_string = Some(connection_string);
    }

    pub async fn get_config_db(&self) -> Option<(String, String, String)> {
        let inner = self.inner.lock().await;
        match (
            inner.config_db_engine.clone(),
            inner.config_db_engine_version.clone(),
            inner.config_db_connection_string.clone(),
        ) {
            (Some(engine), Some(version), Some(cs)) => Some((engine, version, cs)),
            _ => None,
        }
    }
}
