.PHONY: help build launch stop clean clean-all status logs test

# default target - show help
help:
	@echo "raftlike - commands:"
	@echo ""
	@echo "  make build       - build the project"
	@echo "  make launch      - start 3-node cluster (ports 8080, 8081, 8082)"
	@echo "  make stop        - stop all running nodes"
	@echo "  make clean       - remove logs and state files"
	@echo "  make clean-all   - remove logs, state, and build artifacts"
	@echo "  make status      - check status of all nodes"
	@echo "  make logs        - tail all node logs (ctrl+c to exit)"
	@echo "  make test        - run basic functionality tests"
	@echo ""

# build the project
build:
	@echo "Building Raft node..."
	@cargo build
	@echo "Build complete!"

# launch the cluster using launch.sh
launch: stop clean
	@echo "Launching 3-node cluster..."
	@./scripts/launch.sh
	@sleep 2
	@echo ""
	@echo "Cluster started! Check status with: make status"

# stop all nodes
stop:
	@echo "Stopping all Raft nodes..."
	@pkill -9 -f "target/debug/raftlike" 2>/dev/null || true
	@lsof -ti :8080 | xargs kill -9 2>/dev/null || true
	@lsof -ti :8081 | xargs kill -9 2>/dev/null || true
	@lsof -ti :8082 | xargs kill -9 2>/dev/null || true
	@echo "All nodes stopped."

# clean logs and state files
clean:
	@echo "Cleaning logs and state files..."
	@rm -f ./logs/node_*.log
	@rm -rf ./states/raft_state_*.json
	@echo "Clean complete."

# clean everything including build artifacts
clean-all: clean
	@echo "Cleaning build artifacts..."
	@cargo clean
	@echo "Full clean complete."

# check status of all nodes
status:
	@echo "Node A (8080):"
	@curl -s http://localhost:8080/metrics 2>/dev/null | jq '.' || echo "  Not responding"
	@echo ""
	@echo "Node B (8081):"
	@curl -s http://localhost:8081/metrics 2>/dev/null | jq '.' || echo "  Not responding"
	@echo ""
	@echo "Node C (8082):"
	@curl -s http://localhost:8082/metrics 2>/dev/null | jq '.' || echo "  Not responding"

# tail all logs (requires multiple terminals or tmux)
logs:
	@echo "Tailing all node logs (Ctrl+C to exit)..."
	@tail -f node_a.log node_b.log node_c.log 2>/dev/null || echo "No log files found. Run 'make launch' first."

# basic functionality test
test:
	@echo "Running basic tests..."
	@echo ""
	@echo "1. Checking if cluster is running..."
	@curl -s http://localhost:8080/metrics > /dev/null && echo "✓ Node A responding" || echo "✗ Node A not responding"
	@curl -s http://localhost:8081/metrics > /dev/null && echo "✓ Node B responding" || echo "✗ Node B not responding"
	@curl -s http://localhost:8082/metrics > /dev/null && echo "✓ Node C responding" || echo "✗ Node C not responding"
	@echo ""
	@echo "2. Finding leader..."
	@LEADER=$$(curl -s http://localhost:8080/metrics | jq -r 'select(.state=="Leader") | "8080"') && \
	if [ -z "$$LEADER" ]; then \
		LEADER=$$(curl -s http://localhost:8081/metrics | jq -r 'select(.state=="Leader") | "8081"'); \
	fi && \
	if [ -z "$$LEADER" ]; then \
		LEADER=$$(curl -s http://localhost:8082/metrics | jq -r 'select(.state=="Leader") | "8082"'); \
	fi && \
	if [ -n "$$LEADER" ]; then \
		echo "✓ Leader found on port $$LEADER"; \
	else \
		echo "✗ No leader elected yet"; \
	fi
	@echo ""
	@echo "Run 'make status' for detailed cluster status"
