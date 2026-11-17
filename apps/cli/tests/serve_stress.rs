//! Load and fault tests of `opencircuit serve`: the server keeps answering
//! well-behaved clients while others pipeline, trickle, flood, send garbage
//! or come and go.

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{Shutdown, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const COMMAND: &str = "examples/direct-starter/command.ocircuit";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root exists")
}

/// A running server, killed when dropped.
struct Server {
    child: Child,
    address: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_opencircuit"))
            .args([
                "serve",
                COMMAND,
                "--listen",
                "127.0.0.1:0",
                "--duration-ms",
                "60000",
            ])
            .current_dir(root())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the server starts");
        let mut line = String::new();
        BufReader::new(child.stdout.take().expect("stdout"))
            .read_line(&mut line)
            .expect("the server announces its address");
        let address = line
            .trim()
            .strip_prefix("listening on ")
            .unwrap_or_else(|| panic!("unexpected first line `{line}`"))
            .to_owned();
        Self { child, address }
    }

    fn connect(&self) -> TcpStream {
        let stream = TcpStream::connect(&self.address).expect("connects");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("timeout");
        stream
    }

    /// The server process has not exited (crashed).
    fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

/// One Modbus TCP frame around `pdu`.
fn frame(transaction: u16, pdu: &[u8]) -> Vec<u8> {
    let mut frame = transaction.to_be_bytes().to_vec();
    frame.extend([0, 0]);
    frame.extend(u16::try_from(pdu.len() + 1).unwrap().to_be_bytes());
    frame.push(1);
    frame.extend(pdu);
    frame
}

/// Read one answer and return its transaction and PDU.
fn answer(stream: &mut TcpStream) -> (u16, Vec<u8>) {
    let mut header = [0_u8; 7];
    stream.read_exact(&mut header).expect("header");
    let length = usize::from(u16::from_be_bytes([header[4], header[5]])) - 1;
    let mut pdu = vec![0_u8; length];
    stream.read_exact(&mut pdu).expect("answer");
    (u16::from_be_bytes([header[0], header[1]]), pdu)
}

/// Read discrete input 0 (K1): a request a healthy server always answers.
const READ_K1: [u8; 5] = [2, 0, 0, 0, 1];

fn healthy(server: &Server) {
    let mut client = server.connect();
    client.write_all(&frame(77, &READ_K1)).expect("sent");
    let (transaction, pdu) = answer(&mut client);
    assert_eq!(transaction, 77);
    assert_eq!(&pdu[..2], &[2, 1]);
}

/// A well-behaved client is served within a second. Connections that were
/// closed while still waiting in the listen queue are accepted, and hold one
/// of the four slots, until the next turn of the loop reads their end; a
/// client that arrives right behind such a burst is refused once and gets
/// in when it tries again, as Modbus clients do.
fn healthy_after_retrying(server: &Server) {
    for _ in 0..50 {
        let mut client = server.connect();
        if client.write_all(&frame(78, &READ_K1)).is_ok() {
            let mut header = [0_u8; 7];
            if client.read_exact(&mut header).is_ok() {
                assert_eq!(&header[..2], &78_u16.to_be_bytes());
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("no client was served for a second");
}

#[test]
fn a_thousand_pipelined_requests_are_answered_in_order() {
    let server = Server::start();
    let mut client = server.connect();
    let burst: Vec<u8> = (0..1000_u16).flat_map(|t| frame(t, &READ_K1)).collect();

    client.write_all(&burst).expect("sent");

    for expected in 0..1000_u16 {
        let (transaction, pdu) = answer(&mut client);
        assert_eq!(transaction, expected);
        assert_eq!(&pdu[..2], &[2, 1]);
    }
}

#[test]
fn a_request_sent_one_byte_at_a_time_is_answered() {
    let server = Server::start();
    let mut client = server.connect();
    client.set_nodelay(true).expect("nodelay");

    for byte in frame(5, &READ_K1) {
        client.write_all(&[byte]).expect("sent");
        std::thread::sleep(Duration::from_millis(3));
    }

    let (transaction, pdu) = answer(&mut client);
    assert_eq!(transaction, 5);
    assert_eq!(&pdu[..2], &[2, 1]);
}

#[test]
fn garbage_from_one_client_does_not_disturb_the_others() {
    let mut server = Server::start();
    // A deterministic stream of bytes that is not Modbus.
    let mut state: u32 = 0x1234_5678;
    for _ in 0..20 {
        let mut noisy = server.connect();
        let garbage: Vec<u8> = (0..300)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state.to_le_bytes()[0]
            })
            .collect();
        // The server may close the connection part way; either is fine.
        let _ = noisy.write_all(&garbage);
        drop(noisy);
    }

    healthy_after_retrying(&server);
    assert!(server.is_running());
}

#[test]
fn a_client_that_floods_without_reading_does_not_stall_the_server() {
    let mut server = Server::start();
    let mut flooder = server.connect();
    flooder
        .set_write_timeout(Some(Duration::from_millis(500)))
        .expect("timeout");
    let burst: Vec<u8> = (0..2000_u16).flat_map(|t| frame(t, &READ_K1)).collect();

    // Send for a while without reading a single answer: the server's send
    // buffer to this client fills. Stop when the server pushes back or
    // drops the connection.
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(3) {
        if flooder.write_all(&burst).is_err() {
            break;
        }
    }

    let answered = Instant::now();
    healthy(&server);
    assert!(
        answered.elapsed() < Duration::from_secs(5),
        "a well-behaved client waited {:?}",
        answered.elapsed()
    );
    assert!(server.is_running());
}

#[test]
fn hundreds_of_short_connections_leave_the_server_serving() {
    let mut server = Server::start();

    for round in 0..300_u16 {
        let mut client = server.connect();
        if round % 3 == 0 {
            client.write_all(&frame(round, &READ_K1)).expect("sent");
            let (transaction, _) = answer(&mut client);
            assert_eq!(transaction, round);
        }
        // Half-closed, fully closed and dropped connections in turn.
        if round % 2 == 0 {
            let _ = client.shutdown(Shutdown::Write);
        }
    }

    healthy_after_retrying(&server);
    assert!(server.is_running());
}

#[test]
fn an_oversized_length_field_closes_only_that_connection() {
    let mut server = Server::start();
    let mut liar = server.connect();
    // A header announcing 65,535 bytes: far beyond the Modbus maximum.
    liar.write_all(&[0, 1, 0, 0, 0xFF, 0xFF, 1, 2])
        .expect("sent");
    let mut byte = [0_u8; 1];

    let closed = match liar.read(&mut byte) {
        Ok(read) => read == 0,
        Err(error) => !matches!(
            error.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        ),
    };

    assert!(closed);
    healthy(&server);
    assert!(server.is_running());
}
