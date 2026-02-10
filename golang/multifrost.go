// Package multifrost provides inter-process communication between Go processes
// and Go-Python-Node.js processes using ZeroMQ and msgpack.
//
// # Architecture
//
// The library uses ROUTER/DEALER socket pattern:
//   - ChildWorker uses ROUTER socket (supports multiple parents)
//   - ParentWorker uses DEALER socket
//   - ServiceRegistry provides service discovery via JSON file
//
// # Quick Start
//
// Go Parent -> Go Child (Spawn Mode):
//
//	// Child worker implementation
//	type MathWorker struct {
//	    *multifrost.ChildWorker
//	}
//
//	func (w *MathWorker) Add(a, b int) int {
//	    return a + b
//	}
//
//	// In child process
//	func main() {
//	    worker := &MathWorker{ChildWorker: multifrost.NewChildWorker()}
//	    worker.Run()
//	}
//
//	// In parent process
//	worker := multifrost.Spawn("path/to/worker", "go", "run")
//	if err := worker.Start(); err != nil {
//	    log.Fatal(err)
//	}
//	defer worker.Close()
//
//	result, err := worker.Call.Call(context.Background(), "Add", 1, 2)
//
// Go Parent -> Go Child (Connect Mode):
//
//	// Child (runs independently as microservice)
//	type MathWorker struct {
//	    *multifrost.ChildWorker
//	}
//
//	func main() {
//	    worker := &MathWorker{ChildWorker: multifrost.NewChildWorkerWithService("math-service")}
//	    worker.Run()
//	}
//
//	// Parent (connects to running service)
//	worker, err := multifrost.Connect(ctx, "math-service")
//	if err != nil {
//	    log.Fatal(err)
//	}
//	if err := worker.Start(); err != nil {
//	    log.Fatal(err)
//	}
//	defer worker.Close()
//
//	result, err := worker.Call.Call(context.Background(), "Add", 1, 2)
//
// Go Parent -> Python Child:
//
//	worker := multifrost.Spawn("math_worker.py", "python")
//	if err := worker.Start(); err != nil {
//	    log.Fatal(err)
//	}
//	defer worker.Close()
//
//	result, err := worker.Call.Call(context.Background(), "add", 1, 2)
//
// Go Parent -> Node.js Child:
//
//	worker := multifrost.Spawn("node_worker.js", "node")
//	if err := worker.Start(); err != nil {
//	    log.Fatal(err)
//	}
//	defer worker.Close()
//
//	result, err := worker.Call.Call(context.Background(), "add", 1, 2)
package multifrost

// Version is the current library version
const Version = "4.0.0"
