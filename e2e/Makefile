.PHONY: help test test-v5 router clean

REPO_ROOT := $(abspath ..)
ROUTER_MANIFEST := $(REPO_ROOT)/router/Cargo.toml
ROUTER_BIN := $(REPO_ROOT)/router/target/debug/multifrost-router
V5_MATRIX := $(REPO_ROOT)/e2e/v5/tests/test_matrix.py

help:
	@echo "Multifrost end-to-end harness"
	@echo ""
	@echo "Available targets:"
	@echo "  make test      - Build the router and run the full v5 four-caller by four-service matrix"
	@echo "  make test-v5   - Alias for the full v5 matrix run"
	@echo "  make router    - Build the router binary"
	@echo "  make clean     - Remove e2e-local build artifacts"

test: test-v5

test-v5: router
	@echo "Running the v5 end-to-end matrix..."
	python3 "$(V5_MATRIX)"

router:
	@echo "Building the router binary..."
	cargo build --manifest-path "$(ROUTER_MANIFEST)" --bin multifrost-router

clean:
	@echo "Cleaning end-to-end build artifacts..."
	rm -rf "$(REPO_ROOT)/router/target" 2>/dev/null || true
