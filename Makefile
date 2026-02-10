.PHONY: help install install-python install-javascript test test-python test-javascript clean

help:
	@echo "Multifrost - IPC Library for Python and Node.js"
	@echo ""
	@echo "Available targets:"
	@echo "  make install          - Install both Python and JavaScript packages"
	@echo "  make install-python   - Install Python package"
	@echo "  make install-javascript - Install JavaScript package"
	@echo "  make test             - Run all tests"
	@echo "  make test-python      - Run Python tests"
	@echo "  make test-javascript  - Run JavaScript tests"
	@echo "  make clean            - Clean build artifacts"

install: install-python install-javascript

install-python:
	@echo "Installing Python package..."
	cd python && pip install -e .

install-javascript:
	@echo "Installing JavaScript package..."
	cd javascript && npm install

test: test-python test-javascript

test-python:
	@echo "Running Python tests..."
	cd python && pytest tests/

test-javascript:
	@echo "Running JavaScript tests..."
	cd javascript && npm test

clean:
	@echo "Cleaning build artifacts..."
	find . -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true
	find . -type f -name "*.pyc" -delete 2>/dev/null || true
	find . -type d -name "node_modules" -exec rm -rf {} + 2>/dev/null || true
	find . -type f -name "*.egg-info" -exec rm -rf {} + 2>/dev/null || true
