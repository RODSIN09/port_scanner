# Commands

## Both versions

```bash
cargo init                  # NOT "innit" — scaffolds a new crate
cargo build                 # debug build: slow, but panics on integer overflow
cargo build --release       # optimized: what to use for anything timed
cargo run --release -- 127.0.0.1 1 1024
cargo test                  # runs #[test] / #[tokio::test] functions
cargo clippy --all-targets  # linter — the best Rust teacher available
cargo fmt                   # formatter; `cargo fmt --check` just shows the diff
cargo clean                 # delete target/ (it gets big)
rustc --version
```

`--all-targets` on clippy matters: without it, test code isn't linted.

## mk.II — Tokio setup

Nothing to install. Tokio is a library, so it goes in `Cargo.toml` and cargo
fetches it on the next build:

```toml
[dependencies]
tokio = { version = "1.53", features = [
    "rt-multi-thread",   # work-stealing scheduler across cores
    "macros",            # #[tokio::main], #[tokio::test]
    "net",               # tokio::net::TcpStream
    "time",              # tokio::time::timeout  (without it: runtime panic)
    "sync",              # Semaphore, async channels
    "io-util",           # AsyncReadExt — needed for the banner grab
] }
```

Or `features = ["full"]` to stop thinking about it; the only cost is compile
time.

Every Tokio feature is off by default. `io-util` is the easy one to miss — it's
what gates `AsyncReadExt`, and leaving it out gives "no `AsyncReadExt` in `io`"
pointing at the import rather than at Cargo.toml.

```bash
cargo tree          # see what got pulled in
cargo add tokio --features rt-multi-thread,macros,net,time,sync,io-util
```

## Not needed for this project

The [Tokio tutorial setup page](https://tokio.rs/tokio/tutorial/setup) asks you
to install `mini-redis`:

```bash
cargo install mini-redis
mini-redis-server
mini-redis-cli get foo
```

That's only for the tutorial's own example app — the tutorial builds a Redis
clone, and `mini-redis` is the reference server to compare against. The port
scanner doesn't use it. Skip it unless working through the tutorial chapters
directly.

(Worth keeping in mind as a *next* project though — a key-value server is the
other side of the socket from a scanner, and reuses everything learned here.)

## Testing the scanner against something real

Open a listener in another terminal and confirm the scanner finds it — testing
that the tool detects a thing known to be there:

```bash
nc -l 4444                     # then: cargo run --release -- 127.0.0.1 4440 4450
python3 -m http.server 8000    # a chatty-ish service on 8000
```

To test the **filtered** path, localhost won't do — loopback refuses instantly,
so the timeout never fires. Either scan a real firewalled host, or create a
genuinely dropped port range locally (needs root):

```bash
sudo iptables -A INPUT -p tcp -d 127.0.0.1 --dport 30000:30099 -j DROP
# ... run the scan; those 100 ports now report FILTERED ...
sudo iptables -D INPUT -p tcp -d 127.0.0.1 --dport 30000:30099 -j DROP
```

Remember to delete the rule afterwards.

## Validation against nmap

```bash
nmap -sT -p 1-1024 127.0.0.1   # -sT is a connect scan: the same thing this does
```

Diff the open ports against the scanner's output. If they disagree, one of them
is wrong, and finding out which is the point.

## System limits worth knowing

```bash
ulimit -n     # max open file descriptors — caps concurrent connections
ulimit -s     # main thread stack size in KB (usually 8192 = 8 MiB)
ulimit -u     # max user processes/threads
```

`std::thread` reserves 2 MiB per spawned thread regardless of `ulimit -s`; that
limit applies to the main thread only.

## Legality

Only scan hosts you own or have permission to scan. `scanme.nmap.org` is
maintained by the nmap project as a legal target for exactly this kind of
learning — don't hammer it.
