//go:build windows

// FILE: golang/process_windows.go
// PURPOSE: Provide Windows-compatible no-op process helpers for spawned service processes.
// OWNS: Spawn-time process setup and termination helpers on Windows.
// EXPORTS: configureServiceProcess, terminateServiceProcess.
// DOCS: docs/spec.md, agent_chat/go_v5_api_surface_2026-03-25.md
package multifrost

import "os/exec"

func configureServiceProcess(cmd *exec.Cmd) {}

func terminateServiceProcess(cmd *exec.Cmd) error {
	if cmd == nil || cmd.Process == nil {
		return nil
	}
	return cmd.Process.Kill()
}
