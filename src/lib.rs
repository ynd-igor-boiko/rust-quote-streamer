//! # Stock Quote Streaming Server
//!
//! This crate implements a high-performance stock quote streaming server
//! using a **TCP control channel** and **UDP data streaming**.
//! It supports dynamic subscriptions, periodic quote updates, keep-alive monitoring,
//! and graceful shutdown.
//!
//! ## Features
//!
//! - Load tickers from a configuration file.
//! - Run a background quote generator thread.
//! - Stream quote updates to multiple clients over UDP.
//! - Manage client subscriptions via TCP commands.
//! - Automatic client cleanup based on keep-alive timeouts.
//! - Thread-safe architecture using `Arc`, `Mutex`, and atomics.
//! - Graceful shutdown support.
//!
//! ## Architecture Overview
//!
//! The crate is organized into several modules:
//!
//! - [`client`](crate::client) — Represents a subscribed client.
//! - [`stock_quote`](crate::stock_quote) — Data model for an individual stock quote.
//! - [`quote_generator`](crate::quote_generator) — Periodically generates random price updates.
//! - [`quote_server`](crate::quote_server) — Holds quotes, clients, and runs update/notify loops.
//! - [`tcp_server`](crate::tcp_server) — Implements the TCP protocol for subscription management.
//! - [`defs`](crate::defs) — Shared constants and timing parameters.
//! - [`errors`](crate::errors) — Error types used across modules.
//!
//! ## TCP Control Protocol
//!
//! The server accepts simple text-based commands over TCP:
//!
//! - `PING`
//!   Server responds with `PONG`.
//!
//! - `STREAM host:port TICKER...`
//!   Registers a client and begins streaming updates for the specified tickers
//!   to the provided UDP address.
//!
//! - `STOP`
//!   Unregisters the client and stops streaming updates.
//!
//! - Invalid commands result in:
//!   `ERR Invalid command`
//!
//! Each command must end with a newline (`\n`).
//!
//! ## Quote Update Loop
//!
//! The [`quote_server`](crate::quote_server) runs an internal loop that:
//!
//! 1. Updates all stock quotes using the quote generator.
//! 2. Notifies all subscribed clients using their callback functions.
//! 3. Sends updates to clients via UDP sockets.
//!
//! ## Keep-Alive Monitoring
//!
//! The TCP connection handler expects periodic `PING` messages from clients.
//! If no message is received within `CLIENT_KEEP_ALIVE_SEC`, the server:
//!
//! - removes the client subscription;
//! - closes the TCP connection gracefully.
//!
//! ## Example: Running the Server
//!
//! ```no_run
//! use std::sync::Arc;
//! use stock_server::quote_server::QuoteServer;
//! use stock_server::tcp_server::TcpServer;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Load tickers and initialize QuoteServer
//!     let qs = Arc::new(QuoteServer::from_config("tickers.txt")?);
//!
//!     // Start the background quote generator thread
//!     qs.start_background()?;
//!
//!     // Start the TCP server that manages client subscriptions
//!     let tcp = TcpServer::new("127.0.0.1:3333", qs.clone())?;
//!     tcp.start()?;
//!     Ok(())
//! }
//! ```

#![warn(missing_docs)]
#![deny(unreachable_pub)]

pub mod client;
pub mod defs;
pub mod errors;
pub mod quote_generator;
pub mod quote_server;
pub mod stock_quote;
pub mod tcp_server;
