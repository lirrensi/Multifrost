package multifrost

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"time"
)

func TestCallerRegistrationAckAndDuplicatePeerIDRejection(t *testing.T) {
	env := sharedTestEnv(t)
	ensureRouterBinaryExists(t, env)

	peerID := uniquePeerID(t, "caller")
	connection := Connect("unused-target", ConnectOptions{
		PeerID:           peerID,
		RouterPort:       env.routerPort,
		RequestTimeout:   5 * time.Second,
		BootstrapTimeout: 5 * time.Second,
	})

	handle := connection.Handle()
	if err := handle.Start(context.Background()); err != nil {
		t.Fatalf("first caller start failed: %v", err)
	}
	t.Cleanup(func() { _ = handle.Stop(context.Background()) })

	duplicate := Connect("unused-target", ConnectOptions{
		PeerID:           peerID,
		RouterPort:       env.routerPort,
		RequestTimeout:   5 * time.Second,
		BootstrapTimeout: 5 * time.Second,
	}).Handle()

	err := duplicate.Start(context.Background())
	var regErr *RegistrationError
	if !errors.As(err, &regErr) {
		t.Fatalf("expected registration error, got %T: %v", err, err)
	}
	if regErr.Origin != ErrorOriginRegistration {
		t.Fatalf("unexpected registration error origin: %s", regErr.Origin)
	}
}

func TestGoCallerToGoServiceRoundTripQueriesAndDisconnectInvalidation(t *testing.T) {
	env := sharedTestEnv(t)
	ensureRouterBinaryExists(t, env)

	process := startGoServiceProcess(t, env)
	defer func() {
		_ = process.Stop(context.Background())
	}()

	callerPeerID := uniquePeerID(t, "caller")
	connection := Connect("math-service", ConnectOptions{
		PeerID:           callerPeerID,
		RouterPort:       env.routerPort,
		RequestTimeout:   10 * time.Second,
		BootstrapTimeout: 10 * time.Second,
	})
	handle := connection.Handle()
	if err := handle.Start(context.Background()); err != nil {
		t.Fatalf("caller start failed: %v", err)
	}
	t.Cleanup(func() { _ = handle.Stop(context.Background()) })

	waitForPeer(t, handle, "math-service", 20*time.Second)

	sum, err := handle.Call(context.Background(), "add", int64(10), int64(20))
	if err != nil {
		t.Fatalf("call add: %v", err)
	}
	if got := asInt64Value(t, sum); got != 30 {
		t.Fatalf("unexpected add result: %d", got)
	}

	payload := []byte{0x00, 0x01, 0x02, 0xff, 0x10}
	echo, err := handle.Call(context.Background(), "echo", payload)
	if err != nil {
		t.Fatalf("call echo: %v", err)
	}
	echoBytes, ok := echo.([]byte)
	if !ok {
		t.Fatalf("expected []byte echo result, got %T", echo)
	}
	if !bytes.Equal(echoBytes, payload) {
		t.Fatalf("echo payload changed: got %v want %v", echoBytes, payload)
	}

	exists, err := handle.QueryPeerExists(context.Background(), "math-service")
	if err != nil {
		t.Fatalf("query peer.exists: %v", err)
	}
	if !exists {
		t.Fatal("expected math-service to exist")
	}

	peerInfo, err := handle.QueryPeerGet(context.Background(), "math-service")
	if err != nil {
		t.Fatalf("query peer.get: %v", err)
	}
	if peerInfo.Class == nil || *peerInfo.Class != PeerClassService {
		t.Fatalf("unexpected peer class: %#v", peerInfo.Class)
	}
	if !peerInfo.Connected {
		t.Fatal("expected peer.get to report connected=true")
	}

	if err := process.Stop(context.Background()); err != nil {
		t.Fatalf("stop go service: %v", err)
	}
	waitForPeerAbsent(t, handle, "math-service", 20*time.Second)
}

func TestGoCallerSurfacesTransportErrorWhenRouterDies(t *testing.T) {
	tempHome := t.TempDir()
	routerPort := freeTCPPort()
	routerBin := filepath.Join(repoRoot(), "router", "target", "debug", "multifrost-router")
	goServiceEntry := filepath.Join(repoRoot(), "golang", "examples", "e2e_math_worker", "main.go")

	if _, err := os.Stat(routerBin); err != nil {
		t.Fatalf("router binary missing at %s: %v", routerBin, err)
	}

	t.Setenv("HOME", tempHome)
	t.Setenv(RouterPortEnv, fmt.Sprintf("%d", routerPort))
	t.Setenv(RouterBinEnv, routerBin)

	routerCmd := exec.Command(routerBin)
	routerCmd.Env = append(os.Environ(),
		fmt.Sprintf("%s=%d", RouterPortEnv, routerPort),
		fmt.Sprintf("HOME=%s", tempHome),
	)
	if err := routerCmd.Start(); err != nil {
		t.Fatalf("start router: %v", err)
	}
	t.Cleanup(func() {
		if routerCmd.Process != nil {
			_ = routerCmd.Process.Kill()
			_, _ = routerCmd.Process.Wait()
		}
	})

	deadline := time.Now().Add(10 * time.Second)
	for time.Now().Before(deadline) {
		connection := Connect("math-service", ConnectOptions{RouterPort: routerPort})
		handle := connection.Handle()
		err := handle.Start(context.Background())
		if err == nil {
			_ = handle.Stop(context.Background())
			break
		}
		time.Sleep(100 * time.Millisecond)
	}

	process, err := Spawn(goServiceEntry)
	if err != nil {
		t.Fatalf("spawn go service: %v", err)
	}
	defer func() {
		_ = process.Stop(context.Background())
	}()

	connection := Connect("math-service", ConnectOptions{
		PeerID:         uniquePeerID(t, "caller"),
		RouterPort:     routerPort,
		RequestTimeout: 2 * time.Second,
	})
	handle := connection.Handle()
	if err := handle.Start(context.Background()); err != nil {
		t.Fatalf("caller start failed: %v", err)
	}
	defer func() {
		_ = handle.Stop(context.Background())
	}()

	waitForPeer(t, handle, "math-service", 20*time.Second)

	if err := routerCmd.Process.Kill(); err != nil {
		t.Fatalf("kill router: %v", err)
	}
	_, _ = routerCmd.Process.Wait()

	deadline = time.Now().Add(10 * time.Second)
	for time.Now().Before(deadline) {
		_, err := handle.QueryPeerExists(context.Background(), "math-service")
		if err == nil {
			time.Sleep(100 * time.Millisecond)
			continue
		}

		var transportErr *TransportError
		if !errors.As(err, &transportErr) {
			t.Fatalf("expected transport error after router death, got %T: %v", err, err)
		}
		return
	}

	t.Fatal("router death did not surface a transport error in time")
}
