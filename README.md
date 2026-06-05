# 5v5 Competitive Matchmaking Engine in Rust

A high-performance, concurrent, in-memory 5v5 game matchmaking engine built in Rust. It utilizes a **Quantized Bucket** system, a **Tick-Based Sliding Window** for constraint relaxation, the **Actor Model** for lock-free queue concurrency, and an **Exhaustive Brute-Force Team Balancer** with internal variance tuning.

---

## 1. Clean Architecture Structure

The project is structured according to the principles of Clean Architecture:

* **`src/domain/`** (Enterprise Rules): Holds core business models (`Player`, `Ticket`) and math logic (`team_balancer.rs`) independent of thread runtimes or network interfaces.
* **`src/application/`** (Application Logic): Orchestrates data flow. Coordinates in-memory state (`queue_manager.rs`), the main matchmaking runner (`engine.rs`), and the atomic metrics collectors (`telemetry.rs`).
* **`src/infrastructure/`** (Frameworks & Drivers): Handles external interfaces. Exposes thread runners for exporting metrics (`telemetry_exporter.rs`) and the load simulation runner (`simulator.rs`).
* **`src/main.rs`** (Bootstrap): Entry point parsing arguments and launching the simulation.

---

## 2. Tackling the 5 Engineering Challenges

### 2.1 The Core Algorithm (Latency vs. Match Quality)
To prevent expensive $O(N^2)$ pairwise checks over a raw list of queued players, we quantize players into discrete buckets using their MMR:

$$\text{BucketID} = \text{floor}\left(\frac{\text{PlayerMMR}}{\text{BucketSize}}\right)$$

We store the queues in a Rust `BTreeMap<i32, VecDeque<Ticket>>`. This provides:
* **$O(1)$ Insertion** into the bucket's FIFO double-ended queue.
* **$O(\log B + K)$ Range Scans** to find neighboring buckets, where $B$ is the number of active buckets and $K$ is the search radius.

### 2.2 Thread-Safe State & Lock-Free Eviction
Simultaneously scanning and modifying the waiting pool across multiple worker threads usually leads to massive lock contention (`Mutex`/`RwLock` bottlenecks).
* **Our Solution:** We implement the **Actor Model**. Ingress threads (simulating web/gRPC requests) do not touch the waiting pool memory. They drop player tickets into a Multi-Producer, Single-Consumer (MPSC) channel and return immediately.
* A **single dedicated matchmaking thread (the Actor)** owns the B-Tree Map. It drains the channel, batches insertions, and performs eviction ticks sequentially. This eliminates memory lock contention, ensuring maximum throughput.

### 2.3 Time-Based Constraint Relaxation
If high-skill or low-skill players are isolated in remote MMR buckets, they could wait indefinitely.
* **Step-Function Expansion:** When a player enters the queue, their search radius is `0`. Every $X$ seconds (e.g., 15s), their search radius increments by `1` bucket in both directions.
* **Mutual Consent Rule:** To ensure fairness, Player A can only match with Player B if Player A's expanded window contains Player B's base bucket, **and** Player B's expanded window contains Player A's base bucket. 
$$\text{Consent}(A, B) \iff |BucketID_A - BucketID_B| \le \min(\text{Radius}_A, \text{Radius}_B)$$

### 2.4 Team Balance Optimization
Once 10 compatible players are grouped, we must divide them fairly.
* **Exhaustive Search:** Splitting 10 players into two teams of 5 yields exactly $\frac{10!}{5! \times 5! \times 2} = 126$ unique combinations. The engine generates all 126 splits in microsecond time.
* **Cost Function:** We evaluate each combination using:
$$\text{Cost} = |AvgMMR_A - AvgMMR_B| + W \times (\text{Variance}_A + \text{Variance}_B)$$
* The tuning weight $W$ (default `0.5`) balances the trade-off between matching average skills and preventing high skill variance (e.g. grouping a top-tier player with a novice to mathematically balance the average).

### 2.5 Low-Latency Health Metrics
Recording metrics or writing logs directly inside the matchmaking loop slows down processing.
* **Atomic Counters:** We track metrics using `AtomicU64` with relaxed memory ordering (`Ordering::Relaxed`).
* **Asynchronous Telemetry Thread:** An independent background thread wakes up periodically (e.g., every 2s), reads and resets the counters using `swap(0, Ordering::Relaxed)`, and logs statistics. This completely isolates heavy I/O operations from the main matchmaking thread.

---

## 3. Algorithmic Complexity

| Component | Time Complexity | Space Complexity | Notes |
| :--- | :--- | :--- | :--- |
| **Ticket Insertion** | $O(1)$ | $O(1)$ | Inserting into `VecDeque` inside `BTreeMap` |
| **Sliding Window Tick** | $O(\log B + K \cdot M \log M)$ | $O(N)$ | $B$: Active buckets, $K$: Search radius, $M$: Candidates in window |
| **Team Balancing** | $O(126 \times 10) \approx O(1)$ | $O(1)$ | Exact brute-force over 126 combinations |
| **Telemetry Snapshots** | $O(1)$ | $O(1)$ | Atomic swap operations |

---

## 4. Scaling Challenges & Future Architecture

While the in-memory Actor Model ensures maximum single-node throughput, moving this architecture to a globally distributed environment introduces three major scaling challenges:

1. **State Partitioning (Horizontal Sharding):** Currently, the entire `BTreeMap` queue is held in a single thread's memory. To horizontally scale, the matchmaking pool would need to be sharded by MMR brackets across multiple Redis instances. However, strict sharding creates boundaries where players near the partition line cannot match with adjacent players on a different physical server without complex distributed cross-node transactions.
2. **Channel Backpressure:** The `mpsc` channel effectively decouples ingress from execution, but an unbounded channel during massive traffic spikes (e.g., season launches) could lead to OOM (Out Of Memory) crashes. Bounding the channel protects the memory but shifts the queuing pressure back onto the HTTP/gRPC ingress layer, potentially timing out client connections.
3. **Resilience & State Recovery:** An in-memory queue is inherently ephemeral. If the primary matchmaking node crashes, the entire active waiting pool is lost. A production system must replicate the queue state to a persistent data store or employ an event-sourcing mechanism (e.g., Kafka) to seamlessly replay the event log and rebuild the `BTreeMap` upon reboot.

---

## 5. Real-Time TUI Dashboard & Telemetry

To visualize the engine under heavy concurrent load, the project includes a real-time Terminal User Interface (TUI) built with `ratatui`.

When you launch the application, you enter an interactive Setup Screen where you can dynamically specify load parameters. During the simulation, the dashboard heavily monitors the background matchmaking actor:
* **Wait Time Percentiles (`p50`, `p90`, `p99`, `Max`):** Displayed dynamically on logarithmic gauges. **Significance:** In matchmaking, averages hide outliers. Tracking the `p99` and `Max` wait times ensures that edge-case players (e.g., top 1% skill brackets) aren't being permanently starved or ignored by the matching loop.
* **Throughput & Queue Delta:** Tracks "Matches Per Second" and the live Queue Delta (+/-). **Significance:** If the Queue Delta turns positive and climbs indefinitely, it proves the single-threaded algorithmic B-Tree scan is bottlenecking and failing to outpace the HTTP ingress pressure, signaling a need for horizontal scaling.
* **Queue Health (Fragmentation & Wait Debt):** Tracks active memory buckets and cumulative time wasted. **Significance:** A fragmented queue (few players spread across hundreds of buckets) forces the algorithm to execute expensive Step-Function expansions. It directly visualizes memory density.
* **Match Tension Extremes:** Tracks the Min, Max, and Average for `Delta` (MMR Gap), `Variance` (internal skill spread), and `Search Radius`. **Significance:** This unpacks the internal Cost Function. While the *Average* Delta proves the team-balancer is generally working, tracking the *Max* explicitly exposes the absolute worst-case scenario match the algorithm was forced to generate under load.
* **Queue Demographics & Staleness:** Tracks Solos, Duos, Trios, and the absolute oldest ticket age. **Significance:** Uncovers systemic bias. If the algorithm inherently favors packing Solos to easily optimize MMR averages, the Trio count will perpetually rise and their ticket age will stall out, identifying an unfair balancing heuristic.
* **CPU Tick Profiling:** Benchmarks the B-Tree ordered-iterator execution in microseconds. **Significance:** Directly evaluates the raw CPU efficiency and algorithmic complexity limits of the main actor thread.

---

## 6. Run Guide

### Running Unit Tests
To verify all core logic:
```bash
cargo test
```

### Launching the Dashboard
Start the interactive TUI simulator:
```bash
cargo run
```
* Use `Tab` / `Shift+Tab` or Arrow Keys to navigate the Setup Screen.
* Input your desired `Players` (e.g. 10000) and `Threads` (e.g. 10).
* Press `Enter` to launch the engine and begin the live simulation.
* Press `q` to safely exit the dashboard at any time.
