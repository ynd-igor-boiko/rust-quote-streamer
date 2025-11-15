use crate::errors::ClientError;
use crate::stock_quote::StockQuote;

use serde_json::json;
use std::collections::HashMap;
use std::fmt;
use std::sync::{
    Arc, RwLock,
    mpsc::{Sender, channel},
};
use std::thread;

/// Commands sent from the server to a client thread
#[derive(Clone, Debug)]
pub enum ClientCommand {
    /// Trigger a callback execution (i.e. process updated quotes)
    Update,
    /// Gracefully stop the client loop
    Shutdown,
}

/// A client that listens for real-time stock quote updates.
/// Each client subscribes to a fixed set of tickers.
/// For every update command, the client:
///   1. Reads the shared quotes map
///   2. Extracts only its own tickers
///   3. Serializes each into a JSON message
///   4. Calls the provided callback function
pub struct Client {
    /// List of ticker symbols the client is subscribed to
    pub tickers: Vec<String>,

    /// Shared stock quote map updated by QuoteServer
    quotes: Arc<RwLock<HashMap<String, StockQuote>>>,

    /// Sender to control the client loop (send Update/Shutdown)
    tx: Option<Sender<ClientCommand>>,

    /// Handle to the background thread running the client loop.
    /// Stored internally to allow joining the thread when shutting down.
    thread_handle: Option<thread::JoinHandle<()>>,

    /// User-provided callback executed for each ticker update.
    /// The callback receives JSON string and must return Result<(), ClientError>.
    callback: Arc<dyn Fn(String) -> Result<(), ClientError> + Send + Sync + 'static>,
}

impl Client {
    /// Create a new client instance.
    /// `tx` will be initialized when `start` is called.
    pub fn new(
        tickers: Vec<String>,
        quotes: Arc<RwLock<HashMap<String, StockQuote>>>,
        callback: impl Fn(String) -> Result<(), ClientError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            tickers,
            quotes,
            tx: None,
            thread_handle: None,
            callback: Arc::new(callback),
        }
    }

    /// Start the client loop in a background thread.
    ///
    /// The loop will listen for commands (`Update` or `Shutdown`) and process
    /// updates accordingly. This method creates a channel internally:
    /// - `tx` is stored in the struct to allow sending commands
    /// - `rx` is moved into the spawned thread
    ///
    /// Returns the thread JoinHandle.
    pub fn start(&mut self) -> Result<(), ClientError> {
        let (tx, rx) = channel();
        self.tx = Some(tx.clone());

        let tickers = self.tickers.clone();
        let quotes = self.quotes.clone();
        let callback = self.callback.clone();

        let handle = thread::spawn(move || {
            // Loop forever until Shutdown or channel error
            loop {
                match rx.recv() {
                    Ok(ClientCommand::Update) => {
                        if let Ok(all_quotes) = quotes.read() {
                            for ticker in &tickers {
                                if let Some(q) = all_quotes.get(ticker) {
                                    let msg = json!({
                                        "ticker": q.ticker,
                                        "price": q.price,
                                        "volume": q.volume,
                                        "timestamp": q.timestamp,
                                    });
                                    let _ = (callback)(msg.to_string());
                                }
                            }
                        }
                    }
                    Ok(ClientCommand::Shutdown) | Err(_) => break,
                }
            }
        });
        self.thread_handle = Some(handle);
        Ok(())
    }

    /// Send a command to the client loop.
    /// If the command is `Shutdown`, this method will also wait for the
    /// background thread to finish and clean up internal resources.
    pub fn send(&mut self, cmd: ClientCommand) -> Result<(), ClientError> {
        match &self.tx {
            Some(tx) => {
                tx.send(cmd.clone()).map_err(|e| {
                    ClientError::InitializationError(format!("Failed to send command: {}", e))
                })?;

                if matches!(cmd, ClientCommand::Shutdown) {
                    // Wait for thread to finish and clean up
                    if let Some(handle) = self.thread_handle.take() {
                        let _ = handle.join();
                    }
                    self.tx = None; // Prevent further sends
                }

                Ok(())
            }
            None => Err(ClientError::InitializationError(
                "Client loop not started or already shutdown".into(),
            )),
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(ClientCommand::Shutdown);
        }

        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("tickers", &self.tickers)
            .field("quotes", &self.quotes)
            .field("tx", &self.tx)
            .field("thread_handle", &self.thread_handle)
            // exclude callback
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_client_receives_updates_and_calls_callback() {
        let mut map: HashMap<String, StockQuote> = HashMap::new();
        let mut single_quote = StockQuote::new("AAPL");
        single_quote.price = 150.0;
        single_quote.volume = 1000;
        single_quote.timestamp = 123456;
        map.insert("AAPL".into(), single_quote);

        let quotes = Arc::new(RwLock::new(map));
        let output = Arc::new(Mutex::new(Vec::<String>::new()));
        let out_clone = output.clone();

        let mut client = Client::new(vec!["AAPL".into()], quotes.clone(), move |json| {
            out_clone.lock().unwrap().push(json);
            Ok(())
        });

        let _handle = client.start().unwrap();

        client.send(ClientCommand::Update).unwrap();
        thread::sleep(Duration::from_millis(50));

        let result = output.lock().unwrap();
        assert_eq!(result.len(), 1);
        let msg = &result[0];
        assert!(msg.contains("\"ticker\":\"AAPL\""));
        assert!(msg.contains("\"price\":150.0"));
        assert!(msg.contains("\"volume\":1000"));
        assert!(msg.contains("\"timestamp\":123456"));

        client.send(ClientCommand::Shutdown).unwrap();
    }

    #[test]
    fn test_client_filters_unrelated_tickers() {
        let mut map = HashMap::new();
        map.insert("AAPL".into(), StockQuote::new("AAPL"));
        map.insert("MSFT".into(), StockQuote::new("MSFT"));

        let quotes = Arc::new(RwLock::new(map));
        let output = Arc::new(Mutex::new(Vec::<String>::new()));
        let out_clone = output.clone();

        let mut client = Client::new(vec!["MSFT".into()], quotes.clone(), move |json| {
            out_clone.lock().unwrap().push(json);
            Ok(())
        });

        let _handle = client.start().unwrap();
        client.send(ClientCommand::Update).unwrap();
        thread::sleep(Duration::from_millis(50));

        let result = output.lock().unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].contains("\"ticker\":\"MSFT\""));

        client.send(ClientCommand::Shutdown).unwrap();
    }

    #[test]
    fn test_client_shutdown() {
        let quotes = Arc::new(RwLock::new(HashMap::new()));
        let mut client = Client::new(vec![], quotes.clone(), |_json| Ok(()));

        let _handle = client.start().unwrap();
        client.send(ClientCommand::Shutdown).unwrap();
        thread::sleep(Duration::from_millis(20));

        assert!(true);
    }
}
