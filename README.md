# 🏦 DST Demo: Bank App

This repository demonstrates **Deterministic Simulation Testing (DST)**. It's a fully self-contained project built around a simple bank application that communicates over TCP and supports basic transaction operations. Through DST it uncovers complex bugs like the infamous **"epochalypse"** bug.

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

## 📂 Repo Structure

```bash
.
├── server/      # Core bank app (TCP server)
├── tcp_client/  # Client for interacting with the server over TCP
├── simulator/   # Simulator harness that runs `InteractionPlan`s against the Bank server
├── tcp/         # TCP abstraction library that allows swapping implementation between simulated and concrete at compile-time
├── random/      # Random abstraction library that allows swapping implementation between simulated (deterministically seeded) and fully random at compile-time
└── time/        # Time abstraction library that allows swapping implementation between simulated time and real time at compile-time
```

---

## 📚 References & Further Reading

Deterministic Simulation Testing (DST) is an evolving practice with growing adoption in distributed systems and observability tooling. Here are some great resources for deeper exploration:

- 📄 [**VOPR: Viewstamped Operation Replicator** (TigerBeetle)](https://web.archive.org/web/20250126140225/https://docs.tigerbeetle.com/about/vopr/)  
  Introduces the foundation behind TigerBeetle's deterministic testing framework, focusing on time-travel debugging and logical provenance of state changes.

- 📓 [**Notebook Interfaces for Distributed Systems** (Antithesis)](https://antithesis.com/blog/notebook_interfaces/)  
  Explores DST through the lens of test notebooks that record, replay, and introspect execution paths in a deterministic simulation.

- 🧪 [**Mostly DST in Go** (Polar Signals)](https://www.polarsignals.com/blog/posts/2024/05/28/mostly-dst-in-go)  
  A practical take on applying DST principles to Go-based systems, with insights on flake elimination and reliability gains.

- 🧰 [**tokio-turmoil GitHub Repo**](https://github.com/tokio-rs/turmoil)  
  The library powering this repository. Simulates network conditions, time, and failures in a deterministic way.
