//! `opencircuit serve`: the Modbus TCP server of ADR-0009.
//!
//! The codec and the I/O map are `circuit-io`; this module owns what the
//! core may not have: the socket, the real-time clock and the loop. The
//! simulation advances with real time at its tick; writes received between
//! two ticks are applied as input actions at the next tick boundary (the
//! last write to a device wins); reads answer from the last solved tick.
//!
//! Safety limits: loopback unless `--listen` says otherwise (with a
//! warning), at most [`MAX_CONNECTIONS`] clients, frames up to the Modbus
//! maximum, idle connections closed after [`IDLE_TIMEOUT`]. OpenCircuit is
//! not a controller; it must not be connected to a real plant.

use std::collections::BTreeMap;
use std::io::{self, Read as _, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use circuit_io::IoMap;
use circuit_io::modbus::{self, MAX_FRAME};
use circuit_model::{ComponentCatalog, DeviceId, InputAction, SimSnapshot};
use circuit_sim::{SimConfig, SimulationError, Simulator, compile};

use crate::{CliError, EXIT_FAILED, EXIT_OK};

/// Most clients served at once.
pub(crate) const MAX_CONNECTIONS: usize = 4;
/// A client that sends nothing for this long is disconnected.
pub(crate) const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
/// Pause between two turns of the loop.
const TURN: Duration = Duration::from_millis(1);
/// Most ticks run in one turn, so a stalled process never runs an unbounded
/// burst when it wakes up.
const MAX_TICKS_PER_TURN: u64 = 1_000;

/// What `serve` was asked to do.
pub(crate) struct ServeRequest<'a> {
    pub(crate) doc: &'a circuit_model::Schematic,
    pub(crate) catalog: &'a ComponentCatalog,
    pub(crate) listen: SocketAddr,
    /// Stop after this long; `None` serves until the process is stopped.
    pub(crate) duration: Option<Duration>,
}

struct Client {
    stream: TcpStream,
    buffer: Vec<u8>,
    last_seen: Instant,
}

/// Serve until the duration runs out. Returns [`EXIT_FAILED`] when the
/// project does not compile.
pub(crate) fn serve(
    request: &ServeRequest<'_>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<u8, CliError> {
    let output = compile(request.doc, request.catalog);
    let Some(circuit) = output.circuit else {
        for diagnostic in &output.diagnostics {
            writeln!(err, "{diagnostic}")?;
        }
        return Ok(EXIT_FAILED);
    };
    let map = IoMap::new(request.doc, request.catalog);
    let config = SimConfig {
        record_trace: false,
        ..SimConfig::default()
    };
    let tick_us = u64::from(config.tick_us);
    let mut simulator = Simulator::new(circuit, config)
        .map_err(|error| CliError::Usage(format!("cannot start the simulator: {error}")))?;

    let listener =
        TcpListener::bind(request.listen).map_err(|source| listen_error(request.listen, source))?;
    listener
        .set_nonblocking(true)
        .map_err(|source| listen_error(request.listen, source))?;
    let bound = listener
        .local_addr()
        .map_err(|source| listen_error(request.listen, source))?;
    if !bound.ip().is_loopback() {
        writeln!(
            err,
            "warning: listening on {bound}, which other machines can reach; Modbus has no authentication"
        )?;
    }
    writeln!(out, "listening on {bound}")?;
    out.flush()?;

    let start = Instant::now();
    let mut ticked_us: u64 = 0;
    let mut pending: BTreeMap<DeviceId, InputAction> = BTreeMap::new();
    let mut snapshot: Option<SimSnapshot> = None;
    let mut clients: Vec<Client> = Vec::new();
    loop {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(start);
        if request.duration.is_some_and(|limit| elapsed >= limit) {
            return Ok(EXIT_OK);
        }

        accept(&listener, &mut clients, now, |clients| {
            clients.retain_mut(|client| {
                serve_client(client, &map, snapshot.as_ref(), &mut pending, now)
            });
        });

        // Advance the simulation with real time.
        let real_us = u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX);
        let due = real_us
            .saturating_sub(ticked_us)
            .checked_div(tick_us)
            .unwrap_or(0)
            .min(MAX_TICKS_PER_TURN);
        if due > 0 {
            let actions: Vec<InputAction> = std::mem::take(&mut pending).into_values().collect();
            let ticks = u32::try_from(due).unwrap_or(u32::MAX);
            match simulator.step_n(ticks, &actions) {
                Ok(_) | Err(SimulationError::AlreadyFaulted) => {}
                Err(error) => writeln!(err, "warning: inputs refused: {error}")?,
            }
            ticked_us = ticked_us.saturating_add(due.saturating_mul(tick_us));
            snapshot = simulator.snapshot();
        }

        // Answer every complete frame of every client.
        clients
            .retain_mut(|client| serve_client(client, &map, snapshot.as_ref(), &mut pending, now));

        thread::sleep(TURN);
    }
}

fn listen_error(address: SocketAddr, source: io::Error) -> CliError {
    CliError::Usage(format!("cannot listen on {address}: {source}"))
}

/// Accept waiting clients up to the limit; refuse the others by closing
/// their connection at once. When every slot is taken, `reap` first reads
/// the clients there are: one that has closed (it may have waited in the
/// listen queue with the new one) frees its slot instead of getting the new
/// client refused.
fn accept(
    listener: &TcpListener,
    clients: &mut Vec<Client>,
    now: Instant,
    mut reap: impl FnMut(&mut Vec<Client>),
) {
    while let Ok((stream, _)) = listener.accept() {
        if clients.len() >= MAX_CONNECTIONS {
            reap(clients);
        }
        if clients.len() >= MAX_CONNECTIONS || stream.set_nonblocking(true).is_err() {
            drop(stream);
            continue;
        }
        let _ = stream.set_nodelay(true);
        clients.push(Client {
            stream,
            buffer: Vec::with_capacity(MAX_FRAME),
            last_seen: now,
        });
    }
}

/// Read what a client sent and answer its complete frames. Returns `false`
/// when the connection must be closed: closed by the client, idle too long,
/// a frame that is not Modbus TCP, or an I/O error.
fn serve_client(
    client: &mut Client,
    map: &IoMap,
    snapshot: Option<&SimSnapshot>,
    pending: &mut BTreeMap<DeviceId, InputAction>,
    now: Instant,
) -> bool {
    let mut chunk = [0_u8; MAX_FRAME];
    match client.stream.read(&mut chunk) {
        Ok(0) => return false,
        Ok(read) => {
            client.last_seen = now;
            client.buffer.extend(chunk.get(..read).unwrap_or_default());
        }
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
        Err(_) => return false,
    }
    if now.saturating_duration_since(client.last_seen) > IDLE_TIMEOUT {
        return false;
    }
    loop {
        let length = match modbus::frame_len(&client.buffer) {
            None => return client.buffer.len() <= MAX_FRAME,
            Some(Err(_)) => return false,
            Some(Ok(length)) => length,
        };
        let Some(frame) = client.buffer.get(..length) else {
            // The rest of the frame has not arrived yet.
            return true;
        };
        let Ok((header, request)) = modbus::decode(frame) else {
            return false;
        };
        let (response, actions) = map.handle(&request, snapshot);
        for action in actions {
            pending.insert(action.device(), action);
        }
        if client
            .stream
            .write_all(&modbus::encode(header, &response))
            .is_err()
        {
            return false;
        }
        client.buffer.drain(..length);
    }
}
