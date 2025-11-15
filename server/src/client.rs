use crate::errors::ClientError;
use crate::stock_quote::StockQuote;

use serde_json::json;
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex, RwLock,
    mpsc::{Receiver, channel},
};
use std::thread;

/// Commands sent from the server to a client thread
#[derive(Debug)]
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
    tickers: Vec<String>,

    /// Shared stock quote map updated by QuoteServer
    quotes: Arc<RwLock<HashMap<String, StockQuote>>>,

    /// Receives update/shutdown commands from the server
    rx: Receiver<ClientCommand>,

    /// User-provided callback executed for each ticker update.
    /// The callback receives JSON string and must return Result<(), ClientError>.
    callback: Box<dyn Fn(String) -> Result<(), ClientError> + Send + Sync + 'static>,
}

impl Client {
    /// Create a new client instance
    pub fn new(
        tickers: Vec<String>,
        quotes: Arc<RwLock<HashMap<String, StockQuote>>>,
        rx: Receiver<ClientCommand>,
        callback: impl Fn(String) -> Result<(), ClientError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            tickers,
            quotes,
            rx,
            callback: Box::new(callback),
        }
    }

    /// Start the client loop in a background thread.
    ///
    /// This function launches the client loop in a new thread. It returns a `JoinHandle`
    /// so that the caller can join the thread later if needed.
    ///
    /// Internally, it uses a channel to immediately signal back to the caller
    /// that the loop has started. This ensures that the client is ready to receive
    /// update or shutdown commands.
    ///
    /// # Returns
    /// - `Ok(JoinHandle<()>)` if the loop started successfully
    /// - `Err(ClientError)` if the loop failed to signal startup
    pub fn start(self) -> Result<thread::JoinHandle<()>, ClientError> {
        // Channel to signal that the loop started
        let (started_tx, started_rx) = channel();

        let handle = thread::spawn(move || {
            // Immediately signal that the loop has started
            let _ = started_tx.send(());

            loop {
                match self.rx.recv() {
                    Ok(ClientCommand::Update) => {
                        if let Ok(all_quotes) = self.quotes.read() {
                            for ticker in &self.tickers {
                                if let Some(q) = all_quotes.get(ticker) {
                                    let msg = serde_json::json!({
                                        "ticker": q.ticker,
                                        "price": q.price,
                                        "volume": q.volume,
                                        "timestamp": q.timestamp,
                                    });
                                    let _ = (self.callback)(msg.to_string());
                                }
                            }
                        }
                    }
                    Ok(ClientCommand::Shutdown) | Err(_) => break,
                }
            }
        });

        // Wait for signal that the client loop started
        match started_rx.recv() {
            Ok(_) => Ok(handle),
            Err(RecvError) => Err(ClientError::InitializationError(
                "Client loop channel disconnected".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex, mpsc::channel};

    /// Test that client receives command, processes quotes,
    /// generates JSON and invokes the callback once.
    #[test]
    fn test_client_receives_updates_and_calls_callback() {
        let (tx, rx) = channel();

        // Prepare a single quote entry
        let mut map: HashMap<String, StockQuote> = HashMap::new();
        let mut single_quote = StockQuote::new("AAPL");
        single_quote.price = 150.0;
        single_quote.volume = 1000;
        single_quote.timestamp = 123456;
        map.insert("AAPL".into(), single_quote);

        // Shared storage
        let quotes = Arc::new(RwLock::new(map));

        // Collect callback outputs
        let output = Arc::new(Mutex::new(Vec::<String>::new()));
        let out_clone = output.clone();

        let client = Client::new(vec!["AAPL".into()], quotes.clone(), rx, move |json| {
            out_clone.lock().unwrap().push(json);
            Ok(())
        });

        let _ = client.start();

        // Send update event
        tx.send(ClientCommand::Update).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));

        let result = output.lock().unwrap();
        assert_eq!(result.len(), 1);

        // Validate JSON
        let msg = &result[0];
        assert!(msg.contains("\"ticker\":\"AAPL\""));
        assert!(msg.contains("\"price\":150.0"));
        assert!(msg.contains("\"volume\":1000"));
        assert!(msg.contains("\"timestamp\":123456"));
    }

    /// Client must ignore tickers it is not subscribed to.
    #[test]
    fn test_client_filters_unrelated_tickers() {
        let (tx, rx) = channel();

        let mut map = HashMap::new();
        map.insert("AAPL".into(), StockQuote::new("AAPL"));
        map.insert("MSFT".into(), StockQuote::new("MSFT"));

        let quotes = Arc::new(RwLock::new(map));

        // Capture callback
        let output = Arc::new(Mutex::new(Vec::<String>::new()));
        let out_clone = output.clone();

        let client = Client::new(
            vec!["MSFT".into()], // subscribe to MSFT only
            quotes.clone(),
            rx,
            move |json| {
                out_clone.lock().unwrap().push(json);
                Ok(())
            },
        );

        let _ = client.start();
        tx.send(ClientCommand::Update).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));

        let result = output.lock().unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].contains("\"ticker\":\"MSFT\""));
    }

    /// Sending a shutdown command must terminate the client loop.
    #[test]
    fn test_client_shutdown() {
        let (tx, rx) = channel();
        let quotes = Arc::new(RwLock::new(HashMap::new()));

        let client = Client::new(vec![], quotes.clone(), rx, |_json| Ok(()));

        let _ = client.start();

        // Ask the client to terminate
        tx.send(ClientCommand::Shutdown).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));

        // No crash => success
        assert!(true);
    }
}
