package multifrost

import (
	"net"
)

// findFreePort finds an available port for ZeroMQ communication
func findFreePort() int {
	addr, err := net.ResolveTCPAddr("tcp", "localhost:0")
	if err != nil {
		return 5555
	}

	l, err := net.ListenTCP("tcp", addr)
	if err != nil {
		return 5555
	}
	defer l.Close()

	return l.Addr().(*net.TCPAddr).Port
}
