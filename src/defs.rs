/// Price volatility coefficient used by the quote generator.
///
/// This value determines how much a stock price may change
/// during a single update tick.
/// A small value produces smoother price movements.
pub const VOLATILITY: f64 = 0.000082;

/// Server update period in milliseconds.
///
/// This interval defines how often the quote server:
/// - updates stock prices,
/// - notifies subscribed clients.
///
/// Lower values increase update frequency but also CPU usage.
pub const SERVER_TICK_PERIOD_MSEC: u64 = 200;

/// Retry interval (in milliseconds) when attempting to acquire
/// the write lock for updating quotes.
///
/// If the server cannot immediately obtain the lock (e.g., due to contention),
/// it retries after this delay.
pub const QUOTE_UPDATE_RETRY_PERIOD_MSEC: u64 = 10;

/// Maximum time (in milliseconds) allowed for acquiring the write lock
/// during quote updates.
///
/// If the quote update loop is blocked longer than this timeout,
/// the update is aborted and an error is returned.
pub const WRITE_TIMEOUT_MS: u64 = 1000;

/// Maximum allowed time (in seconds) between PING messages from a client.
///
/// If no input is received from the TCP connection within this interval,
/// the server assumes the client is dead and removes its subscription.
pub const CLIENT_KEEP_ALIVE_SEC: u64 = 5;

/// Polling interval (in milliseconds) used by the TCP connection handler
/// when waiting for data or performing keep-alive checks.
///
/// Reducing this value makes the server more responsive but increases CPU usage.
pub const TCP_CONNECTION_TICK_PERIOD_MSEC: u64 = 200;
