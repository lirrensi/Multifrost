package multifrost

import (
	"context"
	"fmt"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"
)

type testEnv struct {
	homeDir           string
	routerPort        int
	routerBin         string
	goServiceEntry    string
	rustServiceBinary string
	rustConnectBinary string
}

var sharedTestEnvOnce sync.Once
var sharedTestEnvValue testEnv

func sharedTestEnv(t *testing.T) testEnv {
	t.Helper()

	sharedTestEnvOnce.Do(func() {
		root := repoRoot()
		homeDir, err := os.MkdirTemp("", "multifrost-go-home-")
		if err != nil {
			panic(err)
		}

		routerBin := filepath.Join(root, "router", "target", "debug", "multifrost-router")
		goServiceEntry := filepath.Join(root, "golang", "examples", "e2e_math_worker", "main.go")
		rustServiceBinary := filepath.Join(root, "rust", "target", "debug", "examples", "e2e_math_worker")
		rustConnectBinary := filepath.Join(root, "rust", "target", "debug", "examples", "connect")

		sharedTestEnvValue = testEnv{
			homeDir:           homeDir,
			routerPort:        freeTCPPort(),
			routerBin:         routerBin,
			goServiceEntry:    goServiceEntry,
			rustServiceBinary: rustServiceBinary,
			rustConnectBinary: rustConnectBinary,
		}
	})

	t.Setenv("HOME", sharedTestEnvValue.homeDir)
	t.Setenv(RouterPortEnv, strconv.Itoa(sharedTestEnvValue.routerPort))
	t.Setenv(RouterBinEnv, sharedTestEnvValue.routerBin)
	return sharedTestEnvValue
}

func repoRoot() string {
	cwd, err := os.Getwd()
	if err != nil {
		panic(err)
	}
	return filepath.Clean(filepath.Join(cwd, ".."))
}

func freeTCPPort() int {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		panic(err)
	}
	defer listener.Close()
	return listener.Addr().(*net.TCPAddr).Port
}

func waitForPeer(t *testing.T, handle *Handle, peerID string, timeout time.Duration) {
	t.Helper()

	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		exists, err := handle.QueryPeerExists(context.Background(), peerID)
		if err == nil && exists {
			return
		}
		time.Sleep(100 * time.Millisecond)
	}

	t.Fatalf("peer %q never appeared in the router registry", peerID)
}

func startGoServiceProcess(t *testing.T, env testEnv) *ServiceProcess {
	t.Helper()

	process, err := Spawn(env.goServiceEntry)
	if err != nil {
		t.Fatalf("spawn go service: %v", err)
	}

	t.Cleanup(func() {
		_ = process.Stop(context.Background())
	})
	return process
}

func startRustServiceProcess(t *testing.T, env testEnv) *exec.Cmd {
	t.Helper()

	cmd := exec.Command(env.rustServiceBinary, "--service")
	cmd.Env = append(os.Environ(),
		fmt.Sprintf("%s=%d", RouterPortEnv, env.routerPort),
		fmt.Sprintf("%s=%s", RouterBinEnv, env.routerBin),
		fmt.Sprintf("HOME=%s", env.homeDir),
	)

	if err := cmd.Start(); err != nil {
		t.Fatalf("start rust service: %v", err)
	}

	t.Cleanup(func() {
		_ = cmd.Process.Kill()
		_, _ = cmd.Process.Wait()
	})
	return cmd
}

func runRustConnectBinary(t *testing.T, env testEnv, args ...string) string {
	t.Helper()

	cmd := exec.Command(env.rustConnectBinary, args...)
	cmd.Env = append(os.Environ(),
		fmt.Sprintf("%s=%d", RouterPortEnv, env.routerPort),
		fmt.Sprintf("%s=%s", RouterBinEnv, env.routerBin),
		fmt.Sprintf("HOME=%s", env.homeDir),
	)

	output, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("rust connect binary failed: %v\n%s", err, string(output))
	}
	return string(output)
}

func uniquePeerID(t *testing.T, suffix string) string {
	t.Helper()
	base := strings.NewReplacer("/", "-", " ", "-").Replace(t.Name())
	return fmt.Sprintf("%s-%s", base, suffix)
}

func waitForPeerAbsent(t *testing.T, handle *Handle, peerID string, timeout time.Duration) {
	t.Helper()

	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		exists, err := handle.QueryPeerExists(context.Background(), peerID)
		if err == nil && !exists {
			return
		}
		time.Sleep(100 * time.Millisecond)
	}

	t.Fatalf("peer %q never disappeared from the router registry", peerID)
}

func asInt64Value(t *testing.T, value any) int64 {
	t.Helper()

	switch v := value.(type) {
	case int:
		return int64(v)
	case int8:
		return int64(v)
	case int16:
		return int64(v)
	case int32:
		return int64(v)
	case int64:
		return v
	case uint:
		return int64(v)
	case uint8:
		return int64(v)
	case uint16:
		return int64(v)
	case uint32:
		return int64(v)
	case uint64:
		return int64(v)
	case float32:
		return int64(v)
	case float64:
		return int64(v)
	default:
		t.Fatalf("expected numeric result, got %T", value)
		return 0
	}
}

func ensureRouterBinaryExists(t *testing.T, env testEnv) {
	t.Helper()

	if _, err := os.Stat(env.routerBin); err != nil {
		t.Fatalf("router binary missing at %s: %v", env.routerBin, err)
	}
}

func ensureRustBinaryExists(t *testing.T, path string) {
	t.Helper()

	if _, err := os.Stat(path); err != nil {
		t.Fatalf("binary missing at %s: %v", path, err)
	}
}

func containsAll(haystack string, needles ...string) bool {
	for _, needle := range needles {
		if !strings.Contains(haystack, needle) {
			return false
		}
	}
	return true
}
