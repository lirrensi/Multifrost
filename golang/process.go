// FILE: golang/process.go
// PURPOSE: Launch and manage spawned service processes without coupling them to caller state.
// OWNS: ServiceProcess, Spawn, process lifecycle helpers.
// EXPORTS: ServiceProcess, Spawn.
// DOCS: docs/spec.md, agent_chat/go_v5_api_surface_2026-03-25.md
package multifrost

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sync"
)

type ServiceProcess struct {
	cmd      *exec.Cmd
	waitDone chan struct{}
	waitErr  error
	mu       sync.Mutex
}

func Spawn(serviceEntrypoint string) (*ServiceProcess, error) {
	absEntrypoint, err := canonicalEntrypointPath(serviceEntrypoint)
	if err != nil {
		return nil, err
	}

	cmd := exec.Command("go", "run", absEntrypoint)
	configureServiceProcess(cmd)
	cmd.Env = append(os.Environ(), fmt.Sprintf("%s=%s", EntrypointPathEnv, absEntrypoint))
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	if err := cmd.Start(); err != nil {
		return nil, err
	}

	process := &ServiceProcess{
		cmd:      cmd,
		waitDone: make(chan struct{}),
	}
	go process.waitForExit()
	return process, nil
}

func (p *ServiceProcess) ID() int {
	if p == nil || p.cmd == nil || p.cmd.Process == nil {
		return 0
	}
	return p.cmd.Process.Pid
}

func (p *ServiceProcess) Stop(ctx context.Context) error {
	if p == nil || p.cmd == nil || p.cmd.Process == nil || p.waitDone == nil {
		return nil
	}

	if err := terminateServiceProcess(p.cmd); err != nil && !isProcessDoneError(err) {
		return err
	}

	if ctx == nil {
		ctx = context.Background()
	}

	select {
	case <-p.waitDone:
		return nil
	case <-ctx.Done():
		return ctx.Err()
	}
}

func (p *ServiceProcess) Wait() error {
	if p == nil || p.waitDone == nil {
		return nil
	}
	<-p.waitDone
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.waitErr
}

func (p *ServiceProcess) waitForExit() {
	if p == nil || p.cmd == nil {
		return
	}

	err := p.cmd.Wait()
	p.mu.Lock()
	p.waitErr = err
	p.mu.Unlock()
	close(p.waitDone)
}

func canonicalEntrypointPath(path string) (string, error) {
	if path == "" {
		return "", errors.New("service entrypoint path is required")
	}

	absPath, err := filepath.Abs(path)
	if err != nil {
		return "", err
	}

	if resolved, err := filepath.EvalSymlinks(absPath); err == nil {
		absPath = resolved
	}

	return absPath, nil
}

func isProcessDoneError(err error) bool {
	return errors.Is(err, os.ErrProcessDone)
}
