// FILE: golang/router_bootstrap.go
// PURPOSE: Discover, start, and wait for the shared v5 router using the canonical WebSocket endpoint.
// OWNS: Router reachability checks, startup locking, log/lock paths, router process bootstrap.
// EXPORTS: routerPortFromEnv, routerEndpoint, defaultRouterLockPath, defaultRouterLogPath, ensureRouter.
// DOCS: docs/spec.md, agent_chat/go_v5_api_surface_2026-03-25.md
package multifrost

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"time"

	"github.com/coder/websocket"
)

const (
	routerBootstrapTimeout   = 10 * time.Second
	routerBootstrapPollDelay = 100 * time.Millisecond
	routerReachabilityProbe  = 250 * time.Millisecond
	routerLockRetryDelay     = 100 * time.Millisecond
	routerLockMaxAge         = 2 * time.Minute
)

func routerPortFromEnv() int {
	if value := os.Getenv(RouterPortEnv); value != "" {
		if port, err := strconv.Atoi(value); err == nil && port > 0 && port <= 65535 {
			return port
		}
	}
	return DefaultRouterPort
}

func routerEndpoint(port int) string {
	return fmt.Sprintf("ws://127.0.0.1:%d", port)
}

func defaultRouterLockPath() string {
	return filepath.Join(userHomeDir(), ".multifrost", "router.lock")
}

func defaultRouterLogPath() string {
	return filepath.Join(userHomeDir(), ".multifrost", "router.log")
}

func userHomeDir() string {
	if home, err := os.UserHomeDir(); err == nil && home != "" {
		return home
	}
	if home := os.Getenv("HOME"); home != "" {
		return home
	}
	return "."
}

func ensureRouter(ctx context.Context, port int) error {
	if ctx == nil {
		ctx = context.Background()
	}

	endpoint := routerEndpoint(port)
	if routerReachable(ctx, endpoint) {
		return nil
	}

	lock, err := acquireRouterLock(ctx)
	if err != nil {
		return &BootstrapError{Origin: ErrorOriginBootstrap, Err: err}
	}
	defer lock.release()

	if routerReachable(ctx, endpoint) {
		return nil
	}

	if err := spawnRouterProcess(port); err != nil {
		return &BootstrapError{Origin: ErrorOriginBootstrap, Err: err}
	}

	deadline := time.Now().Add(routerBootstrapTimeout)
	for {
		if routerReachable(ctx, endpoint) {
			return nil
		}
		if time.Now().After(deadline) {
			return &BootstrapError{
				Origin: ErrorOriginBootstrap,
				Err:    fmt.Errorf("router did not become reachable at %s within %s", endpoint, routerBootstrapTimeout),
			}
		}

		select {
		case <-ctx.Done():
			return &BootstrapError{Origin: ErrorOriginBootstrap, Err: ctx.Err()}
		case <-time.After(routerBootstrapPollDelay):
		}
	}
}

func routerReachable(ctx context.Context, endpoint string) bool {
	if ctx == nil {
		ctx = context.Background()
	}

	probeCtx, cancel := context.WithTimeout(ctx, routerReachabilityProbe)
	defer cancel()

	conn, _, err := websocket.Dial(probeCtx, endpoint, nil)
	if err != nil {
		return false
	}
	_ = conn.Close(websocket.StatusNormalClosure, "")
	return true
}

type routerLockGuard struct {
	path string
}

func (g *routerLockGuard) release() {
	if g == nil || g.path == "" {
		return
	}
	_ = os.Remove(g.path)
}

func acquireRouterLock(ctx context.Context) (*routerLockGuard, error) {
	lockPath := defaultRouterLockPath()
	if err := os.MkdirAll(filepath.Dir(lockPath), 0o755); err != nil {
		return nil, err
	}

	deadline := time.Now().Add(routerBootstrapTimeout)
	for {
		file, err := os.OpenFile(lockPath, os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0o600)
		if err == nil {
			_, _ = fmt.Fprintf(file, "%d\n", os.Getpid())
			_ = file.Close()
			return &routerLockGuard{path: lockPath}, nil
		}
		if !errors.Is(err, os.ErrExist) {
			return nil, err
		}

		if isStaleLock(lockPath) {
			_ = os.Remove(lockPath)
			continue
		}
		if time.Now().After(deadline) {
			return nil, fmt.Errorf("timed out waiting for router startup lock")
		}

		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		case <-time.After(routerLockRetryDelay):
		}
	}
}

func isStaleLock(path string) bool {
	info, err := os.Stat(path)
	if err != nil {
		return false
	}
	return time.Since(info.ModTime()) > routerLockMaxAge
}

func spawnRouterProcess(port int) error {
	binary := os.Getenv(RouterBinEnv)
	if binary == "" {
		binary = "multifrost-router"
	}

	logPath := defaultRouterLogPath()
	if err := os.MkdirAll(filepath.Dir(logPath), 0o755); err != nil {
		return err
	}

	logFile, err := os.OpenFile(logPath, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		return err
	}
	defer logFile.Close()

	cmd := exec.Command(binary)
	cmd.Env = append(os.Environ(), fmt.Sprintf("%s=%d", RouterPortEnv, port))
	cmd.Stdout = logFile
	cmd.Stderr = logFile

	if err := cmd.Start(); err != nil {
		return err
	}
	return nil
}
