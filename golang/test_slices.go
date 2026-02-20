//go:build ignore
// +build ignore

package main

import (
	"fmt"
	"reflect"
)

// unwrapInterface unwraps an interface{} value to get the underlying value.
// This handles msgpack decoding where all values are decoded as interface{}.
func unwrapInterface(value reflect.Value) reflect.Value {
	// Only unwrap if it's an interface with a valid underlying value
	if value.Kind() == reflect.Interface && value.Elem().IsValid() {
		return value.Elem()
	}
	return value
}

// convertArg converts a reflect.Value to match the expected target type.
func convertArg(value reflect.Value, target reflect.Type) reflect.Value {
	// If already the right type, return as-is
	if value.Type() == target {
		return value
	}

	// Handle nil/invalid
	if !value.IsValid() {
		return reflect.Zero(target)
	}

	// Unwrap interface{} values from msgpack decoding
	value = unwrapInterface(value)

	srcKind := value.Kind()

	// Handle integer conversions
	if isIntegerKind(srcKind) && isIntegerKind(target.Kind()) {
		srcInt := value.Int()
		return reflect.ValueOf(srcInt).Convert(target)
	}

	// Handle float->int conversion
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

	// Handle slices/arrays
	if target.Kind() == reflect.Slice || target.Kind() == reflect.Array {
		if srcKind == reflect.Slice || srcKind == reflect.Array {
			return convertSlice(value, target)
		}
	}

	// Handle maps
	if target.Kind() == reflect.Map && srcKind == reflect.Map {
		return convertMap(value, target)
	}

	// Try direct conversion if possible
	if value.CanConvert(target) {
		return value.Convert(target)
	}

	// Fallback: return as-is
	return value
}

func isIntegerKind(k reflect.Kind) bool {
	return k == reflect.Int || k == reflect.Int8 || k == reflect.Int16 ||
		k == reflect.Int32 || k == reflect.Int64 ||
		k == reflect.Uint || k == reflect.Uint8 || k == reflect.Uint16 ||
		k == reflect.Uint32 || k == reflect.Uint64
}

// convertSlice converts a slice/array to the target slice type
func convertSlice(value reflect.Value, target reflect.Type) reflect.Value {
	length := value.Len()
	targetElemType := target.Elem()
	result := reflect.MakeSlice(target, length, length)

	for i := 0; i < length; i++ {
		elem := value.Index(i)
		// Unwrap interface{} from msgpack decoding before conversion
		elem = unwrapInterface(elem)
		converted := convertArg(elem, targetElemType)
		result.Index(i).Set(converted)
	}

	return result
}

// convertMap converts a map to the target map type
func convertMap(value reflect.Value, target reflect.Type) reflect.Value {
	targetKeyType := target.Key()
	targetValueType := target.Elem()
	result := reflect.MakeMap(target)

	for _, key := range value.MapKeys() {
		// Unwrap interface{} from msgpack decoding before conversion
		key = unwrapInterface(key)
		convertedKey := convertArg(key, targetKeyType)

		val := value.MapIndex(key)
		val = unwrapInterface(val)
		convertedValue := convertArg(val, targetValueType)
		result.SetMapIndex(convertedKey, convertedValue)
	}

	return result
}

// Simulated dispatch - what the child_worker does
func dispatch(method reflect.Value, args []any) []reflect.Value {
	methodType := method.Type()
	convertedArgs := make([]reflect.Value, len(args))

	for i, arg := range args {
		if i < methodType.NumIn() {
			// Convert argument to match expected parameter type
			convertedArgs[i] = convertArg(reflect.ValueOf(arg), methodType.In(i))
		} else {
			convertedArgs[i] = reflect.ValueOf(arg)
		}
	}

	return method.Call(convertedArgs)
}

func main() {
	// Test 1: Add (int, int) - the basic case
	fmt.Println("=== Test 1: Add(int, int) ===")
	addFunc := func(a, b int) int { return a + b }
	method := reflect.ValueOf(addFunc)
	result := dispatch(method, []any{int64(10), int64(20)})
	fmt.Printf("Result: %v\n", result[0].Int())

	// Test 2: Echo with int64 (interface{})
	fmt.Println("\n=== Test 2: Echo(interface{}) with int64 ===")
	echoFunc := func(v interface{}) interface{} { return v }
	method = reflect.ValueOf(echoFunc)
	result = dispatch(method, []any{int64(42)})
	fmt.Printf("Result: %v (type: %T)\n", result[0].Interface(), result[0].Interface())

	// Test 3: Echo with string
	fmt.Println("\n=== Test 3: Echo(interface{}) with string ===")
	result = dispatch(method, []any{"hello"})
	fmt.Printf("Result: %v\n", result[0].Interface())

	// Test 4: What if we pass []interface{} to a method expecting []int?
	fmt.Println("\n=== Test 4: Slice type mismatch (the bug case) ===")
	// This is a more complex case - msgpack decodes arrays as []interface{}
	// but Go method might expect []int
	sliceMethod := func(arr []int) int {
		sum := 0
		for _, v := range arr {
			sum += v
		}
		return sum
	}
	method = reflect.ValueOf(sliceMethod)

	// What does msgpack decode []int as? It's []interface{}
	// Let's simulate: Python sends [1, 2, 3] which becomes []interface{} after msgpack
	inputSlice := []interface{}{int64(1), int64(2), int64(3)}
	result = dispatch(method, []any{inputSlice})
	fmt.Printf("Result: %v\n", result[0].Int())

	// Test 5: Slice of strings
	fmt.Println("\n=== Test 5: Slice of strings ===")
	strSliceMethod := func(arr []string) int {
		return len(arr)
	}
	method = reflect.ValueOf(strSliceMethod)
	inputStrSlice := []interface{}{"hello", "world"}
	result = dispatch(method, []any{inputStrSlice})
	fmt.Printf("Result: %v\n", result[0].Int())

	// Test 6: Map conversion
	fmt.Println("\n=== Test 6: Map with interface{} keys/values ===")
	mapMethod := func(m map[string]int) int {
		sum := 0
		for _, v := range m {
			sum += v
		}
		return sum
	}
	method = reflect.ValueOf(mapMethod)
	// msgpack decodes {"a": 1} as map[string]interface{}
	inputMap := map[string]interface{}{"a": int64(1), "b": int64(2)}
	result = dispatch(method, []any{inputMap})
	fmt.Printf("Result: %v\n", result[0].Int())

	fmt.Println("\n=== All tests passed! ===")
}
