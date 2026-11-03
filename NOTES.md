# Learning notes

Working journal for the port scanner. The README says what the project *is*;
this file is what I learned while building it.

---

## 07/10/2026 — Started

First version: hardcoded `127.0.0.1`, ports 1–1023, sequential, printing every
result.

## 09/10/2026 — Types and errors

**Type inference works backwards.** I removed the `u16` annotation on the port
range and assumed I'd lost something. I hadn't — `SocketAddr::new` takes a
`u16`, so the compiler infers the range as `Range<u16>` from how it's *used*,
not from how it's declared. Changing the upper bound to 70000 is now a compile
error instead of a runtime surprise.

**`IpAddr` is an enum, not a generic.** It has two variants, `V4` and `V6`, each
holding an address. A generic is the `<T>` thing — `Vec<T>`, `Option<T>` — a
type with a hole the caller fills in. Different mechanism entirely. I need
`IpAddr` because that's what `SocketAddr::new` wants, so I wrap my `Ipv4Addr` in
`IpAddr::V4`.

**`match` is an expression.** Its arms are separated by commas because they're
items in a list, and the whole thing produces a value. That's why you can write
`let x = match ... { ... };` — the semicolon ends the `let`, not the arms.

**Exhaustiveness.** The compiler won't let me forget a variant. But `ErrorKind`
is marked `#[non_exhaustive]`, meaning std reserves the right to add more, so a
catch-all `_` arm is mandatory there even if I listed everything. `IpAddr` isn't
marked that way — there are only two IP versions and that's settled — so two
arms is enough.

**`?` propagates, it doesn't handle.** It means "if `Err`, return it to my
caller." Only usable in a function that itself returns `Result`, which is why
`main` had to change signature.

The distinction that mattered: a **closed port is not an error**, it's a result.
If I put `?` on `connect_timeout` the scanner would exit on the first closed
port. `?` is for "I can't continue" — bad input from the user. The `match` on
`e.kind()` is for "this is the answer I was looking for."

`Box<dyn Error>` is "any error at all" — `dyn` means the concrete type is
decided at runtime, `Box` puts it on the heap because the size isn't known at
compile time. `?` auto-converts into it, which is why one signature absorbs
parse errors and I/O errors alike.

`Ok(())` — `()` is the unit type, Rust's "no value". Not a typo.

## 11/10/2026 — Arguments, structure, threads

**Chose `..=` over `..`.** With exclusive bounds, `80 80` would scan nothing.
Also `RangeInclusive` handles `u16::MAX` correctly — a hand-rolled
`while p <= end { p += 1 }` would overflow at 65535 and either panic or loop
forever.

**Turbofish.** Earlier, `.parse()` knew what to produce because
`connect_timeout` constrained it. Parsing CLI args, nothing downstream does, so
I have to say it outright: `.parse::<IpAddr>()`.

**Parsed everything up front**, before any network traffic. The program either
rejects the input immediately or commits to the scan — much better than
discovering a bad end-port after 400 ports have gone out.

**Why the `PortStatus` enum was worth it.** Before: the result of a scan existed
for about three instructions as a string handed to `println!`, then it was gone.
I couldn't count it, sort it, filter it or return it. The enum turns "what
happened" from something I *did* into something I *have*. That's what makes
threads possible at all — a thread can't "print in the right place", it has to
hand a value back.

**`scan_port` can't print.** It returns a `PortStatus` and that's all it can do.
Printing moved out to the caller. This turned out to be exactly what was needed
before threads existed — several threads printing at once splices lines together
mid-word.

**Tests.** A test is a function with `#[test]` above it; it passes if it doesn't
panic. `#[cfg(test)]` means the test module only compiles when testing, so it
isn't in the real binary. The trick for testing an open port: bind a
`TcpListener` to port **0**, which asks the OS to pick any free port, then ask
it with `.local_addr()` which one it gave me. Guaranteed open, no collisions.
The listener has to stay in scope for the whole test — same `Drop` behaviour as
the `TcpStream` closing itself.

**Threads.**
- `thread::spawn` returns a `JoinHandle`; `.join()` blocks until that thread
  finishes. Without joining, `main` can exit and kill the threads mid-flight.
- `move` on the closure forces it to take ownership. Necessary because a thread
  might outlive the function that spawned it, so the compiler won't allow a
  borrow it can't prove stays valid. `IpAddr`, `u16` and `Duration` are all
  `Copy`, so each thread gets its own copy.
- `mpsc` = multi-producer, single-consumer. Clone the `Sender` once per thread,
  the `Receiver` lives in main.
- **`drop(tx)` before the receive loop.** The channel stays open as long as any
  sender exists. The original `tx` in main is a sender. Without dropping it, the
  `for received in rx` loop waits forever for a message that can never come.

**Thread stack size — corrected.** `std::thread::spawn` reserves **2 MiB** per
thread by default, not 8 MiB. The 8 MiB figure is the *main* thread's limit, set
by the OS (`ulimit -s`). So a 500-worker pool reserves ~1 GiB of *virtual*
address space — pages are only committed when touched, which is why 500 threads
didn't actually consume a gigabyte of RAM.

**Warnings.** Coming from C, I treat warnings as near-errors. Still a good habit,
but the reasoning differs: in C a warning often means undefined behaviour, in
Rust most of that class is a hard compile error instead. Rust warnings skew
toward "untidy or suspicious". Keep the count at zero anyway — so the one that
*does* matter (an ignored `Result`, an unreachable match arm) doesn't hide in
the noise.

## 12/10/2026 — Features, and two bugs of the same family

Added service-name lookup, banner grabbing, hostname resolution, result sorting,
open-only output with a summary count, and the `run()`/`main()` split so errors
print with `Display` instead of `Debug`.

**`cargo build` passing does not mean `cargo test` passes.** `#[cfg(test)]`
strips the test module out of a normal build, so test code is only type-checked
when `cargo test` actually runs. When `PortStatus::Open` gained fields, the test
pattern `PortStatus::Open` stopped matching anything — a pattern has to mirror
the shape of the variant — and the build stayed green for a whole day. The fix
is `PortStatus::Open { .. }`, where `..` means "this variant, don't care about
its fields" and doesn't need updating when a field is added later.

**Integer overflow behaves differently per build profile.** I wrote:

```rust
let total_ports = (end_port - start_port + 1) as usize;
```

`as` binds tighter than arithmetic, so that computes in `u16` and only then
widens. `65535 - 0 + 1` is 65536, which doesn't fit in 16 bits. Debug build:
panics with "attempt to add with overflow". Release build: wraps silently to 0,
scans nothing, reports success, exits 0. **The dangerous behaviour is in the
profile I'd actually ship.** Fix: widen the operands first,
`end_port as usize - start_port as usize + 1`.

**And a second bug hiding behind the first.** The next line was:

```rust
let num_workers = 500.min(total_ports as u16);
```

With `total_ports` now correctly 65536, `65536 as u16` is **0** — `as` between
integer types truncates, keeping only the low bits. `500.min(0)` is 0, so zero
workers spawn and the program reports success again. Fixing the first bug alone
would have changed nothing visible. Take the min in the wide type, narrow after:
`500.min(total_ports) as u16`. When the value comes from outside my control,
`u16::try_from(x)` returns a `Result` instead of silently truncating.

Lesson from both: `as` is a truncating cast, not a checked conversion.

**Kept `match` over `HashMap` for the service lookup.** A `match` on integer
literals compiles to a jump table with no allocation; a `HashMap` would have to
be built at runtime or stashed in a static. The leftover `use HashMap` import
was the only problem.

---

# 13/10/2026 — mk.II with Tokio

Rewrote the scanner as async. Kept mk.I in its own directory so both can be
benchmarked.

**Futures are lazy.** An `async fn` doesn't run when called — it returns a
future that does nothing until something polls it. Different from JavaScript,
where calling an async function starts it immediately.

**`.await` means "pause here and let the runtime do something else."** A thread
blocked in `connect_timeout` is parked by the kernel doing nothing. A task that
awaits yields control back, and the runtime immediately polls another task on
the same OS thread.

**The compiler rewrites an `async fn` into a state machine**, with one state per
`.await`. Measured: a future shaped like one of my scan tasks is **40 bytes** —
smaller than the 256-byte banner buffer it uses, because the layout only keeps
what's alive across suspension points. Against 2 MiB of thread stack, that's a
~52,000× ratio.

**Tokio has no `connect_timeout`.** Deadlines come from `tokio::time::timeout`,
which wraps *any* future. The results nest:

```rust
Result<Result<TcpStream, io::Error>, Elapsed>
```

- `Err(_elapsed)` → deadline passed → **Filtered**
- `Ok(Err(e))` → connection actively failed → match `e.kind()` → **Closed** / Error
- `Ok(Ok(stream))` → **Open**

This is a better design than std's. A deadline can now go on the banner read as
well, and hostname resolution + timeouts are no longer mutually exclusive the
way they were with `connect_timeout` vs `connect`.

**The worker pool disappeared.** No more `port % num_workers == worker_id`. One
task per port, spawned in a plain loop. The strided split only ever existed
because threads were expensive.

**But concurrency still needs capping — for a different reason.** Not stack
space: file descriptors. Each in-flight connection holds one, and `ulimit -n` is
often 1024. `tokio::sync::Semaphore` with 500 permits does it. The permit
releases on `Drop`, same RAII pattern as the socket closing itself and the
`TcpStream` in the `Ok(_)` arm — third time that mechanic has shown up.

**`Arc<Semaphore>` is my first real ownership problem.** `acquire()` borrows the
semaphore, so the permit can't outlive it; moving a permit *into* a spawned task
needs `acquire_owned()`, which is called on an `Arc`. `Arc` = atomically
reference-counted shared ownership, cloned per task. I got through the whole
threaded version without `Arc` because `IpAddr`/`u16`/`Duration` are all `Copy`.
A semaphore isn't.

**`JoinSet` replaced `Vec<JoinHandle>` + `mpsc`.** `join_next().await` returns
`Option<Result<T, JoinError>>` in completion order, so it *is* the collection
point — no channel needed. Dropping the set aborts every task still in it.

**`std::io::Read` is not `tokio::io::AsyncReadExt`.** Different trait, must be
imported explicitly, and the call becomes `.read(&mut buf).await`. Also
`set_read_timeout` doesn't exist on the async stream — the read gets wrapped in
`timeout()` instead. Second place that method vanished.

**`AsyncReadExt` is gated behind the `io-util` feature.** Not in `net`, not in
`io-std`. Without it the import fails with "no `AsyncReadExt` in `io`" and the
compiler helpfully suggests `AsyncRead`, which is the wrong trait (it's the
low-level one; `AsyncReadExt` is the extension trait with the convenience
methods). Took a while to spot because the error points at the import, not at
the Cargo.toml.

**Never call blocking code inside an async task.** A `std::thread::sleep` or a
blocking `connect` stalls the entire worker thread and every task scheduled on
it. This is why `std::net::ToSocketAddrs` got replaced with
`tokio::net::lookup_host` — the std version blocks.

# 14/10/2026
**Documenting** Documenting data and reorganising the .mds. 

**nmap** I compare with the loopback and my private network which I will document 
tomorrow due to the fact, I am quite tired. Also finished the basic project, 
I will continue the following days.

## Benchmarks — was Tokio worth it?

Loopback, 400 ms timeout, filtered ports created with `iptables -j DROP`:

| Scenario | mk.I (threads) | mk.II (Tokio) |
| --- | --- | --- |
| 65536 ports, refused instantly | 0.86–0.99 s | 0.98–1.27 s |
| 100 filtered, 500 concurrent | 0.43 s | 0.42 s |
| 5000 filtered, 500 concurrent | 4.08 s, 7.7 MB RSS | 4.06 s, 8.0 MB RSS |
| 5000 filtered, **8000 concurrent** | 0.95 s, 50.9 MB RSS, 11.3 GB virt | **0.54 s, 9.5 MB RSS, 205 MB virt** |

**At 500 concurrency they're indistinguishable, and against an instantly-refusing
host the threaded version is marginally faster** — there's no waiting to overlap,
so scheduling overhead dominates. I expected async to win everywhere; it doesn't.

The difference only appears when concurrency goes up. At 8000 in flight: 1.8×
faster, 5× less resident memory, 55× less virtual address space.

So the honest conclusion is **not** "async is faster." It's "async makes high
concurrency affordable, and high concurrency is what's fast." At the scale my
scanner actually runs at, the rewrite bought nothing in performance — it bought
the headroom to raise the limit, and a much simpler structure (no worker IDs, no
striding, no channel).

## Still to do

- Report filtered and closed counts, not just open and error. Filtered is
  interesting information — it means something is firewalling.
- `127.0.0.1:8080` as a target still fails. It becomes `127.0.0.1:8080:80` and
  won't resolve. Either strip a trailing port or reject it with a clear message.
- One `unwrap()` left, on `acquire_owned()`. It can only fail if the semaphore
  is closed, which never happens here — but it's the last one.
- If any task panics, `results.push(res?)` propagates the `JoinError` and the
  entire scan's results are discarded. Worth deciding whether that's wanted.

**Optional.** UDP scanning — no handshake, so "no reply" means
open-or-filtered-or-dropped and distinguishing them needs ICMP port-unreachable
messages, raw sockets and root. Teaches networking more than it teaches Rust.
`clap` for argument parsing, now that hand-rolling it has shown what it does.

## What this project never taught me

Worth knowing, for picking what to do next. A port scanner is flat data, no
shared state, everything `Copy` — so it dodged:

- **Generics** — never had to write one.
- **Lifetimes** — only `&'static str`; never annotated one myself.
- **Writing traits** — used `Read`, `Error`, `FromStr`; defined none.
- **The borrow checker in anger** — `Arc<Semaphore>` was the first real taste.
- **Custom error types** — `Box<dyn Error>` absorbed everything.
- **Iterators** — barely used any adaptors.
