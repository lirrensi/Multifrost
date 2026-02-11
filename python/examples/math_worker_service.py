"""
Math worker that runs as a standalone microservice.

Run this first, then run parent_connects.py to connect to it.
"""

import sys
import os

# Add parent directory to path for imports
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "python", "src"))

from multifrost import ChildWorker


class MathWorker(ChildWorker):
    """A simple math worker that can be called remotely."""

    def __init__(self):
        # Register with service_id for connect mode
        super().__init__(service_id="math-service")

    def add(self, a: int, b: int) -> int:
        """Add two numbers."""
        print(f"Adding {a} + {b}")
        return a + b

    def multiply(self, a: int, b: int) -> int:
        """Multiply two numbers."""
        print(f"Multiplying {a} * {b}")
        return a * b

    def power(self, base: int, exp: int) -> int:
        """Raise base to the power of exp."""
        print(f"Computing {base}^{exp}")
        return base**exp


if __name__ == "__main__":
    print("Starting MathWorker as microservice...")
    worker = MathWorker()
    worker.run()
