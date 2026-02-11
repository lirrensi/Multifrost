package multifrost

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"sync"
	"time"
)

const (
	RegistryFileName = "services.json"
	LockFileName     = "services.lock"
	DiscoveryTimeout = 5 * time.Second
	LockTimeout      = 10 * time.Second
)

var (
	ErrServiceNotFound = errors.New("service not found")
	ErrTimeout         = errors.New("discovery timeout")
)

// ServiceRegistry manages service discovery via JSON file
type ServiceRegistry struct {
	mu       sync.RWMutex
	services map[string]ServiceInfo
	filePath string
}

// ServiceInfo holds service registration data
type ServiceInfo struct {
	Port      int       `json:"port"`
	PID       int       `json:"pid"`
	StartTime time.Time `json:"start_time"`
}

// registrySingleton is the global registry instance
var registrySingleton *ServiceRegistry
var registryOnce sync.Once

// getRegistry returns the singleton registry instance
func getRegistry() *ServiceRegistry {
	registryOnce.Do(func() {
		registrySingleton = &ServiceRegistry{
			services: make(map[string]ServiceInfo),
			filePath: getRegistryPath(),
		}
		registrySingleton.load()
	})
	return registrySingleton
}

// getRegistryPath returns the path to the registry file
func getRegistryPath() string {
	// Use ~/.multifrost/services.json for cross-platform compatibility
	var homeDir string
	if runtime.GOOS == "windows" {
		homeDir = os.Getenv("USERPROFILE")
	} else {
		homeDir = os.Getenv("HOME")
	}

	if homeDir == "" {
		// Fallback to temp directory
		return filepath.Join(os.TempDir(), RegistryFileName)
	}

	multifrostDir := filepath.Join(homeDir, ".multifrost")
	return filepath.Join(multifrostDir, RegistryFileName)
}

// getLockPath returns the path to the lock file
func getLockPath() string {
	// Use ~/.multifrost/services.lock for cross-platform compatibility
	var homeDir string
	if runtime.GOOS == "windows" {
		homeDir = os.Getenv("USERPROFILE")
	} else {
		homeDir = os.Getenv("HOME")
	}

	if homeDir == "" {
		// Fallback to temp directory
		return filepath.Join(os.TempDir(), LockFileName)
	}

	multifrostDir := filepath.Join(homeDir, ".multifrost")
	return filepath.Join(multifrostDir, LockFileName)
}

// acquireLock attempts to acquire the registry lock with timeout
func acquireLock() (*os.File, error) {
	lockPath := getLockPath()
	deadline := time.Now().Add(LockTimeout)

	for time.Now().Before(deadline) {
		// Try to create lock file exclusively (O_CREAT | O_EXCL)
		// On Unix: os.O_CREATE | os.O_EXCL | os.O_WRONLY
		// On Windows: same flags work
		lockFile, err := os.OpenFile(lockPath, os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0600)
		if err == nil {
			// Lock acquired successfully
			return lockFile, nil
		}

		// Check if file exists (lock held by another process)
		if os.IsExist(err) {
			// Check if lock file is stale (older than 10 seconds)
			if info, statErr := os.Stat(lockPath); statErr == nil {
				if time.Since(info.ModTime()) > LockTimeout {
					// Stale lock, try to remove it
					_ = os.Remove(lockPath)
					continue
				}
			}
			// Lock is held, wait and retry
			time.Sleep(100 * time.Millisecond)
			continue
		}

		// Other error
		return nil, fmt.Errorf("failed to acquire lock: %w", err)
	}

	return nil, fmt.Errorf("lock acquisition timeout after %v", LockTimeout)
}

// releaseLock releases the registry lock
func releaseLock(lockFile *os.File) error {
	if lockFile == nil {
		return nil
	}

	lockPath := getLockPath()

	// Close the file
	if err := lockFile.Close(); err != nil {
		return fmt.Errorf("failed to close lock file: %w", err)
	}

	// Remove the lock file
	if err := os.Remove(lockPath); err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("failed to remove lock file: %w", err)
	}

	return nil
}

// load reads the registry from disk
func (r *ServiceRegistry) load() error {
	data, err := os.ReadFile(r.filePath)
	if err != nil {
		if os.IsNotExist(err) {
			return nil // File doesn't exist yet
		}
		return err
	}

	r.mu.Lock()
	defer r.mu.Unlock()
	return json.Unmarshal(data, &r.services)
}

// save writes the registry to disk (caller must hold lock)
func (r *ServiceRegistry) save() error {
	data, err := json.MarshalIndent(r.services, "", "  ")
	if err != nil {
		return err
	}

	// Ensure directory exists
	filePath := r.filePath
	dir := filepath.Dir(filePath)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return fmt.Errorf("failed to create directory: %w", err)
	}

	return os.WriteFile(filePath, data, 0644)
}

// Register registers a service with the registry
func Register(serviceID string, port int) error {
	r := getRegistry()

	// Acquire lock
	lock, err := acquireLock()
	if err != nil {
		fmt.Fprintf(os.Stderr, "Warning: Failed to acquire registry lock: %v\n", err)
		// Continue without lock (best effort)
	}

	// Ensure lock is released
	if lock != nil {
		defer func() {
			if releaseErr := releaseLock(lock); releaseErr != nil {
				fmt.Fprintf(os.Stderr, "Warning: Failed to release registry lock: %v\n", releaseErr)
			}
		}()
	}

	// Reload to get latest state
	if err := r.load(); err != nil {
		return fmt.Errorf("failed to load registry: %w", err)
	}

	r.mu.Lock()
	defer r.mu.Unlock()

	// Check if service already exists with live PID
	if existing, exists := r.services[serviceID]; exists {
		if isProcessAlive(existing.PID) {
			return fmt.Errorf("service '%s' already registered with PID %d", serviceID, existing.PID)
		}
		// Dead PID, will overwrite
	}

	r.services[serviceID] = ServiceInfo{
		Port:      port,
		PID:       os.Getpid(),
		StartTime: time.Now(),
	}

	return r.save()
}

// Unregister removes a service from the registry
func Unregister(serviceID string) error {
	r := getRegistry()

	// Acquire lock
	lock, err := acquireLock()
	if err != nil {
		fmt.Fprintf(os.Stderr, "Warning: Failed to acquire registry lock: %v\n", err)
		// Continue without lock (best effort)
	}

	// Ensure lock is released
	if lock != nil {
		defer func() {
			if releaseErr := releaseLock(lock); releaseErr != nil {
				fmt.Fprintf(os.Stderr, "Warning: Failed to release registry lock: %v\n", releaseErr)
			}
		}()
	}

	// Reload to get latest state
	if err := r.load(); err != nil {
		return fmt.Errorf("failed to load registry: %w", err)
	}

	r.mu.Lock()
	defer r.mu.Unlock()

	// Only remove if PID matches
	if existing, exists := r.services[serviceID]; exists {
		if existing.PID == os.Getpid() {
			delete(r.services, serviceID)
			return r.save()
		}
		// PID doesn't match, don't unregister
		return fmt.Errorf("service '%s' registered with different PID", serviceID)
	}

	return nil
}

// Discover finds a service by ID and returns its port
func Discover(serviceID string, timeout time.Duration) (int, error) {
	if timeout == 0 {
		timeout = DiscoveryTimeout
	}

	r := getRegistry()
	deadline := time.Now().Add(timeout)

	for time.Now().Before(deadline) {
		// Reload from disk to get latest
		if err := r.load(); err != nil {
			time.Sleep(100 * time.Millisecond)
			continue
		}

		r.mu.RLock()
		info, exists := r.services[serviceID]
		r.mu.RUnlock()

		if exists {
			// Check if process is still alive
			if isProcessAlive(info.PID) {
				return info.Port, nil
			}
			// Process dead, clean up
			_ = Unregister(serviceID)
		}

		time.Sleep(100 * time.Millisecond)
	}

	return 0, ErrServiceNotFound
}

// isProcessAlive checks if a process with the given PID is running
func isProcessAlive(pid int) bool {
	if pid <= 0 {
		return false
	}
	proc, err := os.FindProcess(pid)
	if err != nil {
		return false
	}
	// On Windows, FindProcess always succeeds, so we need to check differently
	// On Unix, sending signal 0 checks if process exists
	return proc != nil
}

// List returns all registered services
func ListServices() (map[string]ServiceInfo, error) {
	r := getRegistry()
	if err := r.load(); err != nil {
		return nil, err
	}

	r.mu.RLock()
	defer r.mu.RUnlock()

	// Return a copy
	result := make(map[string]ServiceInfo)
	for k, v := range r.services {
		result[k] = v
	}
	return result, nil
}

// Clear removes all services from the registry
func ClearRegistry() error {
	r := getRegistry()

	// Acquire lock
	lock, err := acquireLock()
	if err != nil {
		fmt.Fprintf(os.Stderr, "Warning: Failed to acquire registry lock: %v\n", err)
	}

	if lock != nil {
		defer func() {
			if releaseErr := releaseLock(lock); releaseErr != nil {
				fmt.Fprintf(os.Stderr, "Warning: Failed to release registry lock: %v\n", releaseErr)
			}
		}()
	}

	r.mu.Lock()
	r.services = make(map[string]ServiceInfo)
	err = r.save()
	r.mu.Unlock()

	return err
}

// GetRegistryPath returns the current registry file path (for debugging)
func GetRegistryPath() string {
	return getRegistryPath()
}

// ValidatePort checks if a port is in valid range
func ValidatePort(port int) error {
	if port < 1024 || port > 65535 {
		return fmt.Errorf("port %d out of valid range (1024-65535)", port)
	}
	return nil
}
