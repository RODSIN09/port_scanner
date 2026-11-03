/*
 * Author: Rod
 * Date: 07/10/2026
 * Modifications: 09/10/2026, 11/10/2026, 12/10/2026
 * I believe the date is prior, yesterday or before, but I properly started today.
 * Filename: main.rs
 * Description: Main function for the portscanner project!
 */

// Libraries Imported:

use std::env;
use std::sync::mpsc;
use std::net::{IpAddr, ToSocketAddrs};
use std::time::Duration;
use std::thread;
use port_scanner::{scan_port, PortStatus};

struct ScanResult {
    port: u16, 
    status: PortStatus,
}

// Point 3: main catches errors and prints them using Display {} instead of Debug {:?}
fn main() {
    if let Err(err) = run() {
        eprintln!("\nExecution Error: {}", err);
        eprintln!("\nUsage: cargo run -- <Target-Host-or-IP> <Start-Port> <End-Port>");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);

    let target_input = args.next().ok_or("Missing target Hostname or IP address.")?;
    let start_port = args.next().ok_or("Missing starting port range value.")?.parse::<u16>()?;
    let end_port = args.next().ok_or("Missing ending port range value.")?.parse::<u16>()?;

    if start_port > end_port {
        return Err("Starting port configuration must not exceed the target ending port.".into());
    }

    // Feature C: Hostname Resolution
    // Append a dummy port to resolve arbitrary DNS domains to concrete IP targets
    println!("Resolving target host registration details...");
    let target_host = format!("{}:80", target_input);
    let generic_ip = match target_host.to_socket_addrs() {
        Ok(mut addrs) => addrs.next().map(|s| s.ip()).ok_or("Could not resolve domain name mapping.")?,
        Err(_) => target_input.parse::<IpAddr>().map_err(|_| "Target input is neither a valid IP address nor a resolvable domain name.")?,
    };

    // Widen to usize BEFORE the arithmetic. (end - start + 1) in u16 overflows
    // when the range is the full 0..=65535, because 65536 does not fit in a u16.
    let total_ports = end_port as usize - start_port as usize + 1;
    let timeout = Duration::from_millis(400);

    // Fixed worker pool, so we don't spawn one OS thread per port.
    // Take the min in usize, THEN narrow: `total_ports as u16` would truncate
    // 65536 to 0 and leave us with zero workers.
    let num_workers = 500.min(total_ports) as u16;
    
    println!("Scanning target address {} using {} worker threads...", generic_ip, num_workers);

    let (tx, rx) = mpsc::channel();
    let mut handles = Vec::new();

    for worker_id in 0..num_workers {
        let tx_clone = tx.clone();
        
        // Strided strategy: worker_id scans every port matching (port % num_workers == worker_id)
        let handle = thread::spawn(move || {
            for port in start_port..=end_port {
                if port % num_workers == worker_id {
                    let status = scan_port(generic_ip, port, timeout);
                    if tx_clone.send(ScanResult { port, status }).is_err() {
                        break; // Main pipeline dropped receiver
                    }
                }
            }
        });
        handles.push(handle);
    }
    
    // Explicitly drop original sender clone so receiver knows when thread streams conclude
    drop(tx);

    // Point 1: Collect results asynchronously into a vector first
    let mut collected_results = Vec::with_capacity(total_ports);
    for received in rx {
        collected_results.push(received);
    }

    // Point 1: Sort cleanly by numerical port sequence layout
    collected_results.sort_by_key(|r| r.port);

    // Point 1: Output ONLY open ports and generate a dynamic total summary count tracking job metrics
    let mut open_count = 0;
    println!("\n--- Discovery Manifest ---");
    for item in collected_results {
        if let PortStatus::Open { service, banner } = item.status {
            open_count += 1;
            print!("Port {:5} | OPEN | Service: {}", item.port, service);
            if let Some(banner_str) = banner {
                print!(" | Banner: [{}]", banner_str);
            }
            println!();
        }
    }

    println!("\nScan Complete: Discovery identified {} open connections out of {} target endpoints requested.", open_count, total_ports);

    // Synchronize worker handles cleanly
    for handle in handles {
        handle.join().expect("An active scanner thread panicked during network verification runs.");
    }

    Ok(())
}