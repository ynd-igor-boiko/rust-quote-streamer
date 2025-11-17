//! # Quote Client
//!
//! TCP client for connecting to a stock quote streaming server.
//! Supports interactive CLI commands, UDP streaming, and keep-alive monitoring.
//!
//! ## Features
//!
//! - Connects to TCP server and subscribes to stock tickers.
//! - Receives streaming updates via UDP.
//! - Automatic reconnection on TCP failure.
//! - Keep-alive PING/PONG mechanism to maintain connection.
//! - Interactive CLI with commands: `STREAM`, `STOP`, `EXIT`.
//! - Logging via `log` and configurable log level.

use quote_server::errors::CliError;

use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpStream, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use structopt::StructOpt;

/// Command-line options for the client
#[derive(Debug, StructOpt)]
#[structopt(name = "quote_client", about = "TCP client for streaming stock quotes")]
struct Opt {
    /// TCP server address, e.g., 127.0.0.1:33333
    #[structopt(short, long, default_value = "127.0.0.1:33333")]
    server_addr: String,

    /// UDP port to receive quote streams
    #[structopt(short, long, default_value = "50000")]
    udp_port: u16,

    /// Keep-alive interval in seconds
    #[structopt(short, long, default_value = "2")]
    keep_alive_sec: u64,

    /// Log level: error, warn, info, debug, trace
    #[structopt(short, long, default_value = "info")]
    log_level: String,
}

/// Initialize the logger with a given log level
fn init_logger(level: &str) -> Result<(), io::Error> {
    let mut builder = env_logger::Builder::new();

    let log_level = match level.to_lowercase().as_str() {
        "error" => log::LevelFilter::Error,
        "warn" => log::LevelFilter::Warn,
        "info" => log::LevelFilter::Info,
        "debug" => log::LevelFilter::Debug,
        "trace" => log::LevelFilter::Trace,
        _ => log::LevelFilter::Info,
    };

    builder.filter_level(log_level);
    builder.format_timestamp_micros();
    builder.format_module_path(false);
    builder.format_target(false);
    builder.init();

    Ok(())
}

/// Connects to the TCP quote server
fn connect(addr: &str) -> io::Result<(TcpStream, BufReader<TcpStream>)> {
    log::info!("Connecting to TCP server at {}", addr);
    let stream = TcpStream::connect(addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let reader = BufReader::new(stream.try_clone()?);
    log::info!("Connected to TCP server at {}", addr);
    Ok((stream, reader))
}

/// Reconnect to the server if connection is lost
fn reconnect(addr: &str) -> (TcpStream, BufReader<TcpStream>) {
    log::warn!("Attempting to reconnect to server at {}", addr);
    loop {
        match connect(addr) {
            Ok(pair) => {
                log::info!("Reconnected successfully to {}", addr);
                return pair;
            }
            Err(e) => {
                log::error!("Reconnect failed: {}. Retrying in 2s...", e);
                thread::sleep(Duration::from_secs(2));
            }
        }
    }
}

/// Send a command to the TCP server and return a single-line response
fn send_command(
    stream: &mut TcpStream,
    reader: &mut BufReader<TcpStream>,
    command: &str,
) -> io::Result<String> {
    log::debug!("Sending command to server: '{}'", command);
    stream.write_all(command.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    let mut buf = String::new();
    let n = reader.read_line(&mut buf)?;
    if n == 0 {
        log::error!("Server closed connection during command '{}'", command);
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "Server closed connection",
        ));
    }
    log::debug!("Received response from server: '{}'", buf.trim());
    Ok(buf)
}

/// Send a PING command and expect PONG reply
fn send_ping(stream: &mut TcpStream, reader: &mut BufReader<TcpStream>) -> io::Result<()> {
    log::trace!("Sending PING to server");
    stream.write_all(b"PING\n")?;
    stream.flush()?;

    let mut buf = String::new();
    let n = reader.read_line(&mut buf)?;
    if n == 0 {
        log::error!("Server closed connection during PING");
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "Server closed connection",
        ));
    }
    if buf.trim() != "PONG" {
        log::warn!("Expected PONG, got: '{}'", buf.trim());
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Expected PONG, got: {}", buf),
        ));
    }
    log::trace!("Received PONG from server");
    Ok(())
}

/// TCP quote client main loop
fn main() -> Result<(), CliError> {
    let opt = Opt::from_args();

    // Initialize logger
    init_logger(&opt.log_level).map_err(|e| CliError::GeneralError(e.to_string()))?;

    log::info!("Starting Quote Client");
    log::debug!("Command line options: {:?}", opt);

    let (stream, reader) =
        connect(&opt.server_addr).map_err(|e| CliError::GeneralError(e.to_string()))?;
    let stream = Arc::new(Mutex::new(stream));
    let reader = Arc::new(Mutex::new(reader));

    // Spawn keep-alive thread
    {
        let stream_clone = Arc::clone(&stream);
        let reader_clone = Arc::clone(&reader);
        let server_addr = opt.server_addr.clone();
        let keep_alive = Duration::from_secs(opt.keep_alive_sec);

        log::info!(
            "Starting keep-alive thread with interval: {}s",
            opt.keep_alive_sec
        );

        thread::spawn(move || {
            loop {
                thread::sleep(keep_alive);
                let mut s = match stream_clone.lock() {
                    Ok(guard) => guard,
                    Err(e) => {
                        log::error!(
                            "Failed to lock stream mutex: {}, exiting keep-alive thread",
                            e
                        );
                        break;
                    }
                };

                let mut r = match reader_clone.lock() {
                    Ok(guard) => guard,
                    Err(e) => {
                        log::error!(
                            "Failed to lock reader mutex: {}, exiting keep-alive thread",
                            e
                        );
                        break;
                    }
                };

                if let Err(e) = send_ping(&mut *s, &mut *r) {
                    log::warn!("Keep-alive failed: {}. Reconnecting...", e);
                    let (new_s, new_r) = reconnect(&server_addr);
                    *s = new_s;
                    *r = new_r;
                }
            }
        });
    }

    // Create UDP socket for receiving streamed quotes
    let udp_socket = UdpSocket::bind(("0.0.0.0", opt.udp_port))
        .map_err(|e| CliError::GeneralError(e.to_string()))?;
    udp_socket
        .set_read_timeout(Some(Duration::from_millis(500)))
        .map_err(|e| CliError::GeneralError(e.to_string()))?;
    log::info!("UDP socket bound to port {}", opt.udp_port);

    // Spawn thread to handle incoming UDP messages
    {
        let udp_clone = udp_socket
            .try_clone()
            .map_err(|e| CliError::GeneralError(e.to_string()))?;
        log::info!("Starting UDP listener thread");

        thread::spawn(move || {
            log::debug!("UDP listener thread started");
            let mut message_count = 0;
            loop {
                let mut buf = [0u8; 1024];
                if let Ok((n, src)) = udp_clone.recv_from(&mut buf) {
                    let msg = String::from_utf8_lossy(&buf[..n]);
                    message_count += 1;
                    log::debug!(
                        "Received UDP message #{} from {}: {}",
                        message_count,
                        src,
                        msg.trim()
                    );
                    println!("[{}] {}", src, msg.trim());
                }
            }
        });
    }

    // Interactive CLI loop
    let stdin = io::stdin();
    log::info!("Entering interactive command loop");

    loop {
        print!("quote-client> ");
        io::stdout()
            .flush()
            .map_err(|e| CliError::GeneralError(e.to_string()))?;

        let mut input = String::new();
        stdin
            .read_line(&mut input)
            .map_err(|e| CliError::GeneralError(e.to_string()))?;
        let command = input.trim();

        if command.is_empty() {
            continue;
        }
        if command.eq_ignore_ascii_case("EXIT") {
            log::info!("Received EXIT command, shutting down client");
            println!("Exiting client.");
            break;
        }

        let mut s = stream
            .lock()
            .map_err(|e| CliError::GeneralError(e.to_string()))?;
        let mut r = reader
            .lock()
            .map_err(|e| CliError::GeneralError(e.to_string()))?;

        match send_command(&mut *s, &mut *r, command) {
            Ok(resp) => {
                log::debug!("Command '{}' executed successfully", command);
                print!("{}", resp);
            }
            Err(e) => {
                log::error!("Command '{}' failed: {}. Reconnecting...", command, e);
                let (new_s, new_r) = reconnect(&opt.server_addr);
                *s = new_s;
                *r = new_r;
            }
        }
    }

    log::info!("Quote Client shutdown complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::UdpSocket;

    #[test]
    fn test_send_command_invalid_server() {
        let result = connect("127.0.0.1:65000"); // assuming nothing is listening
        assert!(result.is_err());
    }

    #[test]
    fn test_udp_bind() {
        let udp = UdpSocket::bind("127.0.0.1:0");
        assert!(udp.is_ok());
    }

    #[test]
    fn test_send_ping_no_server() {
        let mut stream = TcpStream::connect("127.0.0.1:65000");
        if let Ok(mut s) = stream {
            let reader = BufReader::new(s.try_clone().unwrap());
            let result = send_ping(&mut s, &mut reader);
            assert!(result.is_err());
        } else {
            assert!(true);
        }
    }
}
