use crate::defs::{CLIENT_KEEP_ALIVE_SEC, TCP_CONNECTION_TICK_PERIOD_MSEC};
use crate::errors::TcpServerError;
use crate::quote_server::QuoteServer;

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// TCP server that accepts client commands for streaming stock quotes or stopping a stream.
///
/// Supported commands:
/// - `PING` → responds with `PONG`
/// - `STREAM host:port TICKER...` → creates a client in the `QuoteServer` and sends data via UDP
/// - `STOP` → removes the client
pub struct TcpServer {
    /// TCP listener socket
    listener: TcpListener,

    /// Thread-safe reference to `QuoteServer`
    quote_server: Arc<QuoteServer>,
}

impl TcpServer {
    /// Creates a new TCP server bound to the given address.
    ///
    /// # Arguments
    /// * `addr` - Address to bind, e.g., `"127.0.0.1:3333"`.
    /// * `quote_server` - `Arc` reference to a `QuoteServer` instance.
    ///
    /// # Returns
    /// * `Ok(TcpServer)` if binding succeeds.
    /// * `Err(TcpServerError::BindError)` if the port is unavailable.
    pub fn new(addr: &str, quote_server: Arc<QuoteServer>) -> Result<Self, TcpServerError> {
        log::info!("Binding TCP server to address: {}", addr);
        let listener =
            TcpListener::bind(addr).map_err(|e| TcpServerError::BindError(e.to_string()))?;
        log::info!("TCP server successfully bound to: {}", addr);

        Ok(Self {
            listener,
            quote_server,
        })
    }

    /// Starts the TCP server in an infinite loop.
    ///
    /// For every incoming client connection, spawns a dedicated thread
    /// to handle the connection. Each thread handles keep-alive and
    /// dispatches commands (`PING`, `STREAM`, `STOP`).
    pub fn start(&self) -> Result<(), TcpServerError> {
        log::info!("TCP server starting main loop");
        loop {
            match self.listener.accept() {
                Ok((stream, addr)) => {
                    log::info!("New TCP connection from: {}", addr);
                    let qs = self.quote_server.clone();
                    thread::spawn(move || {
                        log::debug!("Spawning handler thread for client: {}", addr);
                        if let Err(e) = handle_connection(stream, addr, qs) {
                            log::warn!("Connection handler error for {}: {}", addr, e);
                        }
                        log::debug!("Handler thread finished for client: {}", addr);
                    });
                }
                Err(e) => {
                    log::error!("Failed to accept TCP connection: {}", e);
                    return Err(TcpServerError::AcceptError(e.to_string()));
                }
            }
        }
    }
}

/// Handles a single client TCP connection.
///
/// - Reads commands from TCP stream.
/// - Responds to `PING`, `STREAM`, and `STOP`.
/// - Tracks keep-alive; disconnects client if no PING received within timeout.
fn handle_connection(
    mut stream: TcpStream,
    addr: SocketAddr,
    quote_server: Arc<QuoteServer>,
) -> Result<(), TcpServerError> {
    log::info!("[tcp] connected: {}", addr);

    let mut client_id: Option<u64> = None;
    let mut last_ping = Instant::now();

    let tick_timeout = Duration::from_millis(TCP_CONNECTION_TICK_PERIOD_MSEC);
    // Clone the stream for buffered, line-based reading
    let cloned = stream
        .try_clone()
        .map_err(|e| TcpServerError::ClientIoError(e.to_string()))?;

    // Also set the same read timeout on the cloned handle used by BufReader.
    // Some platforms treat timeouts per-handle, so do it explicitly.
    cloned
        .set_read_timeout(Some(tick_timeout))
        .map_err(|e| TcpServerError::ClientIoError(e.to_string()))?;

    // Wrap the cloned stream in BufReader for line-based reading
    let mut reader = BufReader::new(cloned);

    loop {
        let mut line = String::new();

        match reader.read_line(&mut line) {
            Ok(0) => {
                log::info!("Client {} closed connection", addr);
                return Ok(());
            }
            Ok(_) => {
                last_ping = Instant::now();
                let msg = line.trim().to_string(); // trim removes trailing \n

                if msg.is_empty() {
                    continue; // ignore empty lines
                }

                log::debug!("Received from {}: '{}'", addr, msg);

                if msg.starts_with("PING") {
                    handle_ping(&mut stream, &addr)?;
                } else if msg.starts_with("STREAM ") {
                    handle_stream(&mut stream, msg, &quote_server, &mut client_id, &addr)?;
                } else if msg.starts_with("STOP") {
                    handle_stop(&mut stream, &quote_server, &mut client_id, &addr)?;
                } else {
                    handle_invalid(&mut stream, msg, &addr)?;
                }
            }
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                // Keep-alive check
                if last_ping.elapsed().as_secs() > CLIENT_KEEP_ALIVE_SEC {
                    log::warn!(
                        "Client {} keep-alive timeout ({}s), disconnecting",
                        addr,
                        CLIENT_KEEP_ALIVE_SEC
                    );
                    if let Some(id) = client_id.take() {
                        log::info!("Removing client {} due to timeout", id);
                        quote_server.remove_client(id)?;
                    }
                    return Ok(());
                }
                thread::sleep(Duration::from_millis(TCP_CONNECTION_TICK_PERIOD_MSEC));
                continue;
            }
            Err(e) => {
                log::error!("Connection failed for {}: {}", addr, e);
                if let Some(id) = client_id.take() {
                    log::info!("Removing client {} due to connection error", id);
                    quote_server.remove_client(id)?;
                }
                return Err(TcpServerError::ClientIoError(e.to_string()));
            }
        }
    }
}

/// Responds to a `PING` command with `PONG`.
fn handle_ping(stream: &mut TcpStream, addr: &SocketAddr) -> Result<(), TcpServerError> {
    log::debug!("Responding to PING from {}", addr);
    stream
        .write_all(b"PONG\n")
        .map_err(|e| TcpServerError::ClientIoError(e.to_string()))
}

/// Handles a `STREAM` command:
/// - Parses the UDP address and tickers.
/// - Creates a client in `QuoteServer`.
/// - The callback sends updates to the given UDP address.
fn handle_stream(
    stream: &mut TcpStream,
    msg: String,
    quote_server: &Arc<QuoteServer>,
    client_id: &mut Option<u64>,
    addr: &SocketAddr,
) -> Result<(), TcpServerError> {
    let parts: Vec<_> = msg.split_whitespace().collect();

    if parts.len() < 3 {
        log::warn!(
            "Invalid STREAM command from {}: insufficient parameters",
            addr
        );
        return Err(TcpServerError::InvalidCommand(
            "STREAM host:port TICKER...".into(),
        ));
    }

    let udp_addr: SocketAddr = parts[1].parse().map_err(|_| {
        log::warn!("Invalid UDP address from {}: {}", addr, parts[1]);
        TcpServerError::InvalidCommand("Bad UDP address".into())
    })?;

    let tickers = parts[2..].iter().map(|s| s.to_string()).collect::<Vec<_>>();
    log::info!(
        "STREAM request from {}: UDP {} tickers {:?}",
        addr,
        udp_addr,
        tickers
    );

    let udp_socket =
        UdpSocket::bind("0.0.0.0:0").map_err(|e| TcpServerError::ClientIoError(e.to_string()))?;
    let udp = Arc::new(udp_socket);

    let udp_clone = udp.clone();

    let callback = move |json: String| {
        let _ = udp_clone.send_to(json.as_bytes(), udp_addr);
        Ok(())
    };

    let id = quote_server
        .add_client(tickers, Box::new(callback))
        .map_err(|e| TcpServerError::QuoteServerError(e))?;

    *client_id = Some(id);
    log::info!("Registered client {} for TCP connection {}", id, addr);

    stream
        .write_all(b"STREAM OK\n")
        .map_err(|e| TcpServerError::ClientIoError(e.to_string()))
}

/// Handles a `STOP` command:
/// - Removes the client from `QuoteServer`.
/// - Sends confirmation to the TCP client.
fn handle_stop(
    stream: &mut TcpStream,
    quote_server: &Arc<QuoteServer>,
    client_id: &mut Option<u64>,
    addr: &SocketAddr,
) -> Result<(), TcpServerError> {
    if let Some(id) = client_id.take() {
        log::info!("STOP request from {}, removing client {}", addr, id);
        quote_server
            .remove_client(id)
            .map_err(|e| TcpServerError::QuoteServerError(e))?;
    } else {
        log::warn!("STOP request from {} but no client registered", addr);
    }

    stream
        .write_all(b"STOP OK\n")
        .map_err(|e| TcpServerError::ClientIoError(e.to_string()))
}

/// Sends an error message to the client for invalid commands.
fn handle_invalid(
    stream: &mut TcpStream,
    msg: String,
    addr: &SocketAddr,
) -> Result<(), TcpServerError> {
    log::warn!("Invalid command from {}: '{}'", addr, msg);
    stream
        .write_all(b"ERR Invalid command\n")
        .map_err(|e| TcpServerError::ClientIoError(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::thread;
    use std::time::Duration;
    use tempfile::NamedTempFile;

    /// Creates a test QuoteServer with a single stock.
    fn create_test_quote_server() -> Arc<QuoteServer> {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "AAPL").unwrap();
        let server = QuoteServer::from_config(file.path()).unwrap();
        Arc::new(server)
    }

    #[test]
    fn test_ping_pong() {
        let qs = create_test_quote_server();
        let server = TcpServer::new("127.0.0.1:33333", qs.clone()).unwrap();

        thread::spawn(move || {
            server.start().unwrap();
        });

        thread::sleep(Duration::from_millis(100));

        let mut stream = TcpStream::connect("127.0.0.1:33333").unwrap();
        stream.write_all(b"PING\n").unwrap();

        let mut buf = [0u8; 128];
        let n = stream.read(&mut buf).unwrap();
        let s = String::from_utf8_lossy(&buf[..n]);

        assert_eq!(s.trim(), "PONG");
    }

    #[test]
    fn test_invalid_command() {
        let qs = create_test_quote_server();
        let server = TcpServer::new("127.0.0.1:33334", qs.clone()).unwrap();

        thread::spawn(move || {
            server.start().unwrap();
        });

        thread::sleep(Duration::from_millis(100));

        let mut stream = TcpStream::connect("127.0.0.1:33334").unwrap();
        stream.write_all(b"BLAH\n").unwrap();

        let mut buf = [0u8; 128];
        let n = stream.read(&mut buf).unwrap();
        let s = String::from_utf8_lossy(&buf[..n]);

        assert_eq!(s.trim(), "ERR Invalid command");
    }

    #[test]
    fn test_stream_registers_client() {
        let qs = create_test_quote_server();
        let server = TcpServer::new("127.0.0.1:33335", qs.clone()).unwrap();

        thread::spawn(move || {
            server.start().unwrap();
        });

        thread::sleep(Duration::from_millis(100));

        let mut stream = TcpStream::connect("127.0.0.1:33335").unwrap();
        stream
            .write_all(b"STREAM 127.0.0.1:50000 AAPL MSFT\n")
            .unwrap();

        let mut buf = [0u8; 128];
        let n = stream.read(&mut buf).unwrap();
        let s = String::from_utf8_lossy(&buf[..n]);

        assert_eq!(s.trim(), "STREAM OK");
    }

    #[test]
    fn test_stop_removes_client() {
        let qs = create_test_quote_server();
        let server = TcpServer::new("127.0.0.1:33336", qs.clone()).unwrap();

        thread::spawn(move || {
            server.start().unwrap();
        });

        thread::sleep(Duration::from_millis(100));

        let mut stream = TcpStream::connect("127.0.0.1:33336").unwrap();

        stream.write_all(b"STREAM 127.0.0.1:50000 AAPL\n").unwrap();

        let mut buf = [0u8; 128];
        let _ = stream.read(&mut buf).unwrap();

        stream.write_all(b"STOP\n").unwrap();
        let n = stream.read(&mut buf).unwrap();

        let s = String::from_utf8_lossy(&buf[..n]);
        assert_eq!(s.trim(), "STOP OK");
    }
}
