<div align="center">
<img src="assets/logo.png" alt="raftlike logo" width="400"/>
</div>

# Raftlike

A simple implementation of the Raft consensus algorithm in Rust. This project provides a multi-node cluster and a command-line interface (CLI) to demonstrate leader election, log replication, and fault tolerance.

## Prerequisites

Before you begin, ensure you have the following installed:

- [Rust and Cargo](https://www.rust-lang.org/tools/install)
- `make`

## Getting Started

First, clone the repository and navigate into the project directory.

Then, build the project, including the server and the CLI client:

```sh
make build
```

This will compile the source code and place the binaries in the `target/debug/` directory.

## Usage Walkthrough

This project is best understood by starting a cluster and interacting with it through the provided CLI.

### 1. Launch the Cluster

To start a 3-node cluster, run:

```sh
make launch
```

This command stops any old instances, cleans up previous state, and starts three nodes on ports `8080` (Node A), `8081` (Node B), and `8082` (Node C).

### 2. Interact with the Cluster using the CLI

The primary way to interact with the cluster is through the `cli` binary.

**Check Cluster Status**

Run the `status` command to see the current state of each node. One node will be elected as the Leader.

```sh
./cli status
```

**Store a Key-Value Pair**

Use the `put` command to store data. The CLI will automatically find the leader and send the request to it.

```sh
./cli put message "hello world"
```

**Retrieve a Key**

Use the `get` command to read data. This request can be served by any node.

```sh
./cli get message
```

### 3. Simulate a Failure

Now, let's simulate a failure to observe Raft's fault tolerance in action.

**Identify and Kill the Leader**

From the output of `./cli status`, identify which node is the current leader (it will have a `★` next to it). Let's assume Node A is the leader.

Use the `kill` command to stop that node's process:

```sh
./cli kill A
```

**Observe the New Election**

Check the status again. You'll see that Node A is unresponsive, and a new leader has been elected from the remaining nodes.

```sh
./cli status
```

**Verify Data Integrity and Availability**

Even with one node down, the cluster remains operational. You can still read the original data:

```sh
./cli get message
```

And you can still write new data:

```sh
./cli put new_key "it still works"
./cli get new_key
```

**Restart the Failed Node**

Bring the failed node back online with the `restart` command. It will rejoin the cluster as a Follower.

```sh
./cli restart A
```

A final status check will show that the cluster is fully healthy again.

```sh
./cli status
```

## CLI Commands

Here is a summary of all available `cli` commands:

| Command             | Description                                                 |
| ------------------- | ----------------------------------------------------------- | --- | --------------------------------- |
| `status`            | Shows the detailed status of each node in the cluster.      |
| `put <KEY> <VALUE>` | Stores a key-value pair in the distributed key-value store. |
| `get <KEY>`         | Retrieves the value for a given key.                        |
| `kill <A            | B                                                           | C>` | Kills the specified node process. |
| `restart <A         | B                                                           | C>` | Restarts the specified node.      |

## Cluster Management (`make`)

While the `cli` is used for interacting with the cluster's state, `make` is used for managing the lifecycle of the cluster itself.

| Command          | Description                                  |
| ---------------- | -------------------------------------------- |
| `make build`     | Build the project binaries.                  |
| `make launch`    | Start the 3-node cluster.                    |
| `make stop`      | Stop all running nodes.                      |
| `make logs`      | Tail the logs for all nodes.                 |
| `make clean`     | Remove logs and state files.                 |
| `make clean-all` | Remove logs, state, and all build artifacts. |
| `make help`      | Show a help message with all commands.       |
