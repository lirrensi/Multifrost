//go:build windows

// FILE: golang/process_windows.go
// PURPOSE: Provide Windows-compatible process helpers for spawned service processes.
// OWNS: Spawn-time process setup and termination helpers on Windows, process liveness checks.
// EXPORTS: configureServiceProcess, terminateServiceProcess, isProcessAlive.
// DOCS: docs/spec.md, agent_chat/go_v5_api_surface_2026-03-25.md
package multifrost

import (
	"fmt"
	"os/exec"
	"strings"
)

func configureServiceProcess(cmd *exec.Cmd) {}

func terminateServiceProcess(cmd *exec.Cmd) error {
	if cmd == nil || cmd.Process == nil {
		return nil
	}
	return cmd.Process.Kill()
}

// isProcessAlive checks whether the given PID corresponds to a live OS process.
// On Windows this uses tasklist to query the process table, since os.FindProcess
// always succeeds regardless of whether the process exists.
func isProcessAlive(pid int) bool {
	if pid <= 0 {
		return false
	}
	cmd := exec.Command("tasklist", "/FI", fmt.Sprintf("PID eq %d", pid), "/FO", "csv", "/NH")
	out, _ := cmd.Output()
	return strings.Contains(string(out), fmt.Sprintf("\"%d\"", pid))
}
