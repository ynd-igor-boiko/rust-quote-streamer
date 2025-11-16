use crate::client::{Client, ClientCommand};
use crate::defs::{
    QUOTE_UPDATE_RETRY_PERIOD_MSEC, SERVER_TICK_PERIOD_MSEC, VOLATILITY, WRITE_TIMEOUT_MS,
};
use crate::errors::{ClientError, QuoteServerError};
use crate::quote_generator::QuoteGenerator;
use crate::stock_quote::StockQuote;

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

/// QuoteServer holds quotes, a generator, and multiple clients.
#[derive(Debug)]
pub struct QuoteServer {
    quotes: Arc<RwLock<HashMap<String, StockQuote>>>,
    generator: QuoteGenerator,
    clients: Mutex<HashMap<u64, Client>>,
    clients_count: AtomicU64,
}

impl QuoteServer {
    /// Load server configuration from file (tickers one per line)
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
                hashmap.insert(ticker.clone(), StockQuote::new(&ticker));
            }
        }

        let generator = QuoteGenerator::new(VOLATILITY).map_err(|_| {
            QuoteServerError::InitializationError("Failed to create Quote Generator".to_string())
        })?;

        Ok(Self {
            quotes: Arc::new(RwLock::new(hashmap)),
            generator,
            clients: Mutex::new(HashMap::new()),
            clients_count: AtomicU64::new(0),
        })
    }

    /// Run the server loop that updates quotes periodically
    pub fn run_server(&mut self) -> Result<(), QuoteServerError> {
        loop {
            thread::sleep(Duration::from_millis(SERVER_TICK_PERIOD_MSEC));

            self.update_quotes()?;
            self.notify_clients()?;
        }
    }

    /// Add a new client
    pub fn add_client(
        &self,
        tickers: Vec<String>,
        callback: impl Fn(String) -> Result<(), ClientError> + Send + Sync + 'static,
    ) -> Result<u64, QuoteServerError> {
        let mut clients_lock = self
            .clients
            .lock()
            .map_err(|_| QuoteServerError::ClientError("Failed to lock clients".into()))?;
        let mut client = Client::new(tickers, self.quotes.clone(), callback);
        client.start()?;

        let id = self.clients_count.fetch_add(1, Ordering::SeqCst);
        clients_lock.insert(id, client);
        Ok(id)
    }

    /// Remove a client by id
    pub fn remove_client(&self, id: u64) -> Result<(), QuoteServerError> {
        let mut clients_lock = self
            .clients
            .lock()
            .map_err(|_| QuoteServerError::ClientError("Failed to lock clients".into()))?;
        if let Some(mut client) = clients_lock.remove(&id) {
            client.send(ClientCommand::Shutdown)?;
        }
        Ok(())
    }

    /// Update all quotes
    fn update_quotes(&mut self) -> Result<(), QuoteServerError> {
        let start = Instant::now();
        let timeout = Duration::from_millis(WRITE_TIMEOUT_MS);

        while start.elapsed() < timeout {
            if let Ok(mut lock) = self.quotes.try_write() {
                self.generator
                    .update_quotes(lock.values_mut())
                    .map_err(|_| {
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

    fn notify_clients(&mut self) -> Result<(), QuoteServerError> {
        // Notify all clients
        let mut clients = self
            .clients
            .lock()
            .map_err(|_| QuoteServerError::UpdateQuoteError("Failed to lock clients".into()))?;
        for client in clients.values_mut() {
            client
                .send(ClientCommand::Update)
                .map_err(|e| QuoteServerError::UpdateQuoteError(e.to_string()))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;
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
    fn test_add_and_remove_client() -> Result<(), Box<dyn std::error::Error>> {
        let mut file = NamedTempFile::new()?;
        writeln!(file, "AAPL")?;
        let mut server = QuoteServer::from_config(file.path())?;

        let output = Arc::new(Mutex::new(Vec::<String>::new()));
        let output_clone = output.clone();

        let client_id = server.add_client(vec!["AAPL".into()], move |json| {
            output_clone.lock().unwrap().push(json);
            Ok(())
        })?;

        server.update_quotes()?;
        server.notify_clients()?;

        thread::sleep(Duration::from_millis(50));
        let messages = output.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("\"ticker\":\"AAPL\""));

        server.remove_client(client_id)?;
        Ok(())
    }
}
