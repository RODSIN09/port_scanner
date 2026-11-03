# port_scanner

A TCP port scanner written from scratch in Rust, as a learning project.

Idea taken from [rustify.rs — Best Rust Projects for Beginners 2026](https://rustify.rs/articles/best-rust-projects-for-beginners-2026). All credit for the core idea goes to them.

## Usage

```bash
cargo run --release -- <target-ip> <start-port> <end-port>
```

Example:

```bash
cargo run --release -- 127.0.0.1 1 1024
```

Use `--release`. Debug builds are significantly slower.

Arguments are positional and all three are required. The target must be an IP
address — hostnames are not supported yet (see Roadmap).

## What is a port scanner?

A port scanner probes a host to find out which ports are open. If an IP address
is the street address of an apartment building, the ports are the individual
apartment doors. The scanner walks down the hallway and knocks on each one
(there are 65535 of them) to see who answers.

## How this one works

TCP connections are established with a three-way handshake:

1. **SYN** — the client asks to open a connection.
2. **SYN-ACK** — the server agrees. (Or **RST**, if nothing is listening.)
3. **ACK** — the client confirms, and the connection is open.

This scanner performs a **TCP connect scan**: it asks the operating system to
open a normal connection with `TcpStream::connect_timeout`, which completes the
full handshake, and then immediately drops it. Rust closes the socket
automatically when the `TcpStream` goes out of scope.

Three outcomes, distinguished by inspecting the error kind:

| Result | What happened |
| --- | --- |
| **Open** | The connection succeeded. |
| **Closed** | `ConnectionRefused` — the host replied with RST. Something is there, but not on that port. |
| **Filtered** | `TimedOut` — no reply at all, usually a firewall dropping packets silently. |

Note that this is *not* a SYN scan (nmap's `-sS`), which sends a raw SYN and
never completes the handshake. That requires crafting packets by hand, raw
sockets, and root privileges. A connect scan is noisier — it shows up in the
target's logs as a real connection — but it needs no special permissions.

## Concurrency

Scanning sequentially means waiting out the timeout on every filtered port, one
after another. Against a firewalled host that's roughly 200 ms × the number of
ports.

The scan is I/O-bound — the CPU is idle almost the whole time — so it
parallelises well. Each port is currently scanned on its own thread, with
results sent back to the main thread over an `mpsc` channel.

## mk.I - OS threads
A fixed pool of 500 stf::thread workers. Worker i takes every port where port % num_workers
== i (a strided split). Each worker scans with TcpStreams::connect_timeout and sends 
results to the main thread over an mpsc channel. 

One thread per port does not scale, std::thread reserves 2MiB of stack a piece, so 
65535 of them is unworkable. Hence the pool.

## mk.II - Tokio

One async task per port, so the strided worker split disappears entirely - A task costs 
tens of bytes rather than megabytes of stack. Concurrency is capped instead by a 
tokio::sync::Semaphore with 500 permits, for a different reason: each in-flight connection holds a file descriptor, and ulimit -n is often 1024. 

Tasks are collected in a JoinSet rather than a Vector, plus a channel since join_next() 
already hands back results in completion order. Tokio has not connect_timeout. Deadlines
come from wrapping any future in tokio::time::timeout, which nests the results:
Result<Result<TcpStream, io::Error>, Elapsed>

## but.. was it worth it?

On loopback, 400ms timeout, three runs each. 

65536 ports, all refused instantly - mk.I (0.86-0.99)s - mk.II (0.98-1.27)s
100 filtered ports, 500 concurrent - mk.I 0.43s - mk.II 0.42s
5k filtered ports, 500 concurrent - mk.I (4.08s and 7.7MB RSS) - mk.II (4.06s and 8.0MB RSS)
5k filtered ports, 8K concurrent - mk.I (0.95s, 50.9MB RSS and 11.3GB virtual) - mk.II (0.54s, 9.5MB RSS, 205 MB virtual).

At first with 500, we could see that they were quite similar, the threaded version is marginally faster. 

The difference only appears when concurrency goes up. At 8000 Tokio is 1.8x faster, 5x less resident memory and 55x less virtual adress space. 
That's the argument we were looking for!. 

## Project layout

```
mk.I/src/
  lib.rs    scan_port() and PortStatus, plus tests
  main.rs   argument parsing, thread pool, output
mk.II/src/
  lib.rs    async scan_port() and PortStatus, plus tests
  main.rs   argument parsing, JoinSet + Semaphore, output
```

Run the tests with `cargo test`.

## Roadmap

- [x] Build `SocketAddr` directly instead of formatting and re-parsing strings
- [x] Distinguish closed from filtered via `ErrorKind`
- [x] Proper error handling with `Result` and `?` instead of `unwrap()`
- [x] Command-line arguments
- [x] Split into a library, add a `PortStatus` enum
- [x] Tests
- [x] Concurrency with `std::thread` and channels
- [x] Sort results by port and report only open ones, with a summary count
- [x] Thread pool — one thread per port does not scale to 65535
- [x] Friendlier error output (`Display` rather than `Debug`)
- [x] Service name lookup for well-known ports
- [x] Banner grabbing — read what a service announces on connect
- [x] Hostname resolution, so `scanme.nmap.org` works as a target
- [x] **mk.II** — rewrite with the Tokio async runtime
- [x] Benchmark mk.I against mk.II
- [x] Compares results against nmap (nmap -sT -p 1-1024 <target> and diff)
- [ ] Report filtered/closed counts, not just open and error
- [ ] Accept a target that already contains a port, or reject it clearly.

## A note on legality

Only scan hosts you own or have explicit permission to scan. Port scanning machines you don't control is illegal in Spain and in most other places, regardless of intent.

scanme.nmap.org is maintained by the nmap project specifically as a legal target for this kind of learning. Don't hammer it.
