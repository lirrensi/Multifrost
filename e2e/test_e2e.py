#!/usr/bin/env python3
"""
E2E Tests for Multifrost - Python Parent Tests
These tests verify cross-language interoperability:
- Python parent spawning JavaScript child
- Python parent spawning Python child
"""

import asyncio
import os
import sys

import pytest

from multifrost import ParentWorker, RemoteCallError

# Paths to worker scripts
E2E_DIR = os.path.dirname(os.path.abspath(__file__))
PYTHON_WORKER = os.path.join(E2E_DIR, "workers", "math_worker.py")
JS_WORKER = os.path.join(E2E_DIR, "workers", "math_worker.ts")
GO_WORKER = os.path.join(E2E_DIR, "workers", "math_worker_go.exe")
RUST_WORKER = os.path.join(E2E_DIR, "workers", "math_worker_rust.exe")

# Use the Python executable from this venv (not from PATH!)
PYTHON_EXECUTABLE = os.path.join(
    "C:/Users/rx/001_Code/100_M/Multifrost", ".venv-e2e", "Scripts", "python.exe"
)

# Full path to tsx (npx won't work in subprocess) - use .CMD on Windows
TSX_EXECUTABLE = (
    "C:/Users/rx/001_Code/100_M/Multifrost/javascript/node_modules/.bin/tsx.CMD"
)


# =============================================================================
# PYTHON PARENT -> PYTHON CHILD TESTS (Baseline)
# =============================================================================


class TestPythonParentPythonChild:
    """Test Python parent spawning Python child (baseline)."""

    @pytest.mark.asyncio
    async def test_spawn_python_child_basic_call(self):
        """Test basic method call from Python to Python child."""
        async with ParentWorker.spawn(PYTHON_WORKER, PYTHON_EXECUTABLE) as worker:
            await asyncio.sleep(0.5)  # Give child time to start

            result = await worker.acall.add(10, 20)
            assert result == 30, f"Expected 30, got {result}"

            result = await worker.acall.multiply(5, 6)
            assert result == 30, f"Expected 30, got {result}"

    @pytest.mark.asyncio
    async def test_spawn_python_child_various_types(self):
        """Test passing various data types."""
        async with ParentWorker.spawn(PYTHON_WORKER, PYTHON_EXECUTABLE) as worker:
            await asyncio.sleep(0.5)

            # String
            result = await worker.acall.echo("hello")
            assert result == "hello"

            # Number
            result = await worker.acall.echo(42)
            assert result == 42

            # Float
            result = await worker.acall.echo(3.14)
            assert result == 3.14

            # Boolean
            result = await worker.acall.echo(True)
            assert result is True

            # Null/None
            result = await worker.acall.echo(None)
            assert result is None

            # List
            result = await worker.acall.echo([1, 2, 3])
            assert result == [1, 2, 3]

            # Dict
            result = await worker.acall.echo({"a": 1, "b": "test"})
            assert result == {"a": 1, "b": "test"}

    @pytest.mark.asyncio
    async def test_spawn_python_child_error_handling(self):
        """Test error handling from child."""
        async with ParentWorker.spawn(PYTHON_WORKER, PYTHON_EXECUTABLE) as worker:
            await asyncio.sleep(0.5)

            # Test error propagation
            try:
                await worker.acall.throw_error("test error message")
                pytest.fail("Should have raised RemoteCallError")
            except RemoteCallError as e:
                assert "test error message" in str(e)

            # Test divide by zero
            try:
                await worker.acall.divide(10, 0)
                pytest.fail("Should have raised error")
            except RemoteCallError:
                pass

    @pytest.mark.asyncio
    async def test_spawn_python_child_async_method(self):
        """Test async method calls."""
        async with ParentWorker.spawn(PYTHON_WORKER, PYTHON_EXECUTABLE) as worker:
            await asyncio.sleep(0.5)

            result = await worker.acall.async_add(10, 20)
            assert result == 30

    @pytest.mark.asyncio
    async def test_spawn_python_child_info(self):
        """Test getting worker info."""
        async with ParentWorker.spawn(PYTHON_WORKER, PYTHON_EXECUTABLE) as worker:
            await asyncio.sleep(0.5)

            info = await worker.acall.get_info()
            assert info["language"] == "python"
            assert "pid" in info

    @pytest.mark.asyncio
    async def test_spawn_python_child_fibonacci(self):
        """Test Fibonacci computation in Python child (real app use case)."""
        async with ParentWorker.spawn(PYTHON_WORKER, PYTHON_EXECUTABLE) as worker:
            await asyncio.sleep(0.5)

            # Test various fibonacci values
            assert await worker.acall.fibonacci(0) == 0
            assert await worker.acall.fibonacci(1) == 1
            assert await worker.acall.fibonacci(10) == 55
            assert await worker.acall.fibonacci(20) == 6765
            assert await worker.acall.fibonacci(50) == 12586269025

    @pytest.mark.asyncio
    async def test_spawn_python_child_fibonacci_async(self):
        """Test async Fibonacci in Python child (real app use case)."""
        async with ParentWorker.spawn(PYTHON_WORKER, PYTHON_EXECUTABLE) as worker:
            await asyncio.sleep(0.5)

            result = await worker.acall.fibonacci_async(20)
            assert result == 6765, f"Expected 6765, got {result}"


# =============================================================================
# PYTHON PARENT -> JS CHILD TESTS (Cross-Language)
# =============================================================================


class TestPythonParentJSChild:
    """Test Python parent spawning JavaScript child (cross-language)."""

    @pytest.mark.asyncio
    async def test_spawn_js_child_basic_call(self):
        """Test basic method call from Python to JS child."""
        async with ParentWorker.spawn(JS_WORKER, TSX_EXECUTABLE) as worker:
            await asyncio.sleep(1.0)  # Give child more time to start

            result = await worker.acall.add(10, 20)
            assert result == 30, f"Expected 30, got {result}"

            result = await worker.acall.multiply(5, 6)
            assert result == 30, f"Expected 30, got {result}"

    @pytest.mark.asyncio
    async def test_spawn_js_child_various_types(self):
        """Test passing various data types to JS child."""
        async with ParentWorker.spawn(JS_WORKER, TSX_EXECUTABLE) as worker:
            await asyncio.sleep(1.0)

            # String
            result = await worker.acall.echo("hello")
            assert result == "hello"

            # Number
            result = await worker.acall.echo(42)
            assert result == 42

            # Float
            result = await worker.acall.echo(3.14)
            assert result == 3.14

            # Boolean
            result = await worker.acall.echo(True)
            assert result is True

            # List
            result = await worker.acall.echo([1, 2, 3])
            assert result == [1, 2, 3]

            # Dict
            result = await worker.acall.echo({"a": 1, "b": "test"})
            assert result == {"a": 1, "b": "test"}

    @pytest.mark.asyncio
    async def test_spawn_js_child_error_handling(self):
        """Test error handling from JS child."""
        async with ParentWorker.spawn(JS_WORKER, TSX_EXECUTABLE) as worker:
            await asyncio.sleep(1.0)

            try:
                await worker.acall.throwError("JS error message")
                pytest.fail("Should have raised RemoteCallError")
            except RemoteCallError as e:
                assert "JS error message" in str(e)

    @pytest.mark.asyncio
    async def test_spawn_js_child_info(self):
        """Test getting worker info from JS child."""
        async with ParentWorker.spawn(JS_WORKER, TSX_EXECUTABLE) as worker:
            await asyncio.sleep(1.0)

            info = await worker.acall.getInfo()
            assert info["language"] == "javascript"
            assert "pid" in info

    @pytest.mark.asyncio
    async def test_spawn_js_child_factorial(self):
        """Test factorial computation in JS child."""
        async with ParentWorker.spawn(JS_WORKER, TSX_EXECUTABLE) as worker:
            await asyncio.sleep(1.0)

            result = await worker.acall.factorial(10)
            assert result == 3628800, f"Expected 3628800, got {result}"

    @pytest.mark.asyncio
    async def test_spawn_js_child_fibonacci(self):
        """Test Fibonacci computation in JS child (real app use case)."""
        async with ParentWorker.spawn(JS_WORKER, TSX_EXECUTABLE) as worker:
            await asyncio.sleep(1.0)

            # Test various fibonacci values
            assert await worker.acall.fibonacci(0) == 0
            assert await worker.acall.fibonacci(1) == 1
            assert await worker.acall.fibonacci(10) == 55
            assert await worker.acall.fibonacci(20) == 6765
            assert await worker.acall.fibonacci(50) == 12586269025

    @pytest.mark.asyncio
    async def test_spawn_js_child_fibonacci_async(self):
        """Test async Fibonacci in JS child (real app use case)."""
        async with ParentWorker.spawn(JS_WORKER, TSX_EXECUTABLE) as worker:
            await asyncio.sleep(1.0)

            result = await worker.acall.fibonacciAsync(20)
            assert result == 6765, f"Expected 6765, got {result}"


# =============================================================================
# PYTHON PARENT -> GO CHILD TESTS (Cross-Language)
# =============================================================================


class TestPythonParentGoChild:
    """Test Python parent spawning Go child (compiled binary)."""

    @pytest.mark.asyncio
    async def test_spawn_go_child_basic_call(self):
        """Test basic method call from Python to Go child."""
        async with ParentWorker.spawn(GO_WORKER) as worker:
            await asyncio.sleep(0.5)

            result = await worker.acall.Add(10, 20)
            assert result == 30, f"Expected 30, got {result}"

            result = await worker.acall.Multiply(5, 6)
            assert result == 30, f"Expected 30, got {result}"

    @pytest.mark.asyncio
    async def test_spawn_go_child_fibonacci(self):
        """Test Fibonacci computation in Go child."""
        async with ParentWorker.spawn(GO_WORKER) as worker:
            await asyncio.sleep(0.5)

            result = await worker.acall.Fibonacci(10)
            assert result == 55, f"Expected 55, got {result}"

            result = await worker.acall.Fibonacci(20)
            assert result == 6765, f"Expected 6765, got {result}"

    @pytest.mark.asyncio
    async def test_spawn_go_child_factorial(self):
        """Test factorial computation in Go child."""
        async with ParentWorker.spawn(GO_WORKER) as worker:
            await asyncio.sleep(0.5)

            result = await worker.acall.Factorial(10)
            assert result == 3628800, f"Expected 3628800, got {result}"

    @pytest.mark.asyncio
    async def test_spawn_go_child_info(self):
        """Test getting worker info from Go child."""
        async with ParentWorker.spawn(GO_WORKER) as worker:
            await asyncio.sleep(0.5)

            info = await worker.acall.GetInfo()
            assert info["language"] == "go"
            assert "pid" in info

    @pytest.mark.asyncio
    async def test_spawn_go_child_echo(self):
        """Test echo method in Go child."""
        async with ParentWorker.spawn(GO_WORKER) as worker:
            await asyncio.sleep(0.5)

            result = await worker.acall.Echo("hello")
            assert result == "hello"

            result = await worker.acall.Echo(42)
            assert result == 42

    @pytest.mark.asyncio
    async def test_spawn_go_child_error_handling(self):
        """Test error handling from Go child."""
        async with ParentWorker.spawn(GO_WORKER) as worker:
            await asyncio.sleep(0.5)

            try:
                await worker.acall.ThrowError("Go error message")
                pytest.fail("Should have raised RemoteCallError")
            except RemoteCallError as e:
                assert "Go error message" in str(e)


# =============================================================================
# PYTHON PARENT -> RUST CHILD TESTS (Cross-Language)
# =============================================================================


class TestPythonParentRustChild:
    """Test Python parent spawning Rust child (compiled binary)."""

    @pytest.mark.asyncio
    async def test_spawn_rust_child_basic_call(self):
        """Test basic method call from Python to Rust child."""
        async with ParentWorker.spawn(RUST_WORKER) as worker:
            await asyncio.sleep(0.5)

            result = await worker.acall.add(10, 20)
            assert result == 30, f"Expected 30, got {result}"

            result = await worker.acall.multiply(5, 6)
            assert result == 30, f"Expected 30, got {result}"

    @pytest.mark.asyncio
    async def test_spawn_rust_child_fibonacci(self):
        """Test Fibonacci computation in Rust child."""
        async with ParentWorker.spawn(RUST_WORKER) as worker:
            await asyncio.sleep(0.5)

            result = await worker.acall.fibonacci(10)
            assert result == 55, f"Expected 55, got {result}"

            result = await worker.acall.fibonacci(20)
            assert result == 6765, f"Expected 6765, got {result}"

    @pytest.mark.asyncio
    async def test_spawn_rust_child_factorial(self):
        """Test factorial computation in Rust child."""
        async with ParentWorker.spawn(RUST_WORKER) as worker:
            await asyncio.sleep(0.5)

            result = await worker.acall.factorial(10)
            assert result == 3628800, f"Expected 3628800, got {result}"

    @pytest.mark.asyncio
    async def test_spawn_rust_child_info(self):
        """Test getting worker info from Rust child."""
        async with ParentWorker.spawn(RUST_WORKER) as worker:
            await asyncio.sleep(0.5)

            info = await worker.acall.get_info()
            assert info["language"] == "rust"
            assert "pid" in info

    @pytest.mark.asyncio
    async def test_spawn_rust_child_echo(self):
        """Test echo method in Rust child."""
        async with ParentWorker.spawn(RUST_WORKER) as worker:
            await asyncio.sleep(0.5)

            result = await worker.acall.echo("hello")
            assert result == "hello"

            result = await worker.acall.echo(42)
            assert result == 42

    @pytest.mark.asyncio
    async def test_spawn_rust_child_error_handling(self):
        """Test error handling from Rust child."""
        async with ParentWorker.spawn(RUST_WORKER) as worker:
            await asyncio.sleep(0.5)

            try:
                await worker.acall.throw_error("Rust error message")
                pytest.fail("Should have raised RemoteCallError")
            except RemoteCallError as e:
                assert "Rust error message" in str(e)


# =============================================================================
# TIMEOUT AND CIRCUIT BREAKER TESTS
# =============================================================================


class TestTimeoutAndCircuitBreaker:
    """Test timeout and circuit breaker functionality."""

    @pytest.mark.asyncio
    async def test_circuit_breaker_state(self):
        """Test circuit breaker state tracking."""
        async with ParentWorker.spawn(PYTHON_WORKER, PYTHON_EXECUTABLE) as worker:
            await asyncio.sleep(0.5)

            # After start, should be healthy
            assert worker.is_healthy
            assert not worker.circuit_open


# =============================================================================
# RUN TESTS
# =============================================================================


if __name__ == "__main__":
    print("Running Multifrost E2E Tests...")
    print("=" * 60)

    # Run with pytest
    pytest.main([__file__, "-v", "-s"])
