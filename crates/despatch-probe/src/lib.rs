#![doc = "Reachability checks for Despatch servers, without authenticating."]
#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc)]

use std::io::{ErrorKind, Read};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use despatch_application::{HostProbe, PortError, Reachability, ReachabilityRequest};

/// How long the probe waits for the server to volunteer its identification string.
///
/// An SSH server sends it unprompted the moment the connection is up, so a short wait either
/// catches it or tells us the port is running something else. The check never writes to the
/// socket, so a silent service is reported as open rather than being poked.
const BANNER_TIMEOUT: Duration = Duration::from_millis(1_200);
/// The identification string is capped by RFC 4253 at 255 bytes including its line ending.
const BANNER_LIMIT: usize = 255;

/// Checks a host by opening a TCP connection to it and reading what it says first.
#[derive(Clone, Copy, Debug, Default)]
pub struct TcpHostProbe;

impl HostProbe for TcpHostProbe {
    fn probe(&self, request: &ReachabilityRequest) -> Result<Reachability, PortError> {
        // Resolution failures are a fact about the host, not a failure of the check itself.
        let Ok(addresses) = (request.host.as_str(), request.port.get()).to_socket_addrs() else {
            return Ok(Reachability::Unresolved);
        };

        // A host with several addresses is reachable if any one of them answers, so every address
        // is tried and only the last verdict is kept.
        let mut outcome = Reachability::Unresolved;
        for address in addresses {
            match TcpStream::connect_timeout(&address, request.timeout) {
                Ok(stream) => {
                    return Ok(Reachability::Open {
                        service: read_service(&stream),
                    });
                }
                Err(error) => outcome = classify(&error),
            }
        }
        Ok(outcome)
    }
}

/// Turns a connect failure into a verdict.
///
/// Anything that is not an outright refusal is reported as a timeout: from here, a filtered port,
/// an unroutable network, and a host that is simply off are indistinguishable, and claiming
/// otherwise would be guessing.
fn classify(error: &std::io::Error) -> Reachability {
    if error.kind() == ErrorKind::ConnectionRefused {
        Reachability::Refused
    } else {
        Reachability::TimedOut
    }
}

/// Reads the identification string the server sends first, if it sends one in time.
///
/// Nothing is written to the socket and the result is advisory: a `None` here means "did not
/// introduce itself", never "unreachable".
fn read_service(stream: &TcpStream) -> Option<String> {
    if stream.set_read_timeout(Some(BANNER_TIMEOUT)).is_err() {
        return None;
    }
    let mut buffer = [0_u8; BANNER_LIMIT];
    let mut stream = stream;
    let read = stream.read(&mut buffer).ok()?;
    let line = String::from_utf8_lossy(&buffer[..read]);
    let line = line.lines().next()?.trim();
    if line.is_empty() {
        None
    } else {
        Some(line.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::Duration;

    use despatch_application::{HostProbe, Reachability, ReachabilityRequest};
    use despatch_domain::SshPort;

    use super::TcpHostProbe;

    fn request(port: u16) -> ReachabilityRequest {
        ReachabilityRequest {
            host: "127.0.0.1".to_owned(),
            port: SshPort::new(port).expect("non-zero port"),
            timeout: Duration::from_secs(2),
        }
    }

    #[test]
    fn a_listening_port_reports_the_identification_string_it_sends() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            stream
                .write_all(b"SSH-2.0-OpenSSH_9.6\r\n")
                .expect("write banner");
        });

        let outcome = TcpHostProbe.probe(&request(port)).expect("probe ran");
        server.join().expect("server thread");

        assert_eq!(
            outcome,
            Reachability::Open {
                service: Some("SSH-2.0-OpenSSH_9.6".to_owned())
            }
        );
    }

    #[test]
    fn a_silent_service_is_still_reachable() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            // Hold the connection open long enough for the banner read to time out.
            thread::sleep(Duration::from_millis(1_500));
            drop(stream);
        });

        let outcome = TcpHostProbe.probe(&request(port)).expect("probe ran");
        server.join().expect("server thread");

        assert_eq!(outcome, Reachability::Open { service: None });
    }

    #[test]
    fn a_closed_port_is_refused_rather_than_reported_as_an_error() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);

        assert_eq!(
            TcpHostProbe.probe(&request(port)).expect("probe ran"),
            Reachability::Refused
        );
    }

    #[test]
    fn a_hostname_that_does_not_resolve_is_reported_as_such() {
        let outcome = TcpHostProbe
            .probe(&ReachabilityRequest {
                host: "despatch.invalid".to_owned(),
                port: SshPort::DEFAULT,
                timeout: Duration::from_secs(1),
            })
            .expect("probe ran");

        assert_eq!(outcome, Reachability::Unresolved);
    }

    #[test]
    fn the_probe_never_writes_to_the_socket() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            stream
                .set_read_timeout(Some(Duration::from_millis(600)))
                .expect("read timeout");
            let mut buffer = [0_u8; 64];
            let mut stream: &TcpStream = &stream;
            // Anything other than end-of-stream or a timeout means the probe sent something.
            match std::io::Read::read(&mut stream, &mut buffer) {
                Ok(0) | Err(_) => 0,
                Ok(read) => read,
            }
        });

        let _ = TcpHostProbe.probe(&request(port)).expect("probe ran");
        let written_by_probe = server.join().expect("server thread");

        assert_eq!(written_by_probe, 0, "a check must stay read-only");
    }
}
