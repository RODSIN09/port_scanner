/*
 * Author: Rod
 * Date: 13/10/2026
 * Filename: lib.rs mkII
 * Description: Scanning engine. Basically, all connections, handles outcomes of such, and returns the results printed from ports status.
 */
use std::io::ErrorKind;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::io::AsyncReadExt; // brings read() async. 
use tokio::time::timeout; 

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

pub async fn scan_port(ip: IpAddr, port: u16, dur: Duration) -> PortStatus {
    let socket = SocketAddr::new(ip, port);

    // tokio does not have connect_timeout: it wraps any future in timeout()
    match timeout(dur, TcpStream::connect(socket)).await {
        Err(_elapsed)   => PortStatus::Filtered,           // It finished the time
        Ok(Err(e))      => match e.kind() {                // Conects but failed
            ErrorKind::ConnectionRefused => PortStatus::Closed,
            _ => PortStatus::Error,
        },
        Ok(Ok(mut stream)) => {                            // Open
            let mut buf = [0; 256];
            let banner = match timeout(Duration::from_millis(300), stream.read(&mut buf)).await {
                Ok(Ok(n)) if n > 0 => Some(String::from_utf8_lossy(&buf[..n]).trim().to_string()),
                _ => None,
            };
            PortStatus::Open { service: get_service_name(port), banner }
        }
    }
}  
/*
 * What changes the most is that is an asyncronous function, therefore, its quite more complex; but rather
 * cleanlier syntactically. 
 */

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[tokio::test]
    async fn detects_open_port() {
        // 1. Start a listener on an OS-assigned port.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();

        // 2. Find out which port the OS assigned.
        let address = listener.local_addr().unwrap();

        // 3. Scan that port.
        let status = scan_port(
            address.ip(),
            address.port(),
            Duration::from_millis(200),
        ).await;

        // 4. Check the result. `{ .. }` means "an Open variant, I don't care
        //    about its fields" — a pattern must mirror the shape of the variant.
        assert!(matches!(status, PortStatus::Open { .. }));
    }
}