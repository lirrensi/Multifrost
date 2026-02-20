//go:build ignore
// +build ignore

package main

import (
	"fmt"
	"reflect"
)

func main() {
	// Test what happens when we have interface{} parameter type
	// This is what the Echo method uses: func (w *MathWorker) Echo(value interface{}) interface{}

	echoFunc := func(value interface{}) interface{} { return value }
	methodValue := reflect.ValueOf(echoFunc)
	methodType := methodValue.Type()

	fmt.Printf("Method type: %v\n", methodType)
	fmt.Printf("Param 0 type: %v\n", methodType.In(0))

	// Test with int64 (what Python sends)
	pythonInt := int64(42)
	arg := reflect.ValueOf(pythonInt)

	fmt.Printf("\nInput: %v (type: %T)\n", pythonInt, pythonInt)
	fmt.Printf("Input reflect: %v (kind: %v)\n", arg, arg.Kind())

	// What target type is?
	targetType := methodType.In(0)
	fmt.Printf("Target type: %v (kind: %v)\n", targetType, targetType.Kind())

	// Check if interface{}
	isInterface := targetType.Kind() == reflect.Interface
	fmt.Printf("Is interface: %v\n", isInterface)

	// Try the conversion the current code does
	if targetType.Kind() == reflect.Interface {
		// The current code returns the value as-is
		converted := arg
		fmt.Printf("As interface - value: %v (type: %v)\n", converted, converted.Type())

		// Try calling
		result := methodValue.Call([]reflect.Value{converted})
		fmt.Printf("Result: %v (type: %T)\n", result[0].Interface(), result[0].Interface())
	}
}
