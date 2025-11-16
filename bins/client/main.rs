use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
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
}

/// Connects to the TCP quote server
fn connect(addr: &str) -> io::Result<(TcpStream, BufReader<TcpStream>)> {
    let stream = TcpStream::connect(addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let reader = BufReader::new(stream.try_clone()?);
    println!("Connected to TCP server at {}", addr);
    Ok((stream, reader))
}

/// Reconnect to the server if connection is lost
fn reconnect(addr: &str) -> (TcpStream, BufReader<TcpStream>) {
    loop {
        match connect(addr) {
            Ok(pair) => return pair,
            Err(e) => {
                eprintln!("Reconnect failed: {}. Retrying in 2s...", e);
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
    stream.write_all(command.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    let mut buf = String::new();
    let n = reader.read_line(&mut buf)?;
    if n == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "Server closed connection",
        ));
    }
    Ok(buf)
}

/// Send a PING command and expect PONG reply
fn send_ping(stream: &mut TcpStream, reader: &mut BufReader<TcpStream>) -> io::Result<()> {
    stream.write_all(b"PING\n")?;
    stream.flush()?;

    let mut buf = String::new();
    let n = reader.read_line(&mut buf)?;
    if n == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "Server closed connection",
        ));
    }
    if buf.trim() != "PONG" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Expected PONG, got: {}", buf),
        ));
    }
    Ok(())
}

/// TCP quote client main loop
fn main() -> io::Result<()> {
    let opt = Opt::from_args();

    let (stream, reader) = connect(&opt.server_addr)?;
    let stream = Arc::new(Mutex::new(stream));
    let reader = Arc::new(Mutex::new(reader));

    // Spawn keep-alive thread
    {
        let stream_clone = Arc::clone(&stream);
        let reader_clone = Arc::clone(&reader);
        let server_addr = opt.server_addr.clone();
        let keep_alive = Duration::from_secs(opt.keep_alive_sec);

        thread::spawn(move || {
            loop {
                thread::sleep(keep_alive);
                let mut s = stream_clone.lock().unwrap();
                let mut r = reader_clone.lock().unwrap();

                if let Err(e) = send_ping(&mut *s, &mut *r) {
                    eprintln!("Keep-alive failed: {}. Reconnecting...", e);
                    let (new_s, new_r) = reconnect(&server_addr);
                    *s = new_s;
                    *r = new_r;
                }
            }
        });
    }

    // Create UDP socket for receiving streamed quotes
    let udp_socket = UdpSocket::bind(("0.0.0.0", opt.udp_port))?;
    udp_socket.set_read_timeout(Some(Duration::from_millis(500)))?;
    println!("UDP listening on port {}", opt.udp_port);

    // Spawn thread to handle incoming UDP messages
    {
        let udp_clone = udp_socket.try_clone()?;
        thread::spawn(move || {
            loop {
                let mut buf = [0u8; 1024];
                if let Ok((n, src)) = udp_clone.recv_from(&mut buf) {
                    let msg = String::from_utf8_lossy(&buf[..n]);
                    println!("[{}] {}", src, msg);
                }
            }
        });
    }

    // Interactive CLI loop
    let stdin = io::stdin();
    loop {
        print!("quote-client> ");
        io::stdout().flush()?;

        let mut input = String::new();
        stdin.read_line(&mut input)?;
        let command = input.trim();

        if command.is_empty() {
            continue;
        }
        if command.eq_ignore_ascii_case("EXIT") {
            println!("Exiting client.");
            break;
        }

        let mut s = stream.lock().unwrap();
        let mut r = reader.lock().unwrap();

        match send_command(&mut *s, &mut *r, command) {
            Ok(resp) => print!("{}", resp),
            Err(e) => {
                eprintln!("Command failed: {}. Reconnecting...", e);
                let (new_s, new_r) = reconnect(&opt.server_addr);
                *s = new_s;
                *r = new_r;
            }
        }
    }

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
