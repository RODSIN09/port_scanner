/*
 * Author: Rod
 * Date: 11/10/2026 
 * Modifications: 12/10/2026
 * Modifications:
 * Filename: lib.rs
 * Description: Scanning engine. Basically, all connections, handles outcomes of such, and returns the results printed from ports status.
 */
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::Duration;
use std::io::{ErrorKind, Read};

#[derive(Debug, Clone)]
pub enum PortStatus {
    Open { service: &'static str, banner: Option<String> },
    Closed,
    Filtered,
    Error,
}

// Feature A: Service Name Lookup Dictionary
pub fn get_service_name(port: u16) -> &'static str {
    match port {
        22 => "SSH",
        23 => "Telnet",
        25 => "SMTP",
        53 => "DNS",
        80 => "HTTP",
        110 => "POP3",
        143 => "IMAP",
        443 => "HTTPS",
        3306 => "MySQL",
        8080 => "HTTP-Proxy",
        _ => "Unknown Service",
    }
}

pub fn scan_port(ip: IpAddr, port: u16, timeout: Duration) -> PortStatus {
    let socket = SocketAddr::new(ip, port);

    // Feature C: connect_timeout accepts a strict SocketAddr, allowing us to control network delays
    match TcpStream::connect_timeout(&socket, timeout) {
        Ok(mut stream) => {
            let service = get_service_name(port);
            
            // Feature B: Banner Grabbing
            // Set a short read timeout so we don't hang forever waiting for quiet ports to talk
            let banner = if stream.set_read_timeout(Some(Duration::from_millis(300))).is_ok() {
                let mut buffer = [0; 256];
                if let Ok(bytes_read) = stream.read(&mut buffer) {
                    if bytes_read > 0 {
                        Some(String::from_utf8_lossy(&buffer[..bytes_read]).trim().to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            PortStatus::Open { service, banner }
        }
        Err(e) => match e.kind() {
            ErrorKind::ConnectionRefused => PortStatus::Closed,
            ErrorKind::TimedOut => PortStatus::Filtered,
            _ => PortStatus::Error,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn detects_open_port() {
        // 1. Start a listener on an OS-assigned port.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();

        // 2. Find out which port the OS assigned.
        let address = listener.local_addr().unwrap();

        // 3. Scan that port.
        let status = scan_port(
            address.ip(),
            address.port(),
            Duration::from_millis(200),
        );

        // 4. Check the result. `{ .. }` means "an Open variant, I don't care
        //    about its fields" — a pattern must mirror the shape of the variant.
        assert!(matches!(status, PortStatus::Open { .. }));
    }
}