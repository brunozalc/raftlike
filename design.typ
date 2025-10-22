#set page(
  paper: "us-letter",
  margin: (x: 1.5in, y: 1in),
)

#set text(
  font: "New Computer Modern",
  size: 11pt,
)

#set par(justify: true)

#set heading(numbering: "1.")



// Title
#align(center)[
  #text(size: 20pt, weight: "bold")[
    Raftlike Design Document
  ]

  #v(0.5em)

  #text(size: 12pt)[
    Small Implementation of the Raft Consensus System
  ]

  #v(1em)

  #text(size: 10pt)[
    *Author:* Bruno Zalcberg \
    *Date:* October 2025 \
    *Language:* Rust
  ]
]

#v(1em)
#line(length: 100%)
#v(1em)

= Architecture Overview

This project implements a simplified Raft consensus algorithm in Rust, featuring automatic leader election, log replication, and crash recovery across a 3-node cluster.

*Core Components:*
- HTTP API server (Axum framework)
- Asynchronous event loop (Tokio runtime)
- Persistent state storage (JSON files)
- Command-line interface

== Concurrent Task Architecture

The system spawns three async tasks at startup that run concurrently:

#figure(
  image("assets/logic-new.png", width: 70%),
  caption: [
    Concurrent task architecture: Election timeout, heartbeat, and HTTP handler tasks
    operate independently, coordinating through shared state
  ]
) <fig-architecture>

As shown in @fig-architecture, the three tasks are:

+ *Election Timeout Task* - Monitors for leader failures and initiates elections through RPC
+ *Heartbeat Task* - When leader, sends periodic append entries to followers through RPC
+ *HTTP Handler Task* - Accepts and processes client requests

All tasks share access to the `RaftNode` state through `Arc<Mutex<>>`, ensuring thread-safe coordination.

== Election Timeout Mechanism

Each follower maintains a randomized election timeout between 300-500ms. This randomization prevents split votes by ensuring nodes start elections at different times.

#box[
```rust
// Pseudo-code representation
timeout = random(300..500ms)
if no_heartbeat_received(timeout):
    become_candidate()
    increment_term()
    request_votes_from_peers()
```
]

== Voting Rules

A node grants its vote if *all* conditions are satisfied:

+ Candidate's term ≥ node's current term
+ Node hasn't voted for another candidate this term
+ Candidate's log is at least as up-to-date

*Log up-to-date comparison:*
- If last log terms differ → higher term wins
- If terms equal → longer log wins

== Term Management

Terms act as logical clocks. When a node observes a higher term:
- Immediately steps down to follower
- Updates to new term
- Clears its vote

This prevents stale leaders from disrupting the cluster.

= Log Replication

Log replication is one of Raft's core mechanisms and is of utmost importance.

== Append Entries Protocol

The leader replicates log entries by sending `AppendEntries` RPCs every 100ms (heartbeat interval).

*Request includes:*
- Leader's current term
- Previous log index and term (for consistency)
- New entries to append
- Leader's commit index

*Consistency check:* Follower rejects if `prev_log_index` doesn't match locally.

== Commit Protocol

The leader commits an entry when:
+ A majority of nodes have replicated it
+ The entry is from the current term

#box[
```rust
// Automatic commit detection
for each index in uncommitted_range:
    if majority_has(index) && entry.term == current_term:
        commit_index = index
        apply_to_state_machine()
```
]

Once committed, the entry is applied to the key-value store.

= Persistence Strategy

== Persistent State

Three pieces of state survive crashes:

#table(
  columns: (1fr, 2fr),
  [*Field*], [*Purpose*],
  [`current_term`], [Prevents voting in old elections],
  [`voted_for`], [Prevents double-voting],
  [`log`], [Source of truth for all data],
)

== Storage Format

State is serialized to JSON and written to `./states/raft_state_<id>.json` after every modification:
- Term changes (elections)
- Vote grants
- Log appends

== Recovery Process

On startup:
+ Load persistent state from disk
+ Initialize as follower in last known term
+ Wait for leader heartbeat or timeout

= Failure Handling

== Leader Failure

*Detection:* Followers detect via missed heartbeats (>400ms).

*Recovery:*
+ Election timeout expires
+ Follower becomes candidate
+ New leader elected within ~500ms

*Data safety:* Log entries committed on majority survive leader crashes.

== Split Votes

If two candidates start elections simultaneously:
- Both may fail to achieve majority
- Election timeout fires again with *different* random delays
- System eventually converges (typically within 2-3 attempts)

== Network Partitions

*Scenario:* Cluster splits into [Leader, A] and [B, C]

The minority partition (Leader, A) cannot commit new entries (no majority). The majority partition (B, C) elects a new leader and continues operating.

When partition heals, the old leader sees higher term and steps down.

= Future Improvements

+ Snapshot/compaction for log growth
+ Batched log replication for throughput
+ Metrics dashboard (Prometheus/Grafana)
