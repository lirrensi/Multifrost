package main

import (
	"fmt"

	"github.com/multifrost/golang"
)

// MathWorker is a sample worker that provides math operations
type MathWorker struct {
	*multifrost.ChildWorker
}

// Add adds two numbers
func (w *MathWorker) Add(a, b int) int {
	return a + b
}

// Subtract subtracts two numbers
func (w *MathWorker) Subtract(a, b int) int {
	return a - b
}

// Multiply multiplies two numbers
func (w *MathWorker) Multiply(a, b int) int {
	return a * b
}

// Greet returns a greeting message
func (w *MathWorker) Greet(name string) string {
	return fmt.Sprintf("Hello, %s!", name)
}

func main() {
	worker := &MathWorker{
		ChildWorker: multifrost.NewChildWorker(),
	}
	worker.Run()
}
