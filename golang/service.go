// FILE: golang/service.go
// PURPOSE: Run a v5 service peer with explicit dispatch through ServiceWorker.HandleCall.
// OWNS: ServiceContext, ServiceWorker, RunService, service-side routing and response emission.
// EXPORTS: ServiceContext, ServiceWorker, RunService.
// DOCS: docs/spec.md, agent_chat/go_v5_api_surface_2026-03-25.md
package multifrost

import (
	"context"
	"fmt"
	"os"
)

type ServiceContext struct {
	PeerID         string
	RouterPort     int
	EntrypointPath string
}

type ServiceWorker interface {
	HandleCall(ctx context.Context, function string, args []any) (any, error)
}

func RunService(ctx context.Context, serviceWorker ServiceWorker, serviceContext ServiceContext) error {
	if ctx == nil {
		ctx = context.Background()
	}

	if serviceWorker == nil {
		return &BootstrapError{Origin: ErrorOriginBootstrap, Err: fmt.Errorf("service worker is nil")}
	}

	if serviceContext.EntrypointPath != "" && os.Getenv(EntrypointPathEnv) == "" {
		if canonicalPath, err := canonicalEntrypointPath(serviceContext.EntrypointPath); err == nil {
			_ = os.Setenv(EntrypointPathEnv, canonicalPath)
		}
	}

	peerID, err := resolveServicePeerID(serviceContext)
	if err != nil {
		return &BootstrapError{Origin: ErrorOriginBootstrap, Err: err}
	}

	port := serviceContext.RouterPort
	if port <= 0 {
		port = routerPortFromEnv()
	}

	if err := ensureRouter(ctx, port); err != nil {
		return err
	}

	var transport *peerTransport
	dispatch := func(frame Frame) {
		if transport == nil {
			return
		}
		if err := handleServiceFrame(ctx, transport, serviceWorker, peerID, frame); err != nil {
			transport.closeWithError(err)
		}
	}

	transport, err = dialPeerTransport(ctx, routerEndpoint(port), peerID, PeerClassService, dispatch)
	if err != nil {
		return err
	}

	select {
	case <-ctx.Done():
		transport.closeWithError(ctx.Err())
		return ctx.Err()
	case <-transport.closed:
		if transport.closeErr.Load() != nil {
			return transport.transportClosedError()
		}
		return nil
	}
}

func resolveServicePeerID(serviceContext ServiceContext) (string, error) {
	if serviceContext.PeerID != "" {
		return serviceContext.PeerID, nil
	}

	if entrypoint := os.Getenv(EntrypointPathEnv); entrypoint != "" {
		return canonicalEntrypointPath(entrypoint)
	}

	executable, err := os.Executable()
	if err != nil {
		return "", err
	}
	return canonicalEntrypointPath(executable)
}

func handleServiceFrame(
	ctx context.Context,
	transport *peerTransport,
	serviceWorker ServiceWorker,
	peerID string,
	frame Frame,
) error {
	switch frame.Envelope.Kind {
	case KindCall:
		return handleServiceCall(ctx, transport, serviceWorker, peerID, frame)
	case KindDisconnect:
		transport.closeWithError(nil)
		return nil
	case KindHeartbeat:
		return nil
	case KindQuery, KindResponse, KindError:
		return nil
	default:
		return &RouterError{
			Origin:  ErrorOriginProtocol,
			Code:    "unsupported_kind",
			Message: fmt.Sprintf("unsupported frame kind: %s", frame.Envelope.Kind),
		}
	}
}

func handleServiceCall(
	ctx context.Context,
	transport *peerTransport,
	serviceWorker ServiceWorker,
	peerID string,
	frame Frame,
) error {
	var call CallBody
	if err := DecodeMsgpack(frame.BodyBytes, &call); err != nil {
		return sendServiceError(ctx, transport, peerID, frame.Envelope, "malformed_call_body", err.Error(), ErrorOriginProtocol)
	}

	result, err := serviceWorker.HandleCall(ctx, call.Function, call.Args)
	if err != nil {
		return sendServiceError(ctx, transport, peerID, frame.Envelope, "remote_call_error", err.Error(), ErrorOriginApplication)
	}

	bodyBytes, err := EncodeMsgpack(ResponseBody{Result: result})
	if err != nil {
		return sendServiceError(ctx, transport, peerID, frame.Envelope, "malformed_response_body", err.Error(), ErrorOriginProtocol)
	}

	envelope := NewEnvelope(KindResponse, peerID, frame.Envelope.From)
	envelope.MsgID = frame.Envelope.MsgID
	envelope.TS = frame.Envelope.TS
	return transport.send(ctx, envelope, bodyBytes)
}

func sendServiceError(
	ctx context.Context,
	transport *peerTransport,
	peerID string,
	request Envelope,
	code string,
	message string,
	kind ErrorOrigin,
) error {
	bodyBytes, err := EncodeMsgpack(ErrorBody{
		Code:    code,
		Message: message,
		Kind:    string(kind),
	})
	if err != nil {
		return err
	}

	envelope := NewEnvelope(KindError, peerID, request.From)
	envelope.MsgID = request.MsgID
	envelope.TS = request.TS
	return transport.send(ctx, envelope, bodyBytes)
}
