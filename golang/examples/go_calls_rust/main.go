// FILE: golang/examples/go_calls_rust/main.go
// PURPOSE: Show a Go caller reaching the Rust v5 service example through the router.
// OWNS: Go-to-Rust caller example.
// EXPORTS: main.
// DOCS: docs/spec.md, golang/docs/quick-examples.md
package main

import (
	"context"
	"fmt"
	"log"
	"time"

	"github.com/multifrost/golang"
)

func main() {
	ctx := context.Background()

	connection := multifrost.Connect("math-service", multifrost.ConnectOptions{
		RequestTimeout:   10 * time.Second,
		BootstrapTimeout: 10 * time.Second,
	})
	handle := connection.Handle()

	if err := handle.Start(ctx); err != nil {
		log.Fatalf("start failed: %v", err)
	}
	defer handle.Stop(context.Background())

	for i := 0; i < 50; i++ {
		exists, err := handle.QueryPeerExists(ctx, "math-service")
		if err == nil && exists {
			break
		}
		time.Sleep(100 * time.Millisecond)
	}

	sum, err := handle.Call(ctx, "add", 10, 20)
	if err != nil {
		log.Fatalf("add failed: %v", err)
	}
	fmt.Printf("add(10, 20) = %v\n", sum)

	product, err := handle.Call(ctx, "multiply", 7, 8)
	if err != nil {
		log.Fatalf("multiply failed: %v", err)
	}
	fmt.Printf("multiply(7, 8) = %v\n", product)

	factorial, err := handle.Call(ctx, "factorial", 5)
	if err != nil {
		log.Fatalf("factorial failed: %v", err)
	}
	fmt.Printf("factorial(5) = %v\n", factorial)
}
