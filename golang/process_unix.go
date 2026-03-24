//go:build !windows

// FILE: golang/process_unix.go
// PURPOSE: Provide Unix process-group management for spawned service processes.
// OWNS: Spawn-time process-group setup and termination helpers.
// EXPORTS: configureServiceProcess, terminateServiceProcess.
// DOCS: docs/spec.md, agent_chat/go_v5_api_surface_2026-03-25.md
package multifrost

import (
	"os/exec"
	"syscall"
)

func configureServiceProcess(cmd *exec.Cmd) {
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
}

func terminateServiceProcess(cmd *exec.Cmd) error {
	if cmd == nil || cmd.Process == nil {
		return nil
	}

	if err := syscall.Kill(-cmd.Process.Pid, syscall.SIGKILL); err != nil && err != syscall.ESRCH {
		return err
	}
	return nil
}
