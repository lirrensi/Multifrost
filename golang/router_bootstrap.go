// FILE: golang/router_bootstrap.go
// PURPOSE: Discover, start, and wait for the shared v5 router using the canonical WebSocket endpoint.
// OWNS: Router reachability checks, startup locking, log/lock paths, router process bootstrap.
// EXPORTS: routerPortFromEnv, routerEndpoint, defaultRouterLockPath, defaultRouterLogPath, ensureRouter.
// DOCS: docs/spec.md, agent_chat/go_v5_api_surface_2026-03-25.md
package multifrost

import (
	"context"
	"encoding/json"
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
)

// routerLockData is the JSON schema for the bootstrap lock file.
// See docs/spec.md (Lock File Format section) for field definitions and
// evaluation rules.
type routerLockData struct {
	Format        string  `json:"format"`
	Pid           int     `json:"pid"`
	RouterPid     *int    `json:"router_pid"`
	Port          int     `json:"port"`
	CreatedAtUnix float64 `json:"created_at_unix"`
	ExpiresAtUnix float64 `json:"expires_at_unix"`
	Status        string  `json:"status"`
	Language      string  `json:"language"`
}

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

	lock, err := acquireRouterLock(ctx, port)
	if err != nil {
		return &BootstrapError{Origin: ErrorOriginBootstrap, Err: err}
	}

	// skip_spawn: another process has a live router (router_pid alive).
	// Do not hold the lock; just poll for reachability.
	if lock == nil {
		deadline := time.Now().Add(routerBootstrapTimeout)
		for time.Now().Before(deadline) {
			if routerReachable(ctx, endpoint) {
				return nil
			}
			select {
			case <-ctx.Done():
				return &BootstrapError{Origin: ErrorOriginBootstrap, Err: ctx.Err()}
			case <-time.After(routerBootstrapPollDelay):
			}
		}
		return &BootstrapError{
			Origin: ErrorOriginBootstrap,
			Err:    fmt.Errorf("router did not become reachable at %s within %s", endpoint, routerBootstrapTimeout),
		}
	}

	defer lock.release()

	// Double-check reachability now that we hold the lock.
	if routerReachable(ctx, endpoint) {
		lock.updateFields(map[string]any{"status": "ready"})
		return nil
	}

	// Spawn the router process and capture its PID.
	routerPid, err := spawnRouterProcess(port)
	if err != nil {
		lock.updateFields(map[string]any{"status": "failed"})
		return &BootstrapError{Origin: ErrorOriginBootstrap, Err: err}
	}
	lock.updateFields(map[string]any{"router_pid": &routerPid})

	// Poll for router readiness.
	deadline := time.Now().Add(routerBootstrapTimeout)
	for {
		if routerReachable(ctx, endpoint) {
			lock.updateFields(map[string]any{"status": "ready"})
			return nil
		}
		if time.Now().After(deadline) {
			lock.updateFields(map[string]any{"status": "failed"})
			return &BootstrapError{
				Origin: ErrorOriginBootstrap,
				Err:    fmt.Errorf("router did not become reachable at %s within %s", endpoint, routerBootstrapTimeout),
			}
		}

		select {
		case <-ctx.Done():
			lock.updateFields(map[string]any{"status": "failed"})
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

// routerLockGuard holds the lock file path and the in-memory lock data.
// Callers should call updateFields before release() to persist status changes.
type routerLockGuard struct {
	path string
	data *routerLockData
}

func (g *routerLockGuard) release() {
	if g == nil || g.path == "" {
		return
	}
	_ = os.Remove(g.path)
}

// updateFields updates in-memory lock data fields and atomically rewrites the
// lock file. This is a best-effort operation; errors are silently discarded
// since the lock file is advisory state.
func (g *routerLockGuard) updateFields(fields map[string]any) {
	if g == nil || g.path == "" || g.data == nil {
		return
	}
	if v, ok := fields["status"]; ok {
		if s, ok := v.(string); ok {
			g.data.Status = s
		}
	}
	if v, ok := fields["router_pid"]; ok {
		if p, ok := v.(*int); ok {
			g.data.RouterPid = p
		}
	}
	bytes, err := json.Marshal(g.data)
	if err != nil {
		return
	}
	_ = os.WriteFile(g.path, bytes, 0o600)
}

// acquireRouterLock attempts to acquire the shared router startup lock by
// atomically creating the lock file. Returns:
//   - (*routerLockGuard, nil) on successful lock acquisition
//   - (nil, nil) when "skip_spawn" is detected — another process has a
//     live router process, so the caller should poll for reachability
//     rather than spawning a new one
//   - (nil, error) on timeout or filesystem error
func acquireRouterLock(ctx context.Context, port int) (*routerLockGuard, error) {
	lockPath := defaultRouterLockPath()
	if err := os.MkdirAll(filepath.Dir(lockPath), 0o755); err != nil {
		return nil, err
	}

	deadline := time.Now().Add(routerBootstrapTimeout)
	for {
		file, err := os.OpenFile(lockPath, os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0o600)
		if err == nil {
			now := float64(time.Now().UnixNano()) / 1e9
			data := routerLockData{
				Format:        "v1",
				Pid:           os.Getpid(),
				Port:          port,
				CreatedAtUnix: now,
				ExpiresAtUnix: now + routerBootstrapTimeout.Seconds(),
				Status:        "starting",
				Language:      "go",
			}
			bytes, _ := json.Marshal(data)
			_, _ = file.Write(bytes)
			_ = file.Close()
			return &routerLockGuard{path: lockPath, data: &data}, nil
		}
		if !errors.Is(err, os.ErrExist) {
			return nil, err
		}

		switch evaluateExistingLock(lockPath) {
		case "reclaim":
			_ = os.Remove(lockPath)
			continue
		case "skip_spawn":
			return nil, nil
		case "wait":
			// fall through to timeout/retry
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

// evaluateExistingLock reads and evaluates an existing lock file against the
// six stale-detection rules defined in docs/spec.md (Lock File Format section).
//
// Returns one of:
//   - "reclaim"    — lock is stale/dead, caller should remove and retry
//   - "skip_spawn" — lock holder has a live router process, caller should
//     attempt to connect before spawning
//   - "wait"       — lock is valid, caller should retry after a delay
func evaluateExistingLock(path string) string {
	data, err := os.ReadFile(path)
	if err != nil {
		return "reclaim"
	}

	var lock routerLockData
	if err := json.Unmarshal(data, &lock); err != nil {
		return "reclaim"
	}

	// Rule 1: format must be "v1"
	if lock.Format != "v1" {
		return "reclaim"
	}

	// Rule 2: self-declared expiry
	now := float64(time.Now().UnixNano()) / 1e9
	if now > lock.ExpiresAtUnix {
		return "reclaim"
	}

	// Rule 3: lock holder (pid) must be alive
	if !isProcessAlive(lock.Pid) {
		return "reclaim"
	}

	// Rule 4: status "failed" means holder gave up
	if lock.Status == "failed" {
		return "reclaim"
	}

	// Rule 5: router_pid set and alive → skip spawn, try connecting
	if lock.RouterPid != nil && isProcessAlive(*lock.RouterPid) {
		return "skip_spawn"
	}

	// Rule 6: otherwise, lock is valid — wait and retry
	return "wait"
}

// spawnRouterProcess starts the router binary in the background.
// Returns the router process PID on success.
func spawnRouterProcess(port int) (int, error) {
	binary := os.Getenv(RouterBinEnv)
	if binary == "" {
		binary = "multifrost-router"
	}

	logPath := defaultRouterLogPath()
	if err := os.MkdirAll(filepath.Dir(logPath), 0o755); err != nil {
		return 0, err
	}

	logFile, err := os.OpenFile(logPath, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		return 0, err
	}
	defer logFile.Close()

	cmd := exec.Command(binary)
	cmd.Env = append(os.Environ(), fmt.Sprintf("%s=%d", RouterPortEnv, port))
	cmd.Stdout = logFile
	cmd.Stderr = logFile

	if err := cmd.Start(); err != nil {
		return 0, err
	}
	return cmd.Process.Pid, nil
}
