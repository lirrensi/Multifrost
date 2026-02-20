//go:build ignore
// +build ignore

package main

import (
	"fmt"
	"reflect"
)

func main() {
	// This is what happens when msgpack decodes a slice
	// Python sends [1, 2, 3] as msgpack array
	// Go msgpack decodes it as []interface{}

	slice := []interface{}{int64(1), int64(2), int64(3)}
	sliceValue := reflect.ValueOf(slice)

	fmt.Printf("Slice type: %v\n", sliceValue.Type())
	fmt.Printf("Slice kind: %v\n", sliceValue.Kind())

	// Each element
	for i := 0; i < sliceValue.Len(); i++ {
		elem := sliceValue.Index(i)
		fmt.Printf("\nElement %d:\n", i)
		fmt.Printf("  Type: %v\n", elem.Type())
		fmt.Printf("  Kind: %v\n", elem.Kind())
		fmt.Printf("  Value: %v\n", elem.Interface())

		// What is the underlying type?
		if elem.Kind() == reflect.Interface {
			underlying := elem.Elem()
			fmt.Printf("  Underlying type: %v\n", underlying.Type())
			fmt.Printf("  Underlying kind: %v\n", underlying.Kind())
		}
	}
}
