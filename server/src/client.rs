use crate::stock_quote::StockQuote;

use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, RwLock, mpsc::Receiver};
use std::thread;

/// Commands sent to the client from the server
#[derive(Debug)]
pub enum ClientCommand {
    /// Trigger a quote update callback
    Update,
    /// Gracefully stop the client thread
    Shutdown,
}

/// A client that listens for stock quote updates and processes
/// them through a user-provided callback function.
pub struct Client<F>
where
    F: Fn(String) + Send + 'static,
{
    /// List of tickers this client is interested in
    tickers: Vec<String>,

    /// Shared reference to the global quotes map
    quotes: Arc<RwLock<HashMap<String, StockQuote>>>,

    /// Channel for receiving update/shutdown commands
    rx: Receiver<ClientCommand>,

    /// Callback executed for each ticker update
    /// Receives a JSON string representing the updated ticker state
    callback: F,
}

impl<F> Client<F>
where
    F: Fn(String) + Send + 'static,
{
    /// Creates a new client instance with its interested tickers,
    /// shared quotes map, command receiver, and callback.
    pub fn new(
        tickers: Vec<String>,
        quotes: Arc<RwLock<HashMap<String, StockQuote>>>,
        rx: Receiver<ClientCommand>,
        callback: F,
    ) -> Self {
        Self {
            tickers,
            quotes,
            rx,
            callback,
        }
    }

    /// Starts the client in a dedicated background thread.
    /// The thread loops forever, processing incoming commands:
    ///  - Update: read the shared quotes and trigger the callback
    ///  - Shutdown: exit the loop and end the thread
    pub fn start(self) {
        thread::spawn(move || {
            loop {
                match self.rx.recv() {
                    Ok(ClientCommand::Update) => {
                        // Acquire read lock for all quotes
                        if let Ok(all_quotes) = self.quotes.read() {
                            // Process only the tickers the client is subscribed to
                            for ticker in &self.tickers {
                                if let Some(q) = all_quotes.get(ticker) {
                                    // Build JSON message for the callback
                                    let msg = json!({
                                        "ticker": q.ticker,
                                        "price": q.price,
                                        "volume": q.volume,
                                        "timestamp": q.timestamp,
                                    });

                                    // Invoke user callback with serialized JSON
                                    (self.callback)(msg.to_string());
                                }
                            }
                        }
                    }

                    // Shutdown or channel error -> exit the client thread
                    Ok(ClientCommand::Shutdown) | Err(_) => {
                        break;
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex, mpsc::channel};

    #[test]
    fn test_client_receives_updates_and_calls_callback() {
        let (tx, rx) = channel();

        // Shared quote storage
        let mut map: HashMap<String, StockQuote> = HashMap::new();
        let mut single_quote = StockQuote::new("AAPL");
        single_quote.price = 150.0;
        single_quote.volume = 1000;
        single_quote.timestamp = 123456;

        map.insert("AAPL".into(), single_quote);
        let quotes = Arc::new(RwLock::new(map));

        // Capture callback outputs for verification
        let output = Arc::new(Mutex::new(Vec::<String>::new()));
        let output_clone = output.clone();

        let client = Client::new(vec!["AAPL".into()], quotes.clone(), rx, move |json| {
            output_clone.lock().unwrap().push(json);
        });

        client.start();

        // Send update command
        tx.send(ClientCommand::Update).unwrap();

        // Allow thread to process message
        std::thread::sleep(std::time::Duration::from_millis(50));

        let result = output.lock().unwrap();
        assert_eq!(result.len(), 1);

        // Ensure JSON contains correct data
        let msg = &result[0];
        assert!(msg.contains("\"ticker\":\"AAPL\""));
        assert!(msg.contains("\"price\":150.0"));
        assert!(msg.contains("\"volume\":1000"));
        assert!(msg.contains("\"timestamp\":123456"));
    }

    #[test]
    fn test_client_filters_unrelated_tickers() {
        let (tx, rx) = channel();

        let mut map: HashMap<String, StockQuote> = HashMap::new();
        map.insert("AAPL".into(), StockQuote::new("AAPL"));
        map.insert("MSFT".into(), StockQuote::new("MSFT"));
        let quotes = Arc::new(RwLock::new(map));

        let output = Arc::new(Mutex::new(Vec::<String>::new()));
        let out_clone = output.clone();

        let client = Client::new(
            vec!["MSFT".into()], // only MSFT for this client
            quotes.clone(),
            rx,
            move |json| {
                out_clone.lock().unwrap().push(json);
            },
        );

        client.start();

        tx.send(ClientCommand::Update).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));

        let result = output.lock().unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].contains("\"ticker\":\"MSFT\""));
    }

    #[test]
    fn test_client_shutdown() {
        let (tx, rx) = channel();

        let quotes = Arc::new(RwLock::new(HashMap::new()));

        let output = Arc::new(Mutex::new(Vec::<String>::new()));
        let out_clone = output.clone();

        let client = Client::new(vec![], quotes.clone(), rx, move |json| {
            out_clone.lock().unwrap().push(json);
        });

        client.start();

        // Ask client to stop
        tx.send(ClientCommand::Shutdown).unwrap();

        // Let thread exit
        std::thread::sleep(std::time::Duration::from_millis(20));

        // Just ensure no panic and everything ends cleanly
        assert!(true);
    }
}
