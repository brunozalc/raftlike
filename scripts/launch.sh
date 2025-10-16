#!/bin/bash

pkill -9 -f "target/debug/raftlike" || true
lsof -ti :8080 | xargs kill -9 2>/dev/null || true
lsof -ti :8081 | xargs kill -9 2>/dev/null || true
lsof -ti :8082 | xargs kill -9 2>/dev/null || true

rm -f ./states/raft_state_*.json ./logs/node_*.log

cargo build

echo "Launching nodes..."
./target/debug/raftlike --id A --port 8080 --peers localhost:8081,localhost:8082 > ./logs/node_a.log 2>&1 &

./target/debug/raftlike --id B --port 8081 --peers localhost:8080,localhost:8082 > ./logs/node_b.log 2>&1 &

./target/debug/raftlike --id C --port 8082 --peers localhost:8080,localhost:8081 > ./logs/node_c.log 2>&1 &
