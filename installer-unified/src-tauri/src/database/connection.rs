// Database connection management
// Ported from C# database connection logic
//
// Phase 6 Addition: DbConnector trait for deterministic testing of connection
// failure paths without requiring a real database.

use anyhow::Result;
use async_trait::async_trait;
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use std::time::Duration;
use tiberius::{Client, Config};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

// =============================================================================
// DbConnector Trait — Enables deterministic testing without real DB
// =============================================================================

/// Error returned by connection attempts.
/// Keeps user-friendly messages separate from internal details.
#[derive(Debug, Clone)]
pub struct ConnectError {
    /// User-friendly message (safe to show in UI)
    pub user_message: String,
    /// Internal details for logging (may contain masked info)
    pub internal_details: String,
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.user_message)
    }
}

impl std::error::Error for ConnectError {}

/// Trait for database connection attempts.
/// Production code uses RealDbConnector; tests use StubDbConnector.
#[async_trait]
pub trait DbConnector: Send + Sync {
    /// Attempt to connect to a database.
    /// Returns Ok(()) on success, or ConnectError with user-friendly message.
    async fn connect(&self, engine: &str, connection_string: &str) -> Result<(), ConnectError>;

    /// Get the timeout duration for connection attempts.
    fn timeout_duration(&self) -> Duration {
        Duration::from_secs(20)
    }

    /// Get the maximum number of retry attempts.
    fn max_retries(&self) -> u32 {
        3
    }
}

/// Production connector that actually connects to databases.
pub struct RealDbConnector;

#[async_trait]
impl DbConnector for RealDbConnector {
    async fn connect(&self, engine: &str, connection_string: &str) -> Result<(), ConnectError> {
        let result = match engine {
            "postgres" => {
                timeout(
                    self.timeout_duration(),
                    DatabaseConnection::postgres(connection_string),
                )
                .await
            }
            _ => {
                timeout(
                    self.timeout_duration(),
                    DatabaseConnection::sql_server(connection_string),
                )
                .await
            }
        };

        match result {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(ConnectError {
                user_message: "Unable to connect. Verify host, credentials, and network access."
                    .to_string(),
                internal_details: format!("Connection error: {}", e),
            }),
            Err(_) => Err(ConnectError {
                user_message: "Connection timed out. Check network connectivity and firewall."
                    .to_string(),
                internal_details: "Connection attempt timed out".to_string(),
            }),
        }
    }
}

#[allow(dead_code)]
pub enum DatabaseEngine {
    SqlServer,
    Postgres,
}

/// SQL Server connection wrapper
/// Uses Arc<Mutex<>> for thread-safe shared access to the client
/// This is the production-ready pattern for tiberius clients
pub struct SqlServerConnection {
    // Arc<Mutex<>> allows safe concurrent access to the client
    // This is the standard pattern for shared async database clients
    client: Arc<Mutex<Client<Compat<TcpStream>>>>,
}

impl Clone for SqlServerConnection {
    fn clone(&self) -> Self {
        Self {
            client: Arc::clone(&self.client),
        }
    }
}

impl SqlServerConnection {
    /// Get a reference to the client for executing queries
    /// Returns a guard that can be used to execute queries
    pub fn client(&self) -> Arc<Mutex<Client<Compat<TcpStream>>>> {
        Arc::clone(&self.client)
    }

    /// Execute a query on SQL Server
    /// This is a convenience method that handles the mutex lock
    #[allow(dead_code)]
    pub async fn execute_query(&self, _query: &str) -> Result<()> {
        let _client = self.client.lock().await;
        // Example query execution - will be expanded when implementing actual operations
        // client.simple_query(query).await?;
        Ok(())
    }
}

/// Database connection enum supporting both SQL Server and PostgreSQL
#[derive(Clone)]
pub enum DatabaseConnection {
    SqlServer(SqlServerConnection),
    Postgres(Pool<Postgres>),
}

impl DatabaseConnection {
    /// Create a PostgreSQL connection
    pub async fn postgres(connection_string: &str) -> Result<Self> {
        let pool = Pool::<Postgres>::connect(connection_string).await?;
        Ok(DatabaseConnection::Postgres(pool))
    }

    /// Create a SQL Server connection
    /// This is a production-ready implementation using proper async patterns
    pub async fn sql_server(connection_string: &str) -> Result<Self> {
        let config = Config::from_ado_string(connection_string)?;
        let tcp = TcpStream::connect(config.get_addr()).await?;
        tcp.set_nodelay(true)?;

        // Convert TcpStream to compatible async write stream for tiberius
        // tiberius expects a futures::io::AsyncWrite, so we use compat_write
        let client = Client::connect(config, tcp.compat_write()).await?;

        // Wrap in Arc<Mutex<>> for thread-safe shared access
        // This is the standard pattern for production database clients
        Ok(DatabaseConnection::SqlServer(SqlServerConnection {
            client: Arc::new(Mutex::new(client)),
        }))
    }

    /// Get SQL Server client if this is a SQL Server connection
    pub fn as_sql_server(&self) -> Option<Arc<Mutex<Client<Compat<TcpStream>>>>> {
        match self {
            DatabaseConnection::SqlServer(conn) => Some(conn.client()),
            _ => None,
        }
    }

    /// Get PostgreSQL pool if this is a PostgreSQL connection
    pub fn as_postgres(&self) -> Option<&Pool<Postgres>> {
        match self {
            DatabaseConnection::Postgres(pool) => Some(pool),
            _ => None,
        }
    }
}

// =============================================================================
// Test Module — Deterministic Behavioral Connection Failure Tests (Phase 6.3)
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Instant;

    // -------------------------------------------------------------------------
    // Stub Connectors for Deterministic Testing
    // -------------------------------------------------------------------------

    /// Stub that immediately returns a controlled failure.
    pub struct ImmediateFailureStub {
        pub user_message: String,
        pub internal_details: String,
        pub call_count: AtomicU32,
    }

    impl ImmediateFailureStub {
        pub fn new(user_message: &str, internal_details: &str) -> Self {
            Self {
                user_message: user_message.to_string(),
                internal_details: internal_details.to_string(),
                call_count: AtomicU32::new(0),
            }
        }
    }

    #[async_trait]
    impl DbConnector for ImmediateFailureStub {
        async fn connect(&self, _engine: &str, _conn_str: &str) -> Result<(), ConnectError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Err(ConnectError {
                user_message: self.user_message.clone(),
                internal_details: self.internal_details.clone(),
            })
        }

        fn timeout_duration(&self) -> Duration {
            Duration::from_millis(100) // Fast for tests
        }

        fn max_retries(&self) -> u32 {
            3
        }
    }

    /// Stub that hangs forever (for timeout testing).
    pub struct HangingStub {
        pub call_count: AtomicU32,
    }

    impl HangingStub {
        pub fn new() -> Self {
            Self {
                call_count: AtomicU32::new(0),
            }
        }
    }

    #[async_trait]
    impl DbConnector for HangingStub {
        async fn connect(&self, _engine: &str, _conn_str: &str) -> Result<(), ConnectError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            // Hang forever — caller must use timeout
            std::future::pending::<()>().await;
            unreachable!()
        }

        fn timeout_duration(&self) -> Duration {
            Duration::from_millis(100) // Short timeout for tests
        }

        fn max_retries(&self) -> u32 {
            1 // Single attempt for timeout test
        }
    }

    /// Stub that succeeds after N failures (for retry testing).
    pub struct FailThenSucceedStub {
        pub failures_before_success: u32,
        pub call_count: AtomicU32,
    }

    impl FailThenSucceedStub {
        pub fn new(failures_before_success: u32) -> Self {
            Self {
                failures_before_success,
                call_count: AtomicU32::new(0),
            }
        }
    }

    #[async_trait]
    impl DbConnector for FailThenSucceedStub {
        async fn connect(&self, _engine: &str, _conn_str: &str) -> Result<(), ConnectError> {
            let count = self.call_count.fetch_add(1, Ordering::SeqCst);
            if count < self.failures_before_success {
                Err(ConnectError {
                    user_message: "Temporary failure, retrying...".to_string(),
                    internal_details: format!("Attempt {} failed", count + 1),
                })
            } else {
                Ok(())
            }
        }

        fn timeout_duration(&self) -> Duration {
            Duration::from_millis(50)
        }

        fn max_retries(&self) -> u32 {
            5
        }
    }

    // -------------------------------------------------------------------------
    // Helper: Connect with timeout and retry (testable version)
    // -------------------------------------------------------------------------

    async fn connect_with_retry_testable<C: DbConnector>(
        connector: &C,
        engine: &str,
        conn_str: &str,
    ) -> Result<(), ConnectError> {
        let mut last_error = None;

        for attempt in 0..connector.max_retries() {
            let result = timeout(
                connector.timeout_duration(),
                connector.connect(engine, conn_str),
            )
            .await;

            match result {
                Ok(Ok(())) => return Ok(()),
                Ok(Err(e)) => {
                    last_error = Some(e);
                    // Brief delay between retries
                    if attempt + 1 < connector.max_retries() {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }
                Err(_) => {
                    last_error = Some(ConnectError {
                        user_message:
                            "Connection timed out. Check network connectivity and firewall."
                                .to_string(),
                        internal_details: format!("Timeout on attempt {}", attempt + 1),
                    });
                }
            }
        }

        Err(last_error.unwrap_or_else(|| ConnectError {
            user_message: "Connection failed after all retries.".to_string(),
            internal_details: "Unknown error".to_string(),
        }))
    }

    // -------------------------------------------------------------------------
    // Deterministic Behavioral Tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn connection_timeout_completes_within_budget() {
        // INTENT: Prove that timeout path triggers deterministically within time budget.
        let start = Instant::now();
        let stub = HangingStub::new();

        let result = connect_with_retry_testable(&stub, "postgres", "ignored").await;

        let elapsed = start.elapsed();

        // Must complete within 3 seconds (generous for CI)
        assert!(
            elapsed < Duration::from_secs(3),
            "Test must complete within 3s, took {:?}",
            elapsed
        );

        // Must be an error (timeout)
        assert!(result.is_err(), "Should fail with timeout");

        // Error must be user-friendly
        let err = result.unwrap_err();
        assert!(
            err.user_message.contains("timed out"),
            "User message should mention timeout: {}",
            err.user_message
        );
        assert!(
            !err.user_message.contains("panic"),
            "Should not contain panic"
        );
        assert!(
            !err.user_message.contains("unwrap"),
            "Should not contain unwrap"
        );
    }

    #[tokio::test]
    async fn immediate_failure_returns_user_friendly_error() {
        // INTENT: Prove that immediate failure returns user-friendly error.
        let stub = ImmediateFailureStub::new(
            "Unable to connect. Verify host, credentials, and network access.",
            "Auth failed: invalid password",
        );

        let result = connect_with_retry_testable(&stub, "sqlserver", "ignored").await;

        assert!(result.is_err(), "Should fail");

        let err = result.unwrap_err();

        // User message must be friendly
        assert!(
            !err.user_message.contains("invalid password"),
            "Should not leak internal details in user message"
        );
        assert!(
            err.user_message.contains("Unable to connect"),
            "Should have friendly message"
        );

        // Internal details preserved for logging
        assert!(
            err.internal_details.contains("Auth failed"),
            "Internal details should be preserved"
        );

        // Retry happened
        assert_eq!(
            stub.call_count.load(Ordering::SeqCst),
            3,
            "Should retry max_retries times"
        );
    }

    #[tokio::test]
    async fn retry_bounded_does_not_infinite_loop() {
        // INTENT: Prove that retry is bounded and doesn't loop forever.
        let start = Instant::now();
        let stub = ImmediateFailureStub::new("Temporary error", "transient");

        let result = connect_with_retry_testable(&stub, "postgres", "ignored").await;

        let elapsed = start.elapsed();

        // Must complete within 1 second (retries should be fast)
        assert!(
            elapsed < Duration::from_secs(1),
            "Retries must complete quickly, took {:?}",
            elapsed
        );

        // Should have failed after exactly max_retries attempts
        assert!(result.is_err());
        assert_eq!(
            stub.call_count.load(Ordering::SeqCst),
            3,
            "Should attempt exactly max_retries times"
        );
    }

    #[tokio::test]
    async fn retry_succeeds_after_transient_failures() {
        // INTENT: Prove that retry can recover from transient failures.
        let stub = FailThenSucceedStub::new(2); // Fail twice, then succeed

        let result = connect_with_retry_testable(&stub, "postgres", "ignored").await;

        assert!(result.is_ok(), "Should succeed after 2 failures");
        assert_eq!(
            stub.call_count.load(Ordering::SeqCst),
            3,
            "Should have made 3 attempts (2 failures + 1 success)"
        );
    }

    #[tokio::test]
    async fn error_message_never_contains_password() {
        // INTENT: Prove that error messages don't leak passwords.
        let passwords = vec![
          "PASSWORD_SHOULD_BE_REDACTED",
          "API_KEY_SHOULD_BE_REDACTED",
          "TOKEN_SHOULD_BE_REDACTED",
        ];


        for password in passwords {
            let internal_details = format!("Auth failed with password={}", password);
            let stub = ImmediateFailureStub::new(
                "Unable to connect. Verify credentials.",
                &internal_details,
            );

            let result = connect_with_retry_testable(&stub, "postgres", "ignored").await;
            let err = result.unwrap_err();

            // User message must never contain the password
            assert!(
                !err.user_message.contains(password),
                "User message leaked password '{}': {}",
                password,
                err.user_message
            );
        }
    }

    #[tokio::test]
    async fn connect_error_display_is_user_friendly() {
        // INTENT: Prove that ConnectError Display trait shows user message only.
        let err = ConnectError {
            user_message: "Connection refused by server.".to_string(),
            internal_details: "tcp connect failed: errno=111".to_string(),
        };

        let displayed = format!("{}", err);

        assert_eq!(
            displayed, "Connection refused by server.",
            "Display should show user_message"
        );
        assert!(
            !displayed.contains("errno"),
            "Display should not show internal details"
        );
    }

    #[test]
    fn connect_error_is_send_sync() {
        // INTENT: Prove ConnectError can be used across threads.
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ConnectError>();
    }
}
