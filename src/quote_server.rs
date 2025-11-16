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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// QuoteServer holds quotes, a generator, and multiple clients.
#[derive(Debug)]
pub struct QuoteServer {
    quotes: Arc<RwLock<HashMap<String, StockQuote>>>,
    generator: QuoteGenerator,
    clients: Mutex<HashMap<u64, Client>>,
    clients_count: AtomicU64,

    /// Background thread
    bg_thread: Mutex<Option<JoinHandle<()>>>,
    /// Graceful shutdown flag
    shutdown_flag: Arc<AtomicBool>,
}

impl QuoteServer {
    /// Load server configuration from file (tickers one per line)
    pub fn from_config<P: AsRef<std::path::Path>>(path: P) -> Result<Self, QuoteServerError> {
        log::info!(
            "Loading QuoteServer configuration from: {:?}",
            path.as_ref()
        );
        let file = File::open(&path).map_err(|e| QuoteServerError::InvalidConfig(e.to_string()))?;
        let reader = BufReader::new(file);

        let mut hashmap = HashMap::new();
        let mut ticker_count = 0;

        for line in reader.lines() {
            let ticker = line
                .map_err(|e| QuoteServerError::InvalidConfig(e.to_string()))?
                .trim()
                .to_uppercase();
            if !ticker.is_empty() {
                hashmap.insert(ticker.clone(), StockQuote::new(&ticker));
                ticker_count += 1;
            }
        }

        log::info!("Loaded {} tickers from configuration", ticker_count);

        let generator = QuoteGenerator::new(VOLATILITY).map_err(|_| {
            QuoteServerError::InitializationError("Failed to create Quote Generator".to_string())
        })?;

        log::info!(
            "QuoteServer initialized successfully with volatility: {}",
            VOLATILITY
        );

        Ok(Self {
            quotes: Arc::new(RwLock::new(hashmap)),
            generator,
            clients: Mutex::new(HashMap::new()),
            clients_count: AtomicU64::new(0),
            bg_thread: Mutex::new(None),
            shutdown_flag: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Run the server loop that updates quotes periodically
    pub fn start(self: &Arc<Self>) -> Result<(), QuoteServerError> {
        let mut guard = self.bg_thread.lock().unwrap();

        if guard.is_some() {
            log::warn!("QuoteServer background thread already running");
            return Ok(()); // already running
        }

        let server = Arc::clone(self);

        log::info!(
            "Starting QuoteServer background thread with tick period: {}ms",
            SERVER_TICK_PERIOD_MSEC
        );

        let handle = std::thread::spawn(move || {
            log::info!("QuoteServer background thread started");
            while !server.shutdown_flag.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(SERVER_TICK_PERIOD_MSEC));

                if let Err(e) = server.update_quotes() {
                    log::error!("Failed to update quotes: {}", e);
                }
                if let Err(e) = server.notify_clients() {
                    log::error!("Failed to notify clients: {}", e);
                }
            }
            log::info!("QuoteServer background thread stopped");
        });

        *guard = Some(handle);

        Ok(())
    }

    /// Signal the server to shut down and join the thread
    pub fn shutdown(&self) {
        log::info!("Initiating QuoteServer shutdown");
        self.shutdown_flag.store(true, Ordering::SeqCst);

        if let Some(handle) = self.bg_thread.lock().unwrap().take() {
            log::debug!("Waiting for QuoteServer background thread to finish");
            handle.join().ok();
            log::info!("QuoteServer background thread joined successfully");
        } else {
            log::warn!("No background thread found to shutdown");
        }
    }

    /// Add a new client
    pub fn add_client(
        &self,
        tickers: Vec<String>,
        callback: impl Fn(String) -> Result<(), ClientError> + Send + Sync + 'static,
    ) -> Result<u64, QuoteServerError> {
        log::debug!("Adding new client with tickers: {:?}", tickers);
        let mut clients_lock = self
            .clients
            .lock()
            .map_err(|_| QuoteServerError::ClientError("Failed to lock clients".into()))?;
        let mut client = Client::new(tickers, self.quotes.clone(), callback);
        client.start()?;

        let id = self.clients_count.fetch_add(1, Ordering::SeqCst);
        clients_lock.insert(id, client);

        let client_count = clients_lock.len();
        log::info!(
            "Client {} added successfully. Total clients: {}",
            id,
            client_count
        );
        Ok(id)
    }

    /// Remove a client by id
    pub fn remove_client(&self, id: u64) -> Result<(), QuoteServerError> {
        log::debug!("Removing client: {}", id);
        let mut clients_lock = self
            .clients
            .lock()
            .map_err(|_| QuoteServerError::ClientError("Failed to lock clients".into()))?;
        if let Some(mut client) = clients_lock.remove(&id) {
            log::debug!("Sending shutdown command to client: {}", id);
            client.send(ClientCommand::Shutdown)?;
            log::info!("Client {} removed successfully", id);
        } else {
            log::warn!("Attempted to remove non-existent client: {}", id);
        }

        let client_count = clients_lock.len();
        log::debug!("Total clients after removal: {}", client_count);
        Ok(())
    }

    /// Update all quotes
    fn update_quotes(&self) -> Result<(), QuoteServerError> {
        let start = Instant::now();
        let timeout = Duration::from_millis(WRITE_TIMEOUT_MS);

        log::debug!(
            "Attempting to update quotes (timeout: {}ms)",
            WRITE_TIMEOUT_MS
        );

        while start.elapsed() < timeout {
            if let Ok(mut lock) = self.quotes.try_write() {
                let quote_count = lock.len();
                self.generator
                    .update_quotes(lock.values_mut())
                    .map_err(|_| {
                        QuoteServerError::UpdateQuoteError("Failed to update quotes".to_string())
                    })?;
                log::debug!("Successfully updated {} quotes", quote_count);
                return Ok(());
            }
            log::trace!(
                "Quotes lock busy, retrying in {}ms",
                QUOTE_UPDATE_RETRY_PERIOD_MSEC
            );
            thread::sleep(Duration::from_millis(QUOTE_UPDATE_RETRY_PERIOD_MSEC));
        }

        log::error!("Timeout updating quotes after {:?}", timeout);
        Err(QuoteServerError::UpdateQuoteError(format!(
            "Timeout after {:?}",
            timeout
        )))
    }

    fn notify_clients(&self) -> Result<(), QuoteServerError> {
        // Notify all clients
        let mut clients = self
            .clients
            .lock()
            .map_err(|_| QuoteServerError::UpdateQuoteError("Failed to lock clients".into()))?;

        let client_count = clients.len();
        log::debug!("Notifying {} clients about quote updates", client_count);

        for (client_id, client) in clients.iter_mut() {
            if let Err(e) = client.send(ClientCommand::Update) {
                log::warn!("Failed to send update to client {}: {}", client_id, e);
            }
        }

        log::trace!("Successfully notified {} clients", client_count);
        Ok(())
    }
}

impl Drop for QuoteServer {
    fn drop(&mut self) {
        log::debug!("QuoteServer drop called, initiating shutdown");
        let _ = self.shutdown(); // graceful shutdown
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
