/**
 * E2E Test Worker - JavaScript/TypeScript implementation
 * This worker provides various methods for cross-language testing.
 */

import { ChildWorker } from "../../javascript/src/multifrost.ts";

class MathWorker extends ChildWorker {
    add(a: number, b: number): number {
        return a + b;
    }

    multiply(a: number, b: number): number {
        return a * b;
    }

    divide(a: number, b: number): number {
        if (b === 0) {
            throw new Error("Cannot divide by zero");
        }
        return a / b;
    }

    factorial(n: number): number {
        if (n < 0) {
            throw new Error("Factorial not defined for negative numbers");
        }
        if (n > 100) {
            throw new Error("Input too large");
        }
        
        let result = 1;
        for (let i = 2; i <= n; i++) {
            result *= i;
        }
        return result;
    }

    echo(value: unknown): unknown {
        return value;
    }

    getInfo(): { language: string; pid: number; version: string } {
        return {
            language: "javascript",
            pid: process.pid,
            version: process.version,
        };
    }

    throwError(message: string): never {
        throw new Error(message);
    }

    async asyncAdd(a: number, b: number): Promise<number> {
        await new Promise(r => setTimeout(r, 10));  // Simulate async work
        return a + b;
    }

    largeData(size: number): { data: number[]; length: number } {
        return {
            data: Array.from({ length: size }, (_, i) => i),
            length: size,
        };
    }

    // Real-world computation: Fibonacci (iterative, efficient)
    fibonacci(n: number): number {
        if (n < 0) {
            throw new Error("Fibonacci not defined for negative numbers");
        }
        if (n === 0) return 0;
        if (n === 1) return 1;

        let a = 0, b = 1;
        for (let i = 2; i <= n; i++) {
            const temp = a + b;
            a = b;
            b = temp;
        }
        return b;
    }

    // Recursive fibonacci (slower, demonstrates async)
    async fibonacciAsync(n: number): Promise<number> {
        // Simulate computation time
        await new Promise(r => setTimeout(r, 50));
        return this.fibonacci(n);
    }
}

// Run the worker if this is the entry point (supports both --worker and --parent-test-worker flags)
if (process.argv.includes("--worker") || process.argv.includes("--parent-test-worker")) {
    new MathWorker().run();
}