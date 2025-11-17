use std::time::{SystemTime, UNIX_EPOCH};

/// Represents a single real-time stock quote.
///
/// A `StockQuote` contains the current price, trading volume,
/// last update timestamp, and the stock's ticker symbol.
///
/// This structure is used throughout the system as the core payload
/// transmitted to subscribed clients.
#[derive(Clone, Debug)]
pub struct StockQuote {
    /// Stock ticker symbol (e.g., `"AAPL"`, `"TSLA"`).
    pub ticker: String,

    /// Current stock price at the moment of the latest update.
    pub price: f64,

    /// Trading volume associated with this quote.
    ///
    /// The meaning of this field may vary depending on the quote generator
    /// (e.g., cumulative volume, random simulated spikes, etc.).
    pub volume: u32,

    /// Timestamp of the last update, measured in **milliseconds** since the UNIX epoch.
    pub timestamp: u64,
}

impl StockQuote {
    /// Creates a new quote with default/randomized values.
    ///
    /// - `ticker` — the stock symbol this quote represents.
    /// - `price` — initialized to a random value within `[0, 1000)`.
    /// - `volume` — initialized to `0`.
    /// - `timestamp` — set to the current system time.
    ///
    /// # Examples
    ///
    /// ```
    /// use quote_server::stock_quote::StockQuote;
    /// let quote = StockQuote::new("AAPL");
    /// assert_eq!(quote.ticker, "AAPL");
    /// ```
    pub fn new(ticker: &str) -> Self {
        StockQuote {
            ticker: ticker.to_string(),
            price: rand::random::<f64>() * 1000.0,
            volume: 0,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }
    }

    /// Serializes the quote into a compact `ticker|price|volume|timestamp` text format.
    ///
    /// This string format is used by the UDP streaming subsystem.
    ///
    /// # Example
    ///
    /// ```
    /// use quote_server::stock_quote::StockQuote;
    /// let q = StockQuote::new("AAPL");
    /// let s = q.to_string();
    /// assert!(s.starts_with("AAPL|"));
    /// ```
    pub fn to_string(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.ticker, self.price, self.volume, self.timestamp
        )
    }

    /// Parses a `StockQuote` from its serialized textual format.
    ///
    /// Returns `None` if the input string does not contain exactly four fields
    /// or if numeric fields fail to parse.
    ///
    /// # Format
    ///
    /// ```text
    /// ticker|price|volume|timestamp
    /// ```
    ///
    /// # Example
    ///
    /// ```
    /// use quote_server::stock_quote::StockQuote;
    /// let raw = "AAPL|123.45|100|1700000000000";
    /// let quote = StockQuote::from_string(raw).unwrap();
    /// assert_eq!(quote.ticker, "AAPL");
    /// ```
    pub fn from_string(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('|').collect();
        if parts.len() == 4 {
            Some(StockQuote {
                ticker: parts[0].to_string(),
                price: parts[1].parse().ok()?,
                volume: parts[2].parse().ok()?,
                timestamp: parts[3].parse().ok()?,
            })
        } else {
            None
        }
    }

    /// Serializes the quote into raw bytes using the same `|`-separated format
    /// as [`to_string`](Self::to_string).
    ///
    /// This is intended for fast UDP transmission without allocating intermediate strings.
    ///
    /// # Example
    ///
    /// ```
    /// use quote_server::stock_quote::StockQuote;
    /// let q = StockQuote::new("AAPL");
    /// let bytes = q.to_bytes();
    /// assert!(bytes.contains(&b'|'));
    /// ```
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(self.ticker.as_bytes());
        bytes.push(b'|');
        bytes.extend_from_slice(self.price.to_string().as_bytes());
        bytes.push(b'|');
        bytes.extend_from_slice(self.volume.to_string().as_bytes());
        bytes.push(b'|');
        bytes.extend_from_slice(self.timestamp.to_string().as_bytes());
        bytes
    }
}
