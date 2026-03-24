// FILE: golang/examples/math_worker/main.go
// PURPOSE: Show the minimal Go v5 service peer running explicit HandleCall dispatch.
// OWNS: Standalone math-service example.
// EXPORTS: main.
// DOCS: docs/spec.md, golang/docs/quick-examples.md
package main

import (
	"context"
	"fmt"
	"os"

	"github.com/multifrost/golang"
)

type MathService struct{}

func (s *MathService) HandleCall(ctx context.Context, function string, args []any) (any, error) {
	switch function {
	case "add":
		a, err := asInt64(args, 0)
		if err != nil {
			return nil, err
		}
		b, err := asInt64(args, 1)
		if err != nil {
			return nil, err
		}
		return a + b, nil
	case "multiply":
		a, err := asInt64(args, 0)
		if err != nil {
			return nil, err
		}
		b, err := asInt64(args, 1)
		if err != nil {
			return nil, err
		}
		return a * b, nil
	case "divide":
		a, err := asInt64(args, 0)
		if err != nil {
			return nil, err
		}
		b, err := asInt64(args, 1)
		if err != nil {
			return nil, err
		}
		if b == 0 {
			return nil, fmt.Errorf("division by zero")
		}
		return a / b, nil
	case "factorial":
		n, err := asInt64(args, 0)
		if err != nil {
			return nil, err
		}
		if n < 0 {
			return nil, fmt.Errorf("factorial not defined for negative numbers")
		}
		result := int64(1)
		for i := int64(2); i <= n; i++ {
			result *= i
		}
		return result, nil
	case "fibonacci":
		n, err := asInt64(args, 0)
		if err != nil {
			return nil, err
		}
		if n < 0 {
			return nil, fmt.Errorf("fibonacci not defined for negative numbers")
		}
		return fibonacci(n), nil
	case "echo":
		if len(args) == 0 {
			return nil, nil
		}
		return args[0], nil
	case "large_data":
		size, err := asInt64(args, 0)
		if err != nil {
			return nil, err
		}
		if size < 0 {
			return nil, fmt.Errorf("size must be non-negative")
		}
		data := make([]int64, int(size))
		for i := range data {
			data[i] = int64(i)
		}
		return map[string]any{"data": data, "length": len(data)}, nil
	case "get_info":
		return map[string]any{
			"language": "go",
			"pid":      os.Getpid(),
			"version":  multifrost.Version,
		}, nil
	case "throw_error":
		message := "boom"
		if len(args) > 0 {
			if value, ok := args[0].(string); ok && value != "" {
				message = value
			}
		}
		if message == "" {
			message = "boom"
		}
		return nil, fmt.Errorf(message)
	default:
		return nil, fmt.Errorf("unknown function: %s", function)
	}
}

func asInt64(args []any, index int) (int64, error) {
	if index >= len(args) {
		return 0, fmt.Errorf("missing arg[%d]", index)
	}

	switch value := args[index].(type) {
	case int:
		return int64(value), nil
	case int8:
		return int64(value), nil
	case int16:
		return int64(value), nil
	case int32:
		return int64(value), nil
	case int64:
		return value, nil
	case uint:
		return int64(value), nil
	case uint8:
		return int64(value), nil
	case uint16:
		return int64(value), nil
	case uint32:
		return int64(value), nil
	case uint64:
		return int64(value), nil
	case float32:
		return int64(value), nil
	case float64:
		return int64(value), nil
	default:
		return 0, fmt.Errorf("arg[%d] must be numeric, got %T", index, args[index])
	}
}

func fibonacci(n int64) int64 {
	if n <= 1 {
		return n
	}
	var a int64
	b := int64(1)
	for i := int64(2); i <= n; i++ {
		a, b = b, a+b
	}
	return b
}

func main() {
	if err := multifrost.RunService(context.Background(), &MathService{}, multifrost.ServiceContext{PeerID: "math-service"}); err != nil {
		panic(err)
	}
}
