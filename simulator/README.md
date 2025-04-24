# 🧪 Simulator Overview

This simulator orchestrates deterministic simulation testing (DST) using the `tokio-turmoil` framework to uncover concurrency and failure edge cases in the TCP-based bank server application.

## 🧩 Architecture

The simulation involves two main components:

### 🖥️ Host Server (`host`)

Simulates the real TCP bank server within the turmoil simulation. It processes client requests to create, void, get, and list transactions, using simulated time and deterministic execution to model realistic server behavior under network conditions and failures.

### 🧑‍🤝‍🧑 Clients (`client`)

Simulated bank clients that execute a series of planned interactions (via `InteractionPlan`) with the host server. These clients mimic real-world usage by sending timed and possibly conflicting requests, helping to uncover bugs like race conditions or consistency errors. Each client runs in a fully simulated environment with deterministic timing and networking, allowing for reproducible stress testing and debugging.

There are 3 clients that interact with the host:

#### 💼 Banker

Acts as a realistic user of the bank system. Executes a sequence of operations (e.g. create, void, get, list transactions) based on an `InteractionPlan`, simulating regular user traffic and transaction workflows.

#### 💥 Fault Injector

Deliberately introduces simulated network partitions, crashes, and restarts to test the system's resilience and recovery. Useful for verifying that transaction state remains consistent despite faults.

#### 🩺 Health Checker

Periodically pings the server to verify its responsiveness and uptime. Ensures that faults or bugs don't silently break the system's liveness guarantees.
