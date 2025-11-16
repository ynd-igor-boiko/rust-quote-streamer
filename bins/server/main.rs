use quote_server::errors::QuoteServerError;
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
}

fn main() -> Result<(), QuoteServerError> {
    // Parse CLI arguments
    let opt = Opt::from_args();

    // Initialize QuoteServer from config file
    let quote_server = QuoteServer::from_config(&opt.config)?;
    let quote_server = Arc::new(quote_server);

    println!(
        "QuoteServer initialized. Starting TCP server on {}",
        opt.tcp_addr
    );

    // Start TCP server
    let tcp_server = TcpServer::new(&opt.tcp_addr, quote_server.clone())
        .map_err(|e| QuoteServerError::InitializationError(format!("{:?}", e)))?;

    println!("TCP server running. Waiting for client connections...");

    // Run server (blocking call)
    tcp_server
        .start()
        .map_err(|e| QuoteServerError::InitializationError(format!("{:?}", e)))?;

    Ok(())
}
