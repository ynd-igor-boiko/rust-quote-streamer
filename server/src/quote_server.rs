use crate::defs::{
    QUOTE_UPDATE_RETRY_PERIOD_MSEC, SERVER_TICK_PERIOD_MSEC, VOLATILITY, WRITE_TIMEOUT_MS,
};
use crate::errors::QuoteServerError;
use crate::quote_generator::QuoteGenerator;
use crate::stock_quote::StockQuote;

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct QuoteServer {
    quotes: Arc<RwLock<HashMap<String, StockQuote>>>,
    generator: QuoteGenerator,
}

impl QuoteServer {
    pub fn from_config<P: AsRef<std::path::Path>>(path: P) -> Result<Self, QuoteServerError> {
        let file = File::open(&path).map_err(|e| QuoteServerError::InvalidConfig(e.to_string()))?;
        let reader = BufReader::new(file);

        let mut hashmap = HashMap::new();

        for line in reader.lines() {
            let ticker = line
                .map_err(|e| QuoteServerError::InvalidConfig(e.to_string()))?
                .trim()
                .to_uppercase();
            if !ticker.is_empty() {
                let stock_quote = StockQuote::new(&ticker);
                hashmap.insert(ticker, stock_quote);
            }
        }

        let generator = QuoteGenerator::new(VOLATILITY).map_err(|_| {
            // todo: rework errors
            QuoteServerError::InitializationError("Failed to create Quote Generator".to_string())
        })?;

        Ok(QuoteServer {
            quotes: Arc::new(RwLock::new(hashmap)),
            generator,
        })
    }

    pub fn run_server(&mut self) -> Result<(), QuoteServerError> {
        loop {
            thread::sleep(Duration::from_millis(SERVER_TICK_PERIOD_MSEC));

            if let Err(e) = self.update_quotes() {
                return Err(e);
            }
        }
    }

    fn update_quotes(&mut self) -> Result<(), QuoteServerError> {
        let start = Instant::now();
        let timeout = Duration::from_millis(WRITE_TIMEOUT_MS);

        while start.elapsed() < timeout {
            if let Ok(mut lock) = self.quotes.try_write() {
                self.generator
                    .update_quotes(lock.values_mut())
                    .map_err(|_| {
                        // todo: rework errors
                        QuoteServerError::UpdateQuoteError("Failed to update quotes".to_string())
                    })?;
                return Ok(());
            }
            thread::sleep(Duration::from_millis(QUOTE_UPDATE_RETRY_PERIOD_MSEC));
        }

        Err(QuoteServerError::UpdateQuoteError(format!(
            "Timeout after {:?}",
            timeout
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_from_config_valid_file() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "AAPL\nGOOG\nMSFT").unwrap();

        let server = QuoteServer::from_config(file.path());
        assert!(server.is_ok());
        let server = server.unwrap();
        let quotes = server.quotes.read().unwrap();
        assert!(quotes.contains_key("AAPL"));
        assert!(quotes.contains_key("GOOG"));
        assert!(quotes.contains_key("MSFT"));
    }

    #[test]
    fn test_from_config_invalid_file() {
        let server = QuoteServer::from_config("nonexistent_file.txt");
        assert!(server.is_err());
        match server.unwrap_err() {
            QuoteServerError::InvalidConfig(_) => {}
            _ => panic!("Expected InvalidConfig error"),
        }
    }

    #[test]
    fn test_update_quotes_success() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "AAPL").unwrap();
        let mut server = QuoteServer::from_config(file.path()).unwrap();
        let result = server.update_quotes();
        assert!(result.is_ok());
    }

    #[test]
    fn test_update_quotes_timeout() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "AAPL").unwrap();
        let mut server = QuoteServer::from_config(file.path()).unwrap();

        let quotes_arc = Arc::clone(&server.quotes);
        let (lock_tx, lock_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();

        let handle = std::thread::spawn(move || {
            let _lock = quotes_arc.write().unwrap();
            lock_tx.send(()).unwrap(); // acquire lock

            // this thread holds lock, so update_quotes() should fail
            release_rx.recv().unwrap();
        });

        lock_rx.recv().unwrap();

        let result = server.update_quotes();
        assert!(result.is_err());

        match result.unwrap_err() {
            QuoteServerError::UpdateQuoteError(_) => {}
            _ => panic!("Expected UpdateQuoteError"),
        }

        // release lock
        release_tx.send(()).unwrap();
        handle.join().unwrap();
    }
}
