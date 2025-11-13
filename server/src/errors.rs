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
