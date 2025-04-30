# 🏦 DST Demo: Bank App

This repository demonstrates **Deterministic Simulation Testing (DST)**. It's a fully self-contained project built around a simple bank application that communicates over TCP and supports basic transaction operations. Through DST it uncovers complex bugs like the infamous **"[epochalypse](https://en.wikipedia.org/wiki/Year_2038_problem)"** bug.

---

## 🚀 What’s Inside

- **Bank App over TCP**  
  A small simulated bank server that lets clients:

  - Create transactions
  - Void transactions
  - Get a transaction by ID
  - List all transactions

- **Deterministic Simulation Framework**  
  Powered by [`tokio-turmoil`](https://github.com/tokio-rs/turmoil), the app runs in a fully controlled, simulated network and time environment.

- **Interaction Plans**  
  Test scenarios are encoded as **InteractionPlans**, describing sequences of client actions over simulated time, exposing edge cases and time-sensitive bugs.

- **Epochalypse Bug**  
  A subtle and illustrative bug involving time-based logic that is reliably reproduced and diagnosed thanks to deterministic testing.

---

## 🧪 Simulator Overview

This simulator orchestrates deterministic simulation testing (DST) using the `tokio-turmoil` framework to uncover concurrency and failure edge cases in the TCP-based bank server application.

### 🧩 Architecture

The simulation involves two main components:

#### 🖥️ Host Server (`host`)

Simulates the real TCP bank server within the turmoil simulation. It processes client requests to create, void, get, and list transactions, using simulated time and deterministic execution to model realistic server behavior under network conditions and failures.

#### 🧑‍🤝‍🧑 Clients (`client`)

Simulated bank clients that execute a series of planned interactions (via `InteractionPlan`) with the host server. These clients mimic real-world usage by sending timed and possibly conflicting requests, helping to uncover bugs like race conditions or consistency errors. Each client runs in a fully simulated environment with deterministic timing and networking, allowing for reproducible stress testing and debugging.

There are 3 clients that interact with the host:

##### 💼 Banker

Acts as a realistic user of the bank system. Executes a sequence of operations (e.g. create, void, get, list transactions) based on an `InteractionPlan`, simulating regular user traffic and transaction workflows.

##### 💥 Fault Injector

Deliberately introduces simulated network partitions, crashes, and restarts to test the system's resilience and recovery. Useful for verifying that transaction state remains consistent despite faults.

##### 🩺 Health Checker

Periodically pings the server to verify its responsiveness and uptime. Ensures that faults or bugs don't silently break the system's liveness guarantees.

---

## 🧑‍💻 Usage Instructions

### Prerequisites

- Rust
- Cargo

This project includes three main executables:

- 🏦 A **real TCP server** (`dst_demo_server`)
- 🎛 A **real TCP client** for manual interaction: (`dst_demo_tcp_client`)
- 🧪 A **deterministic simulator** (`dst_demo_server_simulator`)

### 🏁 Running the Real Server

To run the actual bank application (not in simulation), use:

```bash
cargo run --release -p dst_demo_server
```

By default, this starts the TCP server on `0.0.0.0:3000`.

#### 🔧 Optional Environment Variables

- `PORT` – override the default port (`3000`)
- `ADDR` – override the address to bind to (default: `0.0.0.0`)
- `RUST_LOG` – control log verbosity (`trace`, `debug`, `info`, `warn`, `error`)

##### Example:

```bash
PORT=4000 RUST_LOG=info cargo run --release -p dst_demo_server
```

### 🎛 Running the TCP Client

You can use the provided client to manually interact with the running bank server:

```bash
cargo run --release -p dst_demo_tcp_client 127.0.0.1:3000
```

Replace `127.0.0.1:3000` with the appropriate server address if needed.

Once connected, you can issue the following commands:

- `CREATE_TRANSACTION` - Prompts for the amount (decimal) and returns the new transaction details.
- `VOID_TRANSACTION` - Prompts for the transaction ID (integer) and returns the updated voided transaction.
- `GET_TRANSACTION` - Prompts for the transaction ID (integer) and returns its details, if it exists.
- `LIST_TRANSACTIONS` - Lists all transactions currently stored in the bank.

### 🧪 Running the Simulator

To run the deterministic simulation using turmoil:

```bash
cargo run --release -p dst_demo_server_simulator
```

This will execute a series of predefined interaction plans in a fully simulated environment.

#### 🔧 Optional Environment Variables

- `SIMULATOR_SEED` – set a specific seed to make a test run reproducible
- `SIMULATOR_DURATION` – max steps to simulate before success is assumed
- `SIMULATOR_STEP_MULTIPLIER` – control how fast simulated time moves (higher = faster)
- `SIMULATOR_EPOCH_OFFSET` – control the initial time offset in millis
- `SIMULATOR_RUNS` – control how many simulations will run
- `SIMULATOR_MAX_PARALLEL` – control how many threads are allowed to be spun up to run simulations on
- `SIMULATOR_BANKER_COUNT` – control how many banker clients will be used to interact with the simulated server host
- `RUST_LOG` – control log verbosity (`trace`, `debug`, `info`, `warn`, `error`)

##### Example:

```bash
SIMULATOR_SEED=123 \
    SIMULATOR_DURATION=1000 \
    SIMULATOR_STEP_MULTIPLIER=10 \
    SIMULATOR_EPOCH_OFFSET=1745529640464 \
    SIMULATOR_RUNS=100 \
    SIMULATOR_MAX_PARALLEL=8 \
    SIMULATOR_BANKER_COUNT=15 \
    RUST_LOG="debug" \
    cargo run --release -p dst_demo_server_simulator
```

---

## 🧪 Why Deterministic Testing?

Real-world distributed systems can be hard to test due to:

- Network flakiness
- Time-sensitive logic
- Race conditions

By using turmoil’s simulation of network and time, this project:

- **Eliminates flakiness**: every test run is deterministic.
- **Enables time travel**: simulate delays, timeouts, and epoch shifts.
- **Reveals bugs** that are nearly impossible to catch with regular testing.

---

## 🧠 Concepts to Explore

- Simulated network partitions
- Delayed message delivery
- Time-travel debugging
- Epoch-based behavior

---

## 🐞 Known Bugs Found

- ✅ Epochalypse bug: failure around timestamp boundary logic.
- 🔍 Potential for discovering more with new interaction plans.

---

## 📂 Repo Structure

```bash
.
├── server/      # Core bank app (TCP server)
├── tcp_client/  # Client for interacting with the server over TCP
├── simulator/   # Simulator harness that runs `InteractionPlan`s against the Bank server
├── fs/          # File-system abstraction library that allows swapping implementation between simulated and concrete at compile-time
├── tcp/         # TCP abstraction library that allows swapping implementation between simulated and concrete at compile-time
├── random/      # Random abstraction library that allows swapping implementation between simulated (deterministically seeded) and fully random at compile-time
└── time/        # Time abstraction library that allows swapping implementation between simulated time and real time at compile-time
```

---

## 📚 References & Further Reading

Deterministic Simulation Testing (DST) is an evolving practice with growing adoption in distributed systems and observability tooling. Here are some great resources for deeper exploration:

- 📄 [(reading) **VOPR: Viewstamped Operation Replicator** (TigerBeetle)](https://web.archive.org/web/20250126140225/https://docs.tigerbeetle.com/about/vopr/)  
  Introduces the foundation behind TigerBeetle's deterministic testing framework, focusing on time-travel debugging and logical provenance of state changes.

- 📓 [(reading) **Notebook Interfaces for Distributed Systems** (Antithesis)](https://antithesis.com/blog/notebook_interfaces/)  
  Explores DST through the lens of test notebooks that record, replay, and introspect execution paths in a deterministic simulation.

- 🧪 [(reading) **Mostly DST in Go** (Polar Signals)](https://www.polarsignals.com/blog/posts/2024/05/28/mostly-dst-in-go)  
  A practical take on applying DST principles to Go-based systems, with insights on flake elimination and reliability gains.

- 🧰 [(code) **tokio-turmoil GitHub Repo** (Tokio)](https://github.com/tokio-rs/turmoil)  
  A Rust testing framework for deterministically simulating networks and failures in asynchronous systems.

- 🌀 [(code) **Limbo: Deterministic Simulation Testing for Turso** (Turso)](https://github.com/tursodatabase/limbo) ([simulator](https://github.com/tursodatabase/limbo/blob/main/simulator))  
  Turso's approach to deterministic simulation testing, focusing on reproducing and debugging distributed system issues in a controlled environment.
  The library powering this repository. Simulates network conditions, time, and failures in a deterministic way.

- 📚 [(reading) **sled simulation guide** (jepsen-proof engineering)](https://sled.rs/simulation.html)  
  A guide to simulation testing in the `sled` embedded database, emphasizing techniques for making systems resilient and “Jepsen-proof.”

- 🎥 [(video) **"Testing Distributed Systems w/ Deterministic Simulation"** (Will Wilson - Antithesis)](https://www.youtube.com/watch?v=4fFDFbi3toc)  
  A talk on how deterministic simulation testing improves reproducibility and confidence in distributed systems development.

- 🎥 [(video) **FF meetup #4 - Deterministic simulation testing** (Pekka Enberg - Turso)](https://www.youtube.com/live/29Vz5wkoUR8)  
  A meetup session featuring real-world applications and insights into deterministic simulation testing from industry practitioners.

- 🧑‍💻 [(code) **Hiisi** (Pekka Enberg - Turso)](https://github.com/penberg/hiisi)  
  A proof of concept libSQL written in Rust following TigerBeetle-style with deterministic simulation testing
