package main

import (
	"fmt"
	"log"
	"time"

	"github.com/multifrost/golang"
)

func main() {
	// Spawn a Go child worker
	worker := multifrost.Spawn("examples/math_worker", "go", "run")

	handle := worker.Handle()
	if err := handle.Start(); err != nil {
		log.Fatalf("Failed to start worker: %v", err)
	}
	defer handle.Stop()

	// Wait for worker to be ready
	time.Sleep(1 * time.Second)

	// Add
	result, err := handle.Call("Add", 5, 3)
	if err != nil {
		log.Printf("Add failed: %v", err)
	} else {
		fmt.Printf("Add(5, 3) = %v\n", result)
	}

	// Subtract
	result, err = handle.Call("Subtract", 10, 4)
	if err != nil {
		log.Printf("Subtract failed: %v", err)
	} else {
		fmt.Printf("Subtract(10, 4) = %v\n", result)
	}

	// Multiply
	result, err = handle.Call("Multiply", 6, 7)
	if err != nil {
		log.Printf("Multiply failed: %v", err)
	} else {
		fmt.Printf("Multiply(6, 7) = %v\n", result)
	}

	// Greet
	result, err = handle.Call("Greet", "World")
	if err != nil {
		log.Printf("Greet failed: %v", err)
	} else {
		fmt.Printf("Greet(\"World\") = %v\n", result)
	}

	fmt.Println("Done!")
}
