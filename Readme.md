# Real-Time Stock Quote Server

A TCP/UDP-based real-time stock quote server and client implemented in Rust.  
The server periodically updates stock quotes and streams them to connected clients over UDP.  
Clients can connect via TCP to manage subscriptions and receive updates.

---

## Features

### Server
- Load stock ticker configuration from a file.
- Periodically updates quotes in the background.
- TCP interface for client commands:
  - `PING` → responds with `PONG`  
  - `STREAM host:port TICKER...` → registers a client to receive quotes via UDP  
  - `STOP` → removes a client subscription
- Sends stock updates to clients via UDP.
- Multi-threaded and thread-safe using `Arc` and `Mutex`.
- Logging support with configurable log levels.
- Graceful shutdown support.

### Client
- Connects to TCP server for command interface.
- Receives stock updates via UDP.
- Keep-alive support with `PING`/`PONG`.
- Automatic reconnect if TCP connection is lost.
- Interactive command-line interface.

---

## Requirements

- Rust >= 1.70
- Cargo (Rust package manager)

---

## Installation

Clone the repository:

```bash
git clone https://github.com/Odinsaw/rust-quote-streamer
cd rust-quote-streamer
```

Build the project:

```bash
cargo build --release
```

---

## Usage

### Quote Server

```bash
cargo run --bin quote_server -- --tcp-addr 127.0.0.1:33333 --config tickers.txt --log-level info
```

**Options:**
- `--tcp-addr`, `-t`: TCP server listen address (default: `127.0.0.1:33333`)  
- `--config`, `-c`: Path to configuration file with ticker symbols  
- `--log-level`, `-l`: Logging level (`error`, `warn`, `info`, `debug`, `trace`)  

**Example configuration file (`tickers.txt`):**
```
AAPL
MSFT
GOOG
TSLA
```

### Quote Client

```bash
cargo run --bin quote_client -- --server-addr 127.0.0.1:33333 --udp-port 50000 --keep-alive-sec 2 --log-level info
```

**Options:**
- `--server-addr`, `-s`: TCP server address (default: `127.0.0.1:33333`)  
- `--udp-port`, `-u`: UDP port to receive stock updates (default: `50000`)  
- `--keep-alive-sec`, `-k`: Interval for sending PING commands (default: `2` seconds)  
- `--log-level`, `-l`: Logging level (`error`, `warn`, `info`, `debug`, `trace`)  

**Interactive commands:**
- `PING` → test server connectivity  
- `STREAM host:port TICKER...` → subscribe to stock updates  
- `STOP` → unsubscribe  
- `EXIT` → exit client  

**Example client session:**
```
quote-client> STREAM 127.0.0.1:50000 AAPL MSFT
STREAM OK
quote-client> PING
PONG
quote-client> STOP
STOP OK
quote-client> EXIT
```

---

## Logging

Both server and client support configurable logging.  
You can set the log level to see debug or trace information:

```bash
--log-level debug
--log-level trace
```

Example server log output:
```
INFO  quote_server > Starting Quote Server
DEBUG quote_server > Loaded configuration: tickers.txt
INFO  quote_server > TCP server listening on 127.0.0.1:33333
INFO  quote_server > Client connected: 127.0.0.1:54790
```

---

## Architecture

- `QuoteServer`: Main server that holds stock quotes, a quote generator, and registered clients.
- `TcpServer`: TCP server handling client connections and commands.
- `quote_client`: TCP client that subscribes to stock quotes via UDP.
- Multi-threaded design with Arc and Mutex to safely share state across threads.
- Periodic tasks (quote updates, keep-alive) run in background threads.

---

## Testing

Run server and client tests:

```bash
cargo test
```

Tests include:
- TCP command handling (`PING`, `STREAM`, `STOP`)  
- UDP message reception  
- Keep-alive logic and reconnection  
