// E2E Test Worker - Go implementation
// This worker provides various methods for cross-language testing.
// Build from golang directory: go build -o ../e2e/workers/math_worker_go.exe ./examples/e2e_math_worker
package main

import (
	"fmt"
	"os"

	"github.com/multifrost/golang"
)

// MathWorker is a worker that provides math operations for E2E testing
type MathWorker struct {
	*multifrost.ChildWorker
}

// Add adds two numbers
func (w *MathWorker) Add(a, b int) int {
	return a + b
}

// Multiply multiplies two numbers
func (w *MathWorker) Multiply(a, b int) int {
	return a * b
}

// Factorial calculates factorial of n
func (w *MathWorker) Factorial(n int) (int, error) {
	if n < 0 {
		return 0, fmt.Errorf("factorial not defined for negative numbers")
	}
	if n > 20 {
		return 0, fmt.Errorf("input too large for int")
	}
	result := 1
	for i := 2; i <= n; i++ {
		result *= i
	}
	return result, nil
}

// Fibonacci calculates the nth Fibonacci number
func (w *MathWorker) Fibonacci(n int) (int, error) {
	if n < 0 {
		return 0, fmt.Errorf("fibonacci not defined for negative numbers")
	}
	if n == 0 {
		return 0, nil
	}
	if n == 1 {
		return 1, nil
	}
	a, b := 0, 1
	for i := 2; i <= n; i++ {
		a, b = b, a+b
	}
	return b, nil
}

// Echo returns the input value
func (w *MathWorker) Echo(value interface{}) interface{} {
	return value
}

// GetInfo returns worker information
func (w *MathWorker) GetInfo() map[string]interface{} {
	return map[string]interface{}{
		"language": "go",
		"pid":      os.Getpid(),
	}
}

// ThrowError raises an error with the given message
func (w *MathWorker) ThrowError(message string) error {
	return fmt.Errorf(message)
}

// Greet returns a greeting
func (w *MathWorker) Greet(name string) string {
	return fmt.Sprintf("Hello, %s!", name)
}

func main() {
	worker := &MathWorker{
		ChildWorker: multifrost.NewChildWorker(),
	}
	worker.SetSelf(worker) // Enable method discovery for embedded struct
	worker.Run()
}
