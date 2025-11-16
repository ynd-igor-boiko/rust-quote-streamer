use rand_distr::NormalError;
use thiserror::Error;

/// Represents possible errors returned by [`QuoteGenerator`].
#[derive(Debug)]
pub enum QuoteGeneratorError {
    /// Invalid volatility value (must be in the range `(0.0, 1.0]`).
    InvalidVolatility(f64),
    /// System time error (usually when obtaining the current time).
    TimeError(std::time::SystemTimeError),
    /// Error creating the log-normal distribution.
    DistributionError(NormalError),
}

impl From<NormalError> for QuoteGeneratorError {
    fn from(err: NormalError) -> Self {
        QuoteGeneratorError::DistributionError(err)
    }
}

#[derive(Error, Debug)]
pub enum QuoteServerError {
    #[error("Invalid Quote Server config: {0}")]
    InvalidConfig(String),
    #[error("Failed to initialize Quote Server: {0}")]
    InitializationError(String),
    #[error("Failed to update quotes: {0}")]
    UpdateQuoteError(String),
    #[error("Failed to add/remove client: {0}")]
    ClientError(String),
}

/// Possible client-side errors (primarily callback failures)
#[derive(Error, Debug)]
pub enum ClientError {
    #[error("Callback returned an error: {0}")]
    CallbackFailed(String),
    #[error("Failed to initialize client loop: {0}")]
    InitializationError(String),
}

impl From<ClientError> for QuoteServerError {
    fn from(err: ClientError) -> Self {
        QuoteServerError::ClientError(err.to_string())
    }
}

#[derive(Error, Debug)]
pub enum TcpServerError {
    #[error("Failed to bind TCP listener: {0}")]
    BindError(String),
    #[error("Failed to accept TCP connection: {0}")]
    AcceptError(String),
    #[error("Client IO error: {0}")]
    ClientIoError(String),
    #[error("Invalid command received: {0}")]
    InvalidCommand(String),
    #[error("Quote server error: {0}")]
    QuoteServerError(#[from] QuoteServerError),
}
