//! Engine-free networking core.
//!
//! The scripting-engine-independent half of `flux:net`: unprivileged TCP/UDP
//! sockets and local-interface enumeration, built on tokio (plus `socket2` for
//! the bind-time socket options tokio doesn't expose, and `netdev` for interface
//! listing). It names no scripting-engine types; the marshalling layer
//! (`flux/src/plugins/modules/net.rs`) decodes JS args into these calls, wraps
//! `Conn` / `Listener` / `Udp` in the host's stream / async-iterable types, and
//! encodes results back to JS.
//!
//! Why this exists: it gives the app a cross-platform LAN-discovery floor with no
//! external binary (`nmap` / `ip`) — a TCP connect-scan for host/port liveness, a
//! UDP multicast socket for peer beacons, and interface enumeration for subnet
//! detection. The raw-socket ceiling (no unprivileged ICMP / ARP) is the same as
//! Node and Bun; this is ordinary userland sockets.
//!
//! Reads are pull-based (`Conn::read_chunk` -> `Ok(None)` at EOF) so the caller
//! drives the transport, the same shape as the p2p `Stream`. No background task
//! is needed (writes go straight out under a lock), so unlike subprocess / p2p
//! the caller spawns nothing.

use std::cell::RefCell;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::Mutex;

/// Read granularity: each `read_chunk` pulls at most this many bytes.
const READ_CHUNK: usize = 64 * 1024;

/// Largest UDP datagram a single `recv` will return. A beacon is far smaller;
/// this is only the ceiling so a stray jumbo datagram isn't silently truncated.
const UDP_RECV_MAX: usize = 64 * 1024;

// ---- TCP connect-scan -------------------------------------------------------

/// The result of a single TCP `probe` — the host-liveness primitive a connect
/// scan is built from.
///
/// `Closed` is the subtle one: a refused connection (RST) still proves the host
/// is **up**, because something answered. Only `Filtered` (a timeout, or an
/// unreachable / other error) is no evidence of life. So a sweep counts both
/// `Open` and `Closed` as a live host, and only `Filtered` as absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Liveness {
  Open,
  Closed,
  Filtered,
}

impl Liveness {
  pub fn as_str(self) -> &'static str {
    match self {
      Liveness::Open => "open",
      Liveness::Closed => "closed",
      Liveness::Filtered => "filtered",
    }
  }
}

/// Attempt a TCP connect to `host:port` and report what the outcome says about
/// the host. Infallible by design — every failure mode maps to a `Liveness`, so
/// a sweep never has to special-case errors. `timeout_ms` bounds the wait; a host
/// that neither accepts nor refuses within it is `Filtered`.
pub async fn probe(host: &str, port: u16, timeout_ms: u64) -> Liveness {
  match tokio::time::timeout(Duration::from_millis(timeout_ms), TcpStream::connect((host, port))).await {
    Ok(Ok(_stream)) => Liveness::Open, // connected; the stream is dropped at once
    Ok(Err(e)) if e.kind() == io::ErrorKind::ConnectionRefused => Liveness::Closed,
    Ok(Err(_)) => Liveness::Filtered, // unreachable / reset / resolve failure: no liveness signal
    Err(_) => Liveness::Filtered,     // timed out
  }
}

// ---- TCP streams ------------------------------------------------------------

/// A connected TCP stream: a byte duplex. Reads are pull-based; writes go straight
/// out under a lock so concurrent writes serialize instead of racing. The split
/// halves are held in `Option`s and taken across awaits (no borrow is held across
/// `.await`), mirroring the p2p `Stream`. Dropping the `Conn` closes the socket.
pub struct Conn {
  read: RefCell<Option<OwnedReadHalf>>,
  write: Mutex<Option<OwnedWriteHalf>>,
  peer: SocketAddr,
}

impl Conn {
  fn from_stream(stream: TcpStream) -> Result<Conn, String> {
    let peer = stream.peer_addr().map_err(|e| e.to_string())?;
    let _ = stream.set_nodelay(true);
    let (read, write) = stream.into_split();
    Ok(Conn { read: RefCell::new(Some(read)), write: Mutex::new(Some(write)), peer })
  }

  /// Pull the next chunk (at most `READ_CHUNK` bytes). `Ok(None)` at end-of-stream
  /// or once the read half is gone (closed). The read half is taken out before the
  /// await and put back after, so no borrow is held across it.
  pub async fn read_chunk(&self) -> Result<Option<Vec<u8>>, String> {
    let Some(mut read) = self.read.borrow_mut().take() else {
      return Ok(None);
    };
    let mut buf = vec![0u8; READ_CHUNK];
    let n = read.read(&mut buf).await.map_err(|e| e.to_string())?;
    if n == 0 {
      return Ok(None); // EOF: leave the read half taken, so further reads stay Ok(None)
    }
    buf.truncate(n);
    *self.read.borrow_mut() = Some(read);
    Ok(Some(buf))
  }

  /// Write all of `bytes`, serialized behind the write lock. Errors if the
  /// connection has been closed or the write fails.
  pub async fn write(&self, bytes: Vec<u8>) -> Result<(), String> {
    let mut guard = self.write.lock().await;
    match guard.as_mut() {
      Some(w) => w.write_all(&bytes).await.map_err(|e| e.to_string()),
      None => Err("connection is closed".to_string()),
    }
  }

  /// Stop reading now (drops the read half). The write half closes when the
  /// `Conn` is dropped; bytes already handed to the OS still flush.
  pub fn close(&self) {
    self.read.borrow_mut().take();
  }

  /// The remote peer's address, e.g. `192.168.2.37:445`.
  pub fn peer(&self) -> String {
    self.peer.to_string()
  }
}

/// Open a TCP connection to `host:port`, bounded by `timeout_ms`. Unlike `probe`
/// this hands back a live `Conn` for app protocols / banner grabs; a refused or
/// timed-out connect comes back as a plain message string.
pub async fn connect(host: &str, port: u16, timeout_ms: u64) -> Result<Conn, String> {
  let stream = match tokio::time::timeout(Duration::from_millis(timeout_ms), TcpStream::connect((host, port))).await {
    Ok(Ok(s)) => s,
    Ok(Err(e)) => return Err(format!("connect {host}:{port}: {e}")),
    Err(_) => return Err(format!("connect {host}:{port} timed out after {timeout_ms}ms")),
  };
  Conn::from_stream(stream)
}

/// A bound TCP listener. `accept` yields one `Conn` per incoming connection; the
/// marshalling layer turns repeated calls into an async-iterable. Dropping the
/// listener stops accepting.
pub struct Listener {
  inner: TcpListener,
}

/// Bind a TCP listener to `host:port` (port `0` = OS-assigned).
pub async fn listen(host: &str, port: u16) -> Result<Listener, String> {
  let inner = TcpListener::bind((host, port)).await.map_err(|e| format!("listen {host}:{port}: {e}"))?;
  Ok(Listener { inner })
}

impl Listener {
  /// Accept the next incoming connection.
  pub async fn accept(&self) -> Result<Conn, String> {
    let (stream, _peer) = self.inner.accept().await.map_err(|e| e.to_string())?;
    Conn::from_stream(stream)
  }

  /// The bound local address (with the OS-assigned port when `0` was requested).
  pub fn local_addr(&self) -> String {
    self.inner.local_addr().map(|a| a.to_string()).unwrap_or_default()
  }
}

// ---- UDP / multicast --------------------------------------------------------

/// A bound UDP socket with the multicast / broadcast knobs the peer beacon needs.
/// Bound through `socket2` so `SO_REUSEADDR` / `SO_REUSEPORT` can be set before
/// bind (tokio's `UdpSocket::bind` can't), letting several listeners share the
/// beacon port and a restart re-bind immediately.
pub struct Udp {
  inner: UdpSocket,
}

/// Bind a UDP socket to `0.0.0.0:port` (port `0` = OS-assigned). `reuse` sets
/// `SO_REUSEADDR` (and `SO_REUSEPORT` on Unix) so multiple sockets can bind the
/// same multicast port.
pub async fn udp_bind(port: u16, reuse: bool) -> Result<Udp, String> {
  let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).map_err(|e| e.to_string())?;
  if reuse {
    socket.set_reuse_address(true).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    socket.set_reuse_port(true).map_err(|e| e.to_string())?;
  }
  let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
  socket.bind(&SockAddr::from(addr)).map_err(|e| format!("udp bind :{port}: {e}"))?;
  socket.set_nonblocking(true).map_err(|e| e.to_string())?; // tokio adopts only nonblocking sockets
  let std_socket: std::net::UdpSocket = socket.into();
  let inner = UdpSocket::from_std(std_socket).map_err(|e| e.to_string())?;
  Ok(Udp { inner })
}

impl Udp {
  /// Send a datagram to `host:port` — a multicast group, a broadcast address, or a
  /// unicast peer. Returns the number of bytes sent.
  pub async fn send(&self, data: &[u8], host: &str, port: u16) -> Result<usize, String> {
    self.inner.send_to(data, (host, port)).await.map_err(|e| e.to_string())
  }

  /// Receive one datagram, returning its bytes and the sender's `ip` and `port`.
  pub async fn recv(&self) -> Result<(Vec<u8>, String, u16), String> {
    let mut buf = vec![0u8; UDP_RECV_MAX];
    let (n, from) = self.inner.recv_from(&mut buf).await.map_err(|e| e.to_string())?;
    buf.truncate(n);
    Ok((buf, from.ip().to_string(), from.port()))
  }

  /// Allow sending to the broadcast address (`SO_BROADCAST`).
  pub fn set_broadcast(&self, on: bool) -> Result<(), String> {
    self.inner.set_broadcast(on).map_err(|e| e.to_string())
  }

  /// TTL for outgoing multicast (`1` keeps it on the local link).
  pub fn set_multicast_ttl(&self, ttl: u32) -> Result<(), String> {
    self.inner.set_multicast_ttl_v4(ttl).map_err(|e| e.to_string())
  }

  /// Whether multicast this socket sends loops back to sockets on this host.
  pub fn set_multicast_loop(&self, on: bool) -> Result<(), String> {
    self.inner.set_multicast_loop_v4(on).map_err(|e| e.to_string())
  }

  /// Join multicast `group` on the interface with address `iface` (`0.0.0.0` lets
  /// the OS choose). Required to receive that group's datagrams.
  pub fn join_multicast(&self, group: Ipv4Addr, iface: Ipv4Addr) -> Result<(), String> {
    self.inner.join_multicast_v4(group, iface).map_err(|e| e.to_string())
  }

  /// Leave a multicast group previously joined with `join_multicast`.
  pub fn leave_multicast(&self, group: Ipv4Addr, iface: Ipv4Addr) -> Result<(), String> {
    self.inner.leave_multicast_v4(group, iface).map_err(|e| e.to_string())
  }

  /// The bound local address (with the OS-assigned port when `0` was requested).
  pub fn local_addr(&self) -> String {
    self.inner.local_addr().map(|a| a.to_string()).unwrap_or_default()
  }
}

// ---- Interface enumeration --------------------------------------------------

/// One address bound to an interface.
pub struct IfAddr {
  pub ip: String,
  pub prefix: u8,
  /// `"v4"` or `"v6"`.
  pub family: &'static str,
}

/// A local network interface — the cross-platform replacement for parsing
/// `ip addr` to find the subnet to scan. Pick the first non-`loopback`, `up`
/// interface with a private IPv4 address and derive the CIDR from `ip` / `prefix`.
pub struct NetInterface {
  pub name: String,
  pub mac: Option<String>,
  pub up: bool,
  pub loopback: bool,
  pub multicast: bool,
  pub addrs: Vec<IfAddr>,
}

/// Enumerate local interfaces and their addresses (via `netdev`, which reads the
/// OS directly — no subprocess). Cross-platform incl. Android, where `/proc/net`
/// is restricted to apps but the netlink path `netdev` uses still works.
pub fn interfaces() -> Vec<NetInterface> {
  netdev::get_interfaces()
    .into_iter()
    .map(|iface| {
      let mut addrs = Vec::with_capacity(iface.ipv4.len() + iface.ipv6.len());
      for net in &iface.ipv4 {
        addrs.push(IfAddr { ip: net.addr().to_string(), prefix: net.prefix_len(), family: "v4" });
      }
      for net in &iface.ipv6 {
        addrs.push(IfAddr { ip: net.addr().to_string(), prefix: net.prefix_len(), family: "v6" });
      }
      NetInterface {
        name: iface.name.clone(),
        mac: iface.mac_addr.as_ref().map(|m| m.to_string()),
        up: iface.is_up(),
        loopback: iface.is_loopback(),
        multicast: iface.is_multicast(),
        addrs,
      }
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn probe_open_then_closed() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    assert_eq!(probe("127.0.0.1", port, 1000).await, Liveness::Open);
    drop(listener);
    // Nothing listening on a loopback port -> the kernel refuses (RST) -> Closed,
    // which still means "host up". This is the distinction the sweep relies on.
    assert_eq!(probe("127.0.0.1", port, 1000).await, Liveness::Closed);
  }

  #[tokio::test]
  async fn interfaces_include_loopback() {
    let ifaces = interfaces();
    assert!(ifaces.iter().any(|i| i.loopback), "expected a loopback interface");
    assert!(ifaces.iter().any(|i| i.addrs.iter().any(|a| a.ip == "127.0.0.1")), "expected 127.0.0.1 on some interface");
  }

  #[tokio::test]
  async fn udp_loopback_roundtrip() {
    let rx = udp_bind(0, true).await.unwrap();
    let port = rx.local_addr().rsplit(':').next().unwrap().parse::<u16>().unwrap();
    let tx = udp_bind(0, true).await.unwrap();
    tx.send(b"ping", "127.0.0.1", port).await.unwrap();
    let (data, _ip, _port) = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await.unwrap().unwrap();
    assert_eq!(data.as_slice(), b"ping");
  }
}
