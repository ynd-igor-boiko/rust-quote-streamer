use crate::errors::QuoteGeneratorError;
use crate::stock_quote::StockQuote;

use rand::SeedableRng;
use rand::rngs::StdRng;
use rand_distr::{Distribution, LogNormal};
use rayon::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};

/// A parallel random stock quote generator.
///
/// This generator updates stock quotes in parallel using [`rayon`].
/// Each thread creates its own RNG (`StdRng`), ensuring thread safety.
#[derive(Debug)]
pub struct QuoteGenerator {
    /// Volatility (standard deviation) of the log-normal price distribution.
    volatility: f64,
}

impl QuoteGenerator {
    /// Creates a new quote generator.
    ///
    /// # Arguments
    /// * `volatility` — price fluctuation factor (must be in `(0, 1]`).
    ///
    /// # Errors
    /// Returns [`QuoteGeneratorError::InvalidVolatility`] if `volatility <= 0.0` or `> 1.0`.
    pub fn new(volatility: f64) -> Result<Self, QuoteGeneratorError> {
        if volatility <= 0.0 || volatility > 1.0 {
            return Err(QuoteGeneratorError::InvalidVolatility(volatility));
        }
        Ok(Self { volatility })
    }

    /// Updates stock quotes from any mutable iterator.
    ///
    /// Works with slices, `Vec`, `HashMap::values_mut`, or any other
    /// container that provides an iterator over `&mut StockQuote`.
    pub fn update_quotes<'a, I>(&self, quotes: I) -> Result<(), QuoteGeneratorError>
    where
        I: IntoIterator<Item = &'a mut StockQuote>,
        I::IntoIter: Send + 'a,
    {
        let log_normal = LogNormal::new(0.0, self.volatility)?;

        quotes.into_iter().par_bridge().try_for_each(|quote| {
            let mut rng = StdRng::from_entropy();
            update_single_quote(quote, &mut rng, &log_normal)
        })?;

        Ok(())
    }
}

/// Updates a single quote by applying random price and volume changes.
fn update_single_quote(
    quote: &mut StockQuote,
    rng: &mut StdRng,
    log_normal: &LogNormal<f64>,
) -> Result<(), QuoteGeneratorError> {
    let multiplier = log_normal.sample(rng);
    quote.price *= multiplier;

    quote.volume = match quote.ticker.as_str() {
        "AAPL" | "MSFT" | "TSLA" => 1000 + (rand::random::<f64>() * 5000.0) as u32,
        _ => 100 + (rand::random::<f64>() * 1000.0) as u32,
    };

    quote.timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(QuoteGeneratorError::TimeError)?
        .as_millis() as u64;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_test_quotes() -> Vec<StockQuote> {
        vec![
            StockQuote {
                ticker: "AAPL".into(),
                price: 150.0,
                volume: 0,
                timestamp: 0,
            },
            StockQuote {
                ticker: "MSFT".into(),
                price: 300.0,
                volume: 0,
                timestamp: 0,
            },
            StockQuote {
                ticker: "TSLA".into(),
                price: 700.0,
                volume: 0,
                timestamp: 0,
            },
            StockQuote {
                ticker: "GOOG".into(),
                price: 2800.0,
                volume: 0,
                timestamp: 0,
            },
        ]
    }

    #[test]
    fn test_update_vec() {
        let mut quotes = create_test_quotes();
        let generator = QuoteGenerator::new(0.1).unwrap();

        let original_prices: Vec<f64> = quotes.iter().map(|q| q.price).collect();
        generator.update_quotes(quotes.iter_mut()).unwrap();

        for (i, q) in quotes.iter().enumerate() {
            assert_ne!(q.price, original_prices[i]);
            assert!(q.volume > 0);
            assert!(q.timestamp > 0);
        }
    }

    #[test]
    fn test_update_hashmap() {
        let mut quotes: HashMap<String, StockQuote> = create_test_quotes()
            .into_iter()
            .map(|q| (q.ticker.clone(), q))
            .collect();

        let generator = QuoteGenerator::new(0.2).unwrap();
        let before: HashMap<String, f64> =
            quotes.iter().map(|(k, v)| (k.clone(), v.price)).collect();

        generator.update_quotes(quotes.values_mut()).unwrap();

        for (ticker, quote) in &quotes {
            assert_ne!(quote.price, before[ticker]);
            assert!(quote.volume > 0);
        }
    }

    #[test]
    fn test_invalid_volatility() {
        let res = QuoteGenerator::new(0.0);
        assert!(matches!(
            res,
            Err(QuoteGeneratorError::InvalidVolatility(_))
        ));
    }
}
