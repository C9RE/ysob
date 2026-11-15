//! The agents' door, as `YSOB_SOCK` names it. On unix it is an owner-only socket file;
//! where there are no unix sockets (Windows) it is a loopback TCP port, written `tcp:127.0.0.1:54321`.
//! Either way every request carries the agent's own token, and the hub checks it.

use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
pub enum Door {
    Tcp(String),
    Unix(PathBuf),
}

/// Read a `YSOB_SOCK` value. Only loopback TCP addresses are accepted: the door never leaves the machine.
pub fn parse(s: &str) -> Option<Door> {
    let s = s.trim();
    if let Some(addr) = s.strip_prefix("tcp:") {
        let sa: std::net::SocketAddr = addr.parse().ok()?;
        return (sa.ip().is_loopback() && sa.port() != 0).then(|| Door::Tcp(sa.to_string()));
    }
    (!s.is_empty()).then(|| Door::Unix(PathBuf::from(s)))
}

/// One JSON line out, one JSON line back. `None` if the door is shut or the reply is unreadable.
pub fn ask(door: &str, req: &Value) -> Option<Value> {
    let line = format!("{req}\n");
    let mut reply = String::new();
    match parse(door)? {
        Door::Tcp(addr) => {
            let mut s = std::net::TcpStream::connect(addr).ok()?;
            s.write_all(line.as_bytes()).ok()?;
            BufReader::new(s).read_line(&mut reply).ok()?;
        }
        #[cfg(unix)]
        Door::Unix(path) => {
            let mut s = std::os::unix::net::UnixStream::connect(path).ok()?;
            s.write_all(line.as_bytes()).ok()?;
            BufReader::new(s).read_line(&mut reply).ok()?;
        }
        #[cfg(not(unix))]
        Door::Unix(_) => return None,
    }
    serde_json::from_str(&reply).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_both_kinds_of_door() {
        assert_eq!(parse("tcp:127.0.0.1:54321"), Some(Door::Tcp("127.0.0.1:54321".into())));
        assert_eq!(parse("tcp:[::1]:9"), Some(Door::Tcp("[::1]:9".into())));
        assert_eq!(parse("/home/x/.ysob/lighthouse.sock"), Some(Door::Unix("/home/x/.ysob/lighthouse.sock".into())));
        assert_eq!(parse(""), None);
    }

    #[test]
    fn a_tcp_door_must_be_loopback_with_a_real_port() {
        assert_eq!(parse("tcp:192.168.1.5:54321"), None);
        assert_eq!(parse("tcp:0.0.0.0:54321"), None);
        assert_eq!(parse("tcp:127.0.0.1:0"), None);
        assert_eq!(parse("tcp:localhost:54321"), None, "names are not resolved, only addresses");
        assert_eq!(parse("tcp:127.0.0.1"), None);
    }

    #[test]
    fn a_shut_door_gives_no_answer() {
        assert_eq!(ask("tcp:127.0.0.1:1", &serde_json::json!({})), None);
        assert_eq!(ask("", &serde_json::json!({})), None);
    }
}
