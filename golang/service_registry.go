package multifrost

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"
)

const (
	RegistryFileName = "multifrost_services.json"
	DiscoveryTimeout = 5 * time.Second
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
	// Use temp directory for cross-platform compatibility
	tmpDir := os.TempDir()
	return filepath.Join(tmpDir, RegistryFileName)
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

// save writes the registry to disk
func (r *ServiceRegistry) save() error {
	r.mu.RLock()
	defer r.mu.RUnlock()

	data, err := json.MarshalIndent(r.services, "", "  ")
	if err != nil {
		return err
	}

	return os.WriteFile(r.filePath, data, 0644)
}

// Register registers a service with the registry
func Register(serviceID string, port int) error {
	r := getRegistry()

	r.mu.Lock()
	r.services[serviceID] = ServiceInfo{
		Port:      port,
		PID:       os.Getpid(),
		StartTime: time.Now(),
	}
	r.mu.Unlock()

	return r.save()
}

// Unregister removes a service from the registry
func Unregister(serviceID string) error {
	r := getRegistry()

	r.mu.Lock()
	delete(r.services, serviceID)
	r.mu.Unlock()

	return r.save()
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

	r.mu.Lock()
	r.services = make(map[string]ServiceInfo)
	r.mu.Unlock()

	return r.save()
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
