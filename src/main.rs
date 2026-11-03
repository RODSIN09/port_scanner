/*
 * Author: Rod
 * Date: 13/10/2026
 * Filename: main.rs - mkII 
 * Description: Main function for the portscanner project mkII (tokio).
 */

// Libraries Imported:

use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use port_scanner::{scan_port, PortStatus};

// The distribution dissapears by port % num_workers, the mpsc and join as above.

struct ScanResult {
    port: u16, 
    status: PortStatus,
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("\nExecution Error: {}", err);
        eprintln!("\nUsage: cargo run -- <Target-Host-or-IP> <Start-Port> <End-Port>");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);

    let target_input = args.next().ok_or("Missing target Hostname or IP address.")?;
    let start_port = args.next().ok_or("Missing starting port range value.")?.parse::<u16>()?;
    let end_port = args.next().ok_or("Missing ending port range value.")?.parse::<u16>()?;

    if start_port > end_port {
        return Err("Starting port configuration must not exceed the target ending port.".into());
    }

    println!("Resolving target host registration details...");
    // DNS Resolution: to_socket_addrs() BLOCKS therefore it uses an async version:
    let ip = tokio::net::lookup_host(format!("{}:80", target_input)).await?
        .next().ok_or("Could not resolve")?.ip();

    // Widen to usize BEFORE the arithmetic. (end - start + 1) in u16 overflows
    // when the range is the full 0..=65535, because 65536 does not fit in a u16.
    let total_ports = end_port as usize - start_port as usize + 1;
    let timeout = Duration::from_millis(400);

    // Limitator: as much as 500 connections open at the same time.
    // Each open connection uses a file descriptor; many systems cap them at 1024 (ulimit -n).
    let max_concurrent = 500;
    let sem = Arc::new(Semaphore::new(max_concurrent));

    println!("Scanning target address {} with up to {} concurrent connections...", ip, max_concurrent);
    let mut set = JoinSet::new();

    for port in start_port..=end_port {
        let sem = sem.clone();
        set.spawn(async move {
            let _permit = sem.acquire_owned().await.unwrap(); // espera turno
            ScanResult { port, status: scan_port(ip, port, timeout).await }
        });                                                   // _permit se libera al terminar
    }

    let mut results = Vec::with_capacity(total_ports);
    while let Some(res) = set.join_next().await {
        results.push(res?);
    }

    // Point 1: Sort cleanly by numerical port sequence layout
    results.sort_by_key(|r| r.port);

    // Point 1: Output ONLY open ports and generate a dynamic total summary count tracking job metrics
    let mut open_count = 0;
    let mut error_count = 0;
    println!("\n--- Discovery Manifest ---");
    for item in results {
        match item.status {
            PortStatus::Open { service, banner } => {
                open_count += 1;
                print!("Port {:5} | OPEN | Service: {}", item.port, service);
                if let Some(banner_str) = banner {
                    print!(" | Banner: [{}]", banner_str);
                }
                println!();
            }
            PortStatus::Error => error_count += 1,
            PortStatus::Closed | PortStatus::Filtered => {}
        }
    }

    println!("\nScan Complete: Discovery identified {} open connections out of {} target endpoints requested.", open_count, total_ports);
    if error_count > 0 {
        println!("Warning: {} ports returned unexpected errors (too many open files? try a lower concurrency).", error_count);
    }

    Ok(())
}