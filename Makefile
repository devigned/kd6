.PHONY: fmt lint build test ci clean run-server run-mcp

# Format code
fmt:
	cargo fmt

# Check formatting (same as CI)
fmt-check:
	cargo fmt -- --check

# Lint with clippy (same as CI: warnings are errors)
lint:
	cargo clippy --all-targets -- -D warnings

# Build all targets
build:
	cargo build --all-targets

# Run tests
test:
	cargo test

# Run the full CI pipeline locally — use before every push
ci: fmt-check lint build test

# Auto-format then run CI checks
fix: fmt lint build test

# Remove build artifacts
clean:
	cargo clean

# Run the HTTP API server (port 8080)
run-server:
	KD6_DATABASE_URL="sqlite:kd6.db?mode=rwc" cargo run -p kd6-server

# Run the MCP server (Streamable HTTP, port 8081)
run-mcp:
	KD6_DATABASE_URL="sqlite:kd6.db?mode=rwc" cargo run -p kd6-mcp
