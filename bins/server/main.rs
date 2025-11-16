//! # Quote Server
//!
//! This is a TCP-based real-time stock quote server.
//! It periodically updates stock quotes and streams them to connected clients over UDP.
//!
//! ## Features
//! - Loads stock ticker configuration from a file.
//! - Periodically updates quotes in the background.
//! - TCP interface for client commands: `PING`, `STREAM`, `STOP`.
//! - Sends stock updates to clients via UDP.
//! - Multi-threaded and safe with `Arc` and `Mutex` where necessary.
//! - Logging support with configurable log levels.
//!
//! ## Command-line Options
//! - `--tcp-addr` / `-t`: TCP listen address (default `127.0.0.1:33333`).
//! - `--config` / `-c`: Path to the configuration file with ticker symbols.
//! - `--log-level` / `-l`: Log level (`error`, `warn`, `info`, `debug`, `trace`).

use quote_server::errors::{CliError, QuoteServerError};
use quote_server::quote_server::QuoteServer;
use quote_server::tcp_server::TcpServer;
use std::sync::Arc;
use structopt::StructOpt;

/// Command-line options for the Quote Server
#[derive(Debug, StructOpt)]
#[structopt(
    name = "quote_server",
    about = "TCP Quote Server for real-time stock quotes"
)]
struct Opt {
    /// TCP listen address, e.g., 127.0.0.1:33333
    #[structopt(short, long, default_value = "127.0.0.1:33333")]
    tcp_addr: String,

    /// Path to the configuration file with ticker symbols
    #[structopt(short, long)]
    config: String,

    /// Log level: error, warn, info, debug, trace
    #[structopt(short, long, default_value = "info")]
    log_level: String,
}

/// Initializes the logger using env_logger with the given level
fn init_logger(level: &str) -> Result<(), CliError> {
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

fn main() -> Result<(), CliError> {
    // Parse CLI arguments
    let opt = Opt::from_args();

    // Initialize logger
    init_logger(&opt.log_level)?;

    log::info!("Starting Quote Server");
    log::debug!("Command line options: {:?}", opt);

    // Initialize QuoteServer from config file
    log::info!("Loading configuration from: {}", opt.config);
    let quote_server = QuoteServer::from_config(&opt.config)?;
    let quote_server = Arc::new(quote_server);

    log::info!("Starting QuoteServer background processing");
    quote_server.start()?; // starts periodic quote updates

    log::info!(
        "QuoteServer initialized successfully. Starting TCP server on {}",
        opt.tcp_addr
    );

    // Start TCP server
    let tcp_server = TcpServer::new(&opt.tcp_addr, quote_server.clone())
        .map_err(|e| CliError::GeneralError(format!("{:?}", e)))?;

    log::info!("TCP server initialized. Waiting for client connections...");

    // Run server (blocking call)
    log::info!("Entering main server loop");
    tcp_server
        .start()
        .map_err(|e| CliError::GeneralError(format!("{:?}", e)))?;

    log::info!("Server shutdown complete");
    Ok(())
}
