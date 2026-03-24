package multifrost

import (
	"bytes"
	"context"
	"errors"
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
