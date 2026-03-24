# Multifrost Go Quick Examples

## Install

```bash
go get github.com/multifrost/golang
```

## Caller Example

```go
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
		ValidateTarget:   true,
		RequestTimeout:   10 * time.Second,
		BootstrapTimeout: 10 * time.Second,
	})
	handle := connection.Handle()

	if err := handle.Start(ctx); err != nil {
		log.Fatal(err)
	}
	defer handle.Stop(context.Background())

	result, err := handle.Call(ctx, "add", 10, 20)
	if err != nil {
		log.Fatal(err)
	}

	fmt.Println(result)
}
```

## Service Example

```go
package main

import (
	"context"
	"fmt"

	"github.com/multifrost/golang"
)

type MathService struct{}

func (s *MathService) HandleCall(ctx context.Context, function string, args []any) (any, error) {
	switch function {
	case "add":
		return 30, nil
	default:
		return nil, fmt.Errorf("function not found: %s", function)
	}
}

func main() {
	if err := multifrost.RunService(context.Background(), &MathService{}, multifrost.ServiceContext{PeerID: "math-service"}); err != nil {
		panic(err)
	}
}
```

## Spawn Example

```go
package main

import (
	"context"
	"log"

	"github.com/multifrost/golang"
)

func main() {
	ctx := context.Background()
	process, err := multifrost.Spawn("examples/math_worker/main.go")
	if err != nil {
		log.Fatal(err)
	}
	defer process.Stop(context.Background())

	connection := multifrost.Connect("math-service", multifrost.ConnectOptions{ValidateTarget: true})
	handle := connection.Handle()
	if err := handle.Start(ctx); err != nil {
		log.Fatal(err)
	}
	defer handle.Stop(context.Background())

	_, _ = handle.Call(ctx, "add", 1, 2)
}
```

## Cross-Language

The same caller surface can connect to Rust, JavaScript, or Python service peers as long as they speak the v5 router protocol.
