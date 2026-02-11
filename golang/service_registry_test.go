package multifrost

import (
	"os"
	"runtime"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestServiceRegistry_Paths(t *testing.T) {
	t.Run("registry path is correct", func(t *testing.T) {
		path := getRegistryPath()
		assert.NotEmpty(t, path)
		assert.Contains(t, path, RegistryFileName)
		assert.Contains(t, path, ".multifrost")
	})

	t.Run("lock path is correct", func(t *testing.T) {
		path := getLockPath()
		assert.NotEmpty(t, path)
		assert.Contains(t, path, LockFileName)
		assert.Contains(t, path, ".multifrost")
	})

	t.Run("get registry path public function", func(t *testing.T) {
		path := GetRegistryPath()
		assert.NotEmpty(t, path)
		assert.Contains(t, path, RegistryFileName)
	})
}

func TestServiceRegistry_Lifecycle(t *testing.T) {
	// Clear registry before test
	err := ClearRegistry()
	require.NoError(t, err)

	t.Run("register and discover service", func(t *testing.T) {
		serviceID := "test-service-" + time.Now().Format("20060102150405")
		port := 55555

		// Register service
		err := Register(serviceID, port)
		require.NoError(t, err)

		// Discover service
		discoveredPort, err := Discover(serviceID, 2*time.Second)
		require.NoError(t, err)
		assert.Equal(t, port, discoveredPort)

		// Cleanup
		err = Unregister(serviceID)
		assert.NoError(t, err)
	})

	t.Run("discover non-existent service", func(t *testing.T) {
		_, err := Discover("non-existent-service", 500*time.Millisecond)
		assert.Error(t, err)
		assert.Equal(t, ErrServiceNotFound, err)
	})

	t.Run("unregister service", func(t *testing.T) {
		serviceID := "test-unregister-" + time.Now().Format("20060102150405")
		port := 55554

		// Register
		err := Register(serviceID, port)
		require.NoError(t, err)

		// Unregister
		err = Unregister(serviceID)
		require.NoError(t, err)

		// Should not be discoverable anymore
		_, err = Discover(serviceID, 200*time.Millisecond)
		assert.Error(t, err)
	})
}

func TestServiceRegistry_List(t *testing.T) {
	// Clear registry
	err := ClearRegistry()
	require.NoError(t, err)

	t.Run("list services", func(t *testing.T) {
		// Initially empty
		services, err := ListServices()
		require.NoError(t, err)
		assert.Empty(t, services)

		// Register a service
		serviceID := "test-list-" + time.Now().Format("20060102150405")
		err = Register(serviceID, 55553)
		require.NoError(t, err)

		// List should contain the service
		services, err = ListServices()
		require.NoError(t, err)
		assert.Contains(t, services, serviceID)

		// Cleanup
		err = Unregister(serviceID)
		assert.NoError(t, err)
	})
}

func TestServiceRegistry_Clear(t *testing.T) {
	t.Run("clear registry", func(t *testing.T) {
		// Register a service
		serviceID := "test-clear-" + time.Now().Format("20060102150405")
		err := Register(serviceID, 55552)
		require.NoError(t, err)

		// Clear
		err = ClearRegistry()
		require.NoError(t, err)

		// Should be empty
		services, err := ListServices()
		require.NoError(t, err)
		assert.Empty(t, services)
	})
}

func TestServiceRegistry_Validation(t *testing.T) {
	t.Run("validate valid port", func(t *testing.T) {
		err := ValidatePort(8080)
		assert.NoError(t, err)
	})

	t.Run("validate port too low", func(t *testing.T) {
		err := ValidatePort(1023)
		assert.Error(t, err)
	})

	t.Run("validate port too high", func(t *testing.T) {
		err := ValidatePort(65536)
		assert.Error(t, err)
	})

	t.Run("validate reserved port", func(t *testing.T) {
		err := ValidatePort(80)
		assert.Error(t, err)
	})
}

func TestServiceRegistry_Locking(t *testing.T) {
	t.Run("acquire and release lock", func(t *testing.T) {
		lock, err := acquireLock()
		require.NoError(t, err)
		assert.NotNil(t, lock)

		err = releaseLock(lock)
		assert.NoError(t, err)
	})

	t.Run("release nil lock", func(t *testing.T) {
		err := releaseLock(nil)
		assert.NoError(t, err)
	})
}

func TestServiceRegistry_CrossPlatform(t *testing.T) {
	t.Run("home directory detection", func(t *testing.T) {
		var homeDir string
		if runtime.GOOS == "windows" {
			homeDir = os.Getenv("USERPROFILE")
		} else {
			homeDir = os.Getenv("HOME")
		}

		if homeDir != "" {
			path := getRegistryPath()
			assert.Contains(t, path, homeDir)
		}
	})

	t.Run("fallback to temp directory", func(t *testing.T) {
		// Save original env
		var homeEnv, homeVal string
		if runtime.GOOS == "windows" {
			homeEnv = "USERPROFILE"
		} else {
			homeEnv = "HOME"
		}
		homeVal = os.Getenv(homeEnv)

		// Clear home env
		os.Unsetenv(homeEnv)

		// Get path (should fallback to temp)
		path := getRegistryPath()
		assert.Contains(t, path, os.TempDir())

		// Restore env
		if homeVal != "" {
			os.Setenv(homeEnv, homeVal)
		}
	})
}

func TestServiceRegistry_ServiceInfo(t *testing.T) {
	t.Run("service info structure", func(t *testing.T) {
		info := ServiceInfo{
			Port:      8080,
			PID:       os.Getpid(),
			StartTime: time.Now(),
		}

		assert.Equal(t, 8080, info.Port)
		assert.Equal(t, os.Getpid(), info.PID)
		assert.False(t, info.StartTime.IsZero())
	})
}

func TestServiceRegistry_Concurrent(t *testing.T) {
	t.Run("concurrent register and unregister", func(t *testing.T) {
		// Clear registry
		err := ClearRegistry()
		require.NoError(t, err)

		// Register multiple services concurrently
		done := make(chan bool, 3)
		for i := 0; i < 3; i++ {
			go func(idx int) {
				serviceID := "concurrent-test-" + string(rune('0'+idx))
				port := 55000 + idx
				err := Register(serviceID, port)
				if err == nil {
					// Small delay to ensure registration is complete
					time.Sleep(50 * time.Millisecond)
					Unregister(serviceID)
				}
				done <- true
			}(i)
		}

		// Wait for all goroutines
		for i := 0; i < 3; i++ {
			<-done
		}

		// Registry should be clean
		services, err := ListServices()
		require.NoError(t, err)
		for name := range services {
			assert.NotContains(t, name, "concurrent-test-")
		}
	})
}
