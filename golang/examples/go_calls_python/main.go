package main

import (
	"context"
	"fmt"
	"log"
	"time"

	"github.com/multifrost/golang"
)

// This example shows Go calling a Python worker
func main() {
	// Spawn a Python child worker
	worker := multifrost.Spawn("../examples/math_worker.py", "python")

	if err := worker.Start(); err != nil {
		log.Fatalf("Failed to start worker: %v", err)
	}
	defer worker.Close()

	// Wait for worker to be ready
	time.Sleep(1 * time.Second)

	ctx := context.Background()

	// Call Python's add function
	result, err := worker.ACall.Call(ctx, "add", 10, 20)
	if err != nil {
		log.Printf("add failed: %v", err)
	} else {
		fmt.Printf("Python add(10, 20) = %v\n", result)
	}

	fmt.Println("Done!")
}
