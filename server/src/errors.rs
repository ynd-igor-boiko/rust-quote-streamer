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
}

/// Possible client-side errors (primarily callback failures)
#[derive(Error, Debug)]
pub enum ClientError {
    #[error("Callback returned an error: {0}")]
    CallbackFailed(String),
    #[error("Failed to initialize client loop: {0}")]
    InitializationError(String),
}
