package multifrost

import (
	"context"
	"os"
	"testing"
)

func TestRouterBootstrapReleasesLock(t *testing.T) {
	env := sharedTestEnv(t)
	ensureRouterBinaryExists(t, env)

	if err := ensureRouter(context.Background(), env.routerPort); err != nil {
		t.Fatalf("ensure router: %v", err)
	}

	if _, err := os.Stat(defaultRouterLockPath()); !os.IsNotExist(err) {
		t.Fatalf("router lock file still present: %v", err)
	}
}
