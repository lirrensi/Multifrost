.PHONY: help install install-python install-javascript test test-python test-javascript test-e2e test-e2e-python test-e2e-javascript clean

help:
	@echo "Multifrost - IPC Library for Python and Node.js"
	@echo ""
	@echo "Available targets:"
	@echo "  make install          - Install both Python and JavaScript packages"
	@echo "  make install-python   - Install Python package"
	@echo "  make install-javascript - Install JavaScript package"
	@echo "  make test             - Run all tests"
	@echo "  make test-python      - Run Python unit tests"
	@echo "  make test-javascript  - Run JavaScript unit tests"
	@echo "  make test-e2e         - Run all E2E tests"
	@echo "  make test-e2e-python  - Run Python E2E tests (Python parent)"
	@echo "  make test-e2e-javascript - Run JavaScript E2E tests (JS parent)"
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
	@echo "Running Python unit tests..."
	cd python && pytest tests/

test-javascript:
	@echo "Running JavaScript unit tests..."
	cd javascript && npm test

# E2E test setup - create isolated venv with multifrost + test deps
E2E_VENV = .venv-e2e
E2E_PYTHON = $(E2E_VENV)/Scripts/python.exe
E2E_PIP = $(E2E_VENV)/Scripts/pip.exe

$(E2E_VENV):
	@echo "Creating E2E virtual environment with uv..."
	uv venv $(E2E_VENV)
	uv pip install -p $(E2E_VENV) pytest pytest-asyncio msgpack psutil zmq
	uv pip install -p $(E2E_VENV) -e python/

test-e2e-python: $(E2E_VENV)
	@echo "Running Python E2E tests..."
	$(E2E_PYTHON) -m pytest e2e/test_e2e.py -v -s

test-e2e-javascript:
	@echo "Running JavaScript E2E tests..."
	cd javascript && npx tsx tests/e2e_minimal.test.ts

test-e2e: test-e2e-python test-e2e-javascript

clean:
	@echo "Cleaning build artifacts..."
	find . -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true
	find . -type f -name "*.pyc" -delete 2>/dev/null || true
	find . -type d -name "node_modules" -exec rm -rf {} + 2>/dev/null || true
	find . -type f -name "*.egg-info" -exec rm -rf {} + 2>/dev/null || true
	rm -rf e2e/.venv 2>/dev/null || true
