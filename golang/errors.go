// FILE: golang/errors.go
// PURPOSE: Define the typed error model used by the v5 caller, service, bootstrap, and transport layers.
// OWNS: Typed error families and wire-error translation.
// EXPORTS: ErrorOrigin, TransportError, BootstrapError, RegistrationError, RouterError, RemoteCallError, ErrorFromWire.
// DOCS: docs/spec.md, agent_chat/go_v5_api_surface_2026-03-25.md
package multifrost

import "fmt"

type ErrorOrigin string

const (
	ErrorOriginTransport    ErrorOrigin = "transport"
	ErrorOriginBootstrap    ErrorOrigin = "bootstrap"
	ErrorOriginRegistration ErrorOrigin = "registration"
	ErrorOriginRouter       ErrorOrigin = "router"
	ErrorOriginProtocol     ErrorOrigin = "protocol"
	ErrorOriginRemote       ErrorOrigin = "remote"
	ErrorOriginApplication  ErrorOrigin = "application"
)

type TransportError struct {
	Origin ErrorOrigin
	Err    error
}

func (e *TransportError) Error() string {
	if e == nil {
		return "<nil>"
	}
	return fmt.Sprintf("transport failure [%s]: %v", e.Origin, e.Err)
}

func (e *TransportError) Unwrap() error { return e.Err }

type BootstrapError struct {
	Origin ErrorOrigin
	Err    error
}

func (e *BootstrapError) Error() string {
	if e == nil {
		return "<nil>"
	}
	return fmt.Sprintf("bootstrap failure [%s]: %v", e.Origin, e.Err)
}

func (e *BootstrapError) Unwrap() error { return e.Err }

type RegistrationError struct {
	Origin ErrorOrigin
	Reason string
	Err    error
}

func (e *RegistrationError) Error() string {
	if e == nil {
		return "<nil>"
	}
	if e.Reason != "" {
		return fmt.Sprintf("registration failure [%s]: %s", e.Origin, e.Reason)
	}
	return fmt.Sprintf("registration failure [%s]: %v", e.Origin, e.Err)
}

func (e *RegistrationError) Unwrap() error { return e.Err }

type RouterError struct {
	Origin  ErrorOrigin
	Code    string
	Message string
	Err     error
}

func (e *RouterError) Error() string {
	if e == nil {
		return "<nil>"
	}
	if e.Code != "" {
		return fmt.Sprintf("router error [%s] %s: %s", e.Origin, e.Code, e.Message)
	}
	return fmt.Sprintf("router error [%s]: %s", e.Origin, e.Message)
}

func (e *RouterError) Unwrap() error { return e.Err }

type RemoteCallError struct {
	Origin   ErrorOrigin
	Function string
	Message  string
	Err      error
}

func (e *RemoteCallError) Error() string {
	if e == nil {
		return "<nil>"
	}
	if e.Function != "" {
		return fmt.Sprintf("remote call failure [%s] %s: %s", e.Origin, e.Function, e.Message)
	}
	return fmt.Sprintf("remote call failure [%s]: %s", e.Origin, e.Message)
}

func (e *RemoteCallError) Unwrap() error { return e.Err }

func ErrorFromWire(body ErrorBody) error {
	switch body.Kind {
	case string(ErrorOriginProtocol):
		return &RouterError{
			Origin:  ErrorOriginProtocol,
			Code:    body.Code,
			Message: body.Message,
		}
	case string(ErrorOriginRouter), "library":
		return &RouterError{
			Origin:  ErrorOriginRouter,
			Code:    body.Code,
			Message: body.Message,
		}
	case "application", string(ErrorOriginRemote):
		return &RemoteCallError{
			Origin:  ErrorOriginRemote,
			Message: body.Message,
		}
	default:
		return &RouterError{
			Origin:  ErrorOriginRouter,
			Code:    body.Code,
			Message: body.Message,
		}
	}
}
