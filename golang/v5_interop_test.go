package multifrost

import (
	"context"
	"strings"
	"testing"
	"time"
)

func TestGoCallerToRustServiceInterop(t *testing.T) {
	env := sharedTestEnv(t)
	ensureRustBinaryExists(t, env.rustServiceBinary)

	process := startRustServiceProcess(t, env)
	defer func() {
		_ = process.Process.Kill()
		_, _ = process.Process.Wait()
	}()

	callerPeerID := uniquePeerID(t, "go-to-rust")
	handle := Connect("math-service", ConnectOptions{
		PeerID:           callerPeerID,
		RouterPort:       env.routerPort,
		RequestTimeout:   10 * time.Second,
		BootstrapTimeout: 10 * time.Second,
	}).Handle()

	if err := handle.Start(context.Background()); err != nil {
		t.Fatalf("caller start failed: %v", err)
	}
	t.Cleanup(func() { _ = handle.Stop(context.Background()) })

	waitForPeer(t, handle, "math-service", 20*time.Second)

	sum, err := handle.Call(context.Background(), "add", int64(10), int64(20))
	if err != nil {
		t.Fatalf("add call failed: %v", err)
	}
	if got := asInt64Value(t, sum); got != 30 {
		t.Fatalf("unexpected add result: %d", got)
	}

	product, err := handle.Call(context.Background(), "multiply", int64(7), int64(8))
	if err != nil {
		t.Fatalf("multiply call failed: %v", err)
	}
	if got := asInt64Value(t, product); got != 56 {
		t.Fatalf("unexpected multiply result: %d", got)
	}
}

func TestRustCallerToGoServiceInterop(t *testing.T) {
	env := sharedTestEnv(t)
	ensureRustBinaryExists(t, env.rustConnectBinary)

	process := startGoServiceProcess(t, env)
	defer func() {
		_ = process.Stop(context.Background())
	}()

	callerPeerID := uniquePeerID(t, "rust-to-go")
	handle := Connect("math-service", ConnectOptions{
		PeerID:           callerPeerID,
		RouterPort:       env.routerPort,
		RequestTimeout:   10 * time.Second,
		BootstrapTimeout: 10 * time.Second,
	}).Handle()

	if err := handle.Start(context.Background()); err != nil {
		t.Fatalf("caller start failed: %v", err)
	}
	t.Cleanup(func() { _ = handle.Stop(context.Background()) })

	waitForPeer(t, handle, "math-service", 20*time.Second)

	output := runRustConnectBinary(t, env, "--target", "math-service", "--timeout-ms", "10000")
	if !containsAll(output, "add(10, 20) = 30", "multiply(7, 8) = 56", "factorial(5) = 120") {
		t.Fatalf("rust caller output missing expected results:\n%s", output)
	}

	if !strings.Contains(output, "Connected as") {
		t.Fatalf("rust caller output missing connection message:\n%s", output)
	}
}
