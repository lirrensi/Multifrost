//go:build ignore
// +build ignore

package main

import (
	"fmt"
	"reflect"
)

func convertArg(value reflect.Value, target reflect.Type) reflect.Value {
	// If already the right type, return as-is
	if value.Type() == target {
		return value
	}

	// Handle nil/invalid
	if !value.IsValid() {
		return reflect.Zero(target)
	}

	srcKind := value.Kind()

	// Handle integer conversions (msgpack sends int64, Go methods may expect int/int32/etc)
	if isIntegerKind(srcKind) && isIntegerKind(target.Kind()) {
		srcInt := value.Int()
		return reflect.ValueOf(srcInt).Convert(target)
	}

	// Handle float->int conversion (msgpack may send floats for small integers)
	if srcKind == reflect.Float64 || srcKind == reflect.Float32 {
		if isIntegerKind(target.Kind()) {
			srcFloat := value.Float()
			return reflect.ValueOf(int64(srcFloat)).Convert(target)
		}
	}

	// Handle int->float conversion
	if isIntegerKind(srcKind) && (target.Kind() == reflect.Float64 || target.Kind() == reflect.Float32) {
		srcInt := value.Int()
		return reflect.ValueOf(float64(srcInt)).Convert(target)
	}

	// Handle interface{} / any - just return the value
	if target == reflect.TypeOf((*any)(nil)).Elem() {
		return value
	}

	// Try direct conversion if possible
	if value.CanConvert(target) {
		return value.Convert(target)
	}

	// Fallback: return as-is and let the call fail with a clear error
	return value
}

func isIntegerKind(k reflect.Kind) bool {
	return k == reflect.Int || k == reflect.Int8 || k == reflect.Int16 ||
		k == reflect.Int32 || k == reflect.Int64 ||
		k == reflect.Uint || k == reflect.Uint8 || k == reflect.Uint16 ||
		k == reflect.Uint32 || k == reflect.Uint64
}

func add(a, b int) int {
	return a + b
}

func main() {
	// Get method type
	methodValue := reflect.ValueOf(add)
	methodType := methodValue.Type()

	fmt.Printf("Method type: %v\n", methodType)
	fmt.Printf("Param 0 type: %v\n", methodType.In(0))
	fmt.Printf("Param 1 type: %v\n", methodType.In(1))

	// Simulate Python sending int64 (msgpack default)
	pythonInt := int64(10) // This is what msgpack sends
	otherArg := int64(20)

	fmt.Printf("\nPython sending: %v (type: %T)\n", pythonInt, pythonInt)

	// Convert argument 0
	arg0 := convertArg(reflect.ValueOf(pythonInt), methodType.In(0))
	fmt.Printf("Converted arg0: %v (type: %v)\n", arg0, arg0.Type())

	// Convert argument 1
	arg1 := convertArg(reflect.ValueOf(otherArg), methodType.In(1))
	fmt.Printf("Converted arg1: %v (type: %v)\n", arg1, arg1.Type())

	// Try calling
	fmt.Println("\nCalling method...")
	result := methodValue.Call([]reflect.Value{arg0, arg1})
	fmt.Printf("Result: %v\n", result[0].Int())
}
