use rand_distr::NormalError;
use thiserror::Error;

/// Errors that may occur inside the [`QuoteGenerator`](crate::quote_generator::QuoteGenerator).
///
/// These errors represent failures in randomness generation, invalid inputs,
/// or system time retrieval issues that affect quote computation.
#[derive(Debug)]
pub enum QuoteGeneratorError {
    /// Supplied volatility parameter is outside the allowed numeric range.
    ///
    /// Volatility must be within `(0.0, 1.0]`.
    InvalidVolatility(f64),

    /// Failure retrieving the current system time.
    ///
    /// This usually indicates that the system clock is not monotonic or
    /// the OS returned an unexpected error.
    TimeError(std::time::SystemTimeError),

    /// Error constructing the internal log-normal distribution used for
    /// generating stock price movements.
    ///
    /// This originates from the `rand_distr` crate.
    DistributionError(NormalError),
}

impl From<NormalError> for QuoteGeneratorError {
    fn from(err: NormalError) -> Self {
        QuoteGeneratorError::DistributionError(err)
    }
}

/// Errors returned from the [`QuoteServer`](crate::quote_server::QuoteServer).
///
/// These errors cover misconfiguration, failed initialization,
/// issues in the update loop, and client-management failures.
#[derive(Error, Debug)]
pub enum QuoteServerError {
    /// Configuration file is malformed, missing, or contains invalid ticker data.
    #[error("Invalid Quote Server config: {0}")]
    InvalidConfig(String),

    /// Critical failure during server initialization.
    ///
    /// Usually indicates I/O issues or failure to build initial quote state.
    #[error("Failed to initialize Quote Server: {0}")]
    InitializationError(String),

    /// Failure while updating stock quotes inside the server loop.
    ///
    /// May indicate generator errors or lock/contention timeout.
    #[error("Failed to update quotes: {0}")]
    UpdateQuoteError(String),

    /// Errors related to adding or removing clients (invalid ID, I/O errors, etc.).
    #[error("Failed to add/remove client: {0}")]
    ClientError(String),
}

/// Errors related to individual streaming clients.
///
/// These errors usually originate from callback failures when attempting to
/// deliver updates via UDP or from internal client loop initialization failures.
#[derive(Error, Debug)]
pub enum ClientError {
    /// Client callback returned an error or failed to send UDP data.
    #[error("Callback returned an error: {0}")]
    CallbackFailed(String),

    /// Failure initializing a client-side update or notification routine.
    #[error("Failed to initialize client loop: {0}")]
    InitializationError(String),
}

impl From<ClientError> for QuoteServerError {
    fn from(err: ClientError) -> Self {
        QuoteServerError::ClientError(err.to_string())
    }
}

/// Errors produced by the TCP server subsystem.
///
/// These include I/O errors, protocol violations, and errors propagated
/// from the underlying [`QuoteServer`](crate::quote_server::QuoteServer).
#[derive(Error, Debug)]
pub enum TcpServerError {
    /// The TCP listener failed to bind to the specified address/port.
    #[error("Failed to bind TCP listener: {0}")]
    BindError(String),

    /// Error while accepting an incoming TCP client connection.
    #[error("Failed to accept TCP connection: {0}")]
    AcceptError(String),

    /// I/O error during communication with a specific client.
    #[error("Client IO error: {0}")]
    ClientIoError(String),

    /// The server received an invalid command or malformed input.
    #[error("Invalid command received: {0}")]
    InvalidCommand(String),

    /// An error from the quote server subsystem bubbled up into the TCP layer.
    #[error("Quote server error: {0}")]
    QuoteServerError(#[from] QuoteServerError),
}

/// High-level errors returned by the server and client binaries.
///
/// These errors are used at the application entry point for formatting
/// user-facing error messages and wrapping lower-level failures.
#[derive(Error, Debug)]
pub enum CliError {
    /// General wrapper around any textual failure.
    #[error("Cli failed with error: {0}")]
    GeneralError(String),
}

impl From<QuoteServerError> for CliError {
    fn from(err: QuoteServerError) -> Self {
        CliError::GeneralError(err.to_string())
    }
}
