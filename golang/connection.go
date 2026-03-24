// FILE: golang/connection.go
// PURPOSE: Own the caller-side v5 configuration and live handle lifecycle.
// OWNS: ConnectOptions, Connect, Connection, Handle, caller bootstrap/call/query flow.
// EXPORTS: ConnectOptions, Connect, Connection, Handle.
// DOCS: docs/spec.md, agent_chat/go_v5_api_surface_2026-03-25.md
package multifrost

import (
	"context"
	"fmt"
	"sync"
	"time"

	"github.com/google/uuid"
)

const defaultRequestTimeout = 30 * time.Second

type ConnectOptions struct {
	PeerID           string
	RouterPort       int
	RequestTimeout   time.Duration
	BootstrapTimeout time.Duration
	ValidateTarget   bool
}

type Connection struct {
	defaultTargetPeerID string
	options             ConnectOptions
}

type Handle struct {
	defaultTargetPeerID string
	options             ConnectOptions

	mu        sync.Mutex
	peerID    string
	transport *peerTransport
	started   bool
}

func Connect(defaultTargetPeerID string, options ...ConnectOptions) *Connection {
	cfg := normalizeConnectOptions(options...)
	return &Connection{
		defaultTargetPeerID: defaultTargetPeerID,
		options:             cfg,
	}
}

func (c *Connection) Handle() *Handle {
	return &Handle{
		defaultTargetPeerID: c.defaultTargetPeerID,
		options:             c.options,
	}
}

func (c *Connection) DefaultTargetPeerID() string {
	return c.defaultTargetPeerID
}

func (h *Handle) PeerID() string {
	h.mu.Lock()
	defer h.mu.Unlock()
	return h.peerID
}

func (h *Handle) DefaultTargetPeerID() string {
	return h.defaultTargetPeerID
}

func (h *Handle) Start(ctx context.Context) error {
	if ctx == nil {
		ctx = context.Background()
	}

	h.mu.Lock()
	if h.started {
		h.mu.Unlock()
		return nil
	}
	options := h.options
	target := h.defaultTargetPeerID
	h.mu.Unlock()

	port := options.RouterPort
	if port <= 0 {
		port = routerPortFromEnv()
	}

	startCtx := ctx
	var cancel context.CancelFunc
	if options.BootstrapTimeout > 0 {
		startCtx, cancel = contextWithTimeoutIfNeeded(ctx, options.BootstrapTimeout)
		defer cancel()
	}
	if err := ensureRouter(startCtx, port); err != nil {
		return err
	}

	peerID := options.PeerID
	if peerID == "" {
		peerID = uuid.NewString()
	}

	transport, err := dialPeerTransport(startCtx, routerEndpoint(port), peerID, PeerClassCaller, nil)
	if err != nil {
		return err
	}

	if options.ValidateTarget && target != "" {
		validateCtx := ctx
		var validateCancel context.CancelFunc
		if options.RequestTimeout > 0 {
			validateCtx, validateCancel = contextWithTimeoutIfNeeded(ctx, options.RequestTimeout)
			defer validateCancel()
		}
		exists, err := transport.queryExists(validateCtx, target)
		if err != nil {
			transport.closeWithError(nil)
			return err
		}
		if !exists.Exists {
			transport.closeWithError(nil)
			return &RouterError{
				Origin:  ErrorOriginRouter,
				Code:    "peer_not_found",
				Message: fmt.Sprintf("target peer %q not found", target),
			}
		}
	}

	h.mu.Lock()
	h.peerID = peerID
	h.transport = transport
	h.started = true
	h.mu.Unlock()
	return nil
}

func (h *Handle) Stop(ctx context.Context) error {
	if ctx == nil {
		ctx = context.Background()
	}

	h.mu.Lock()
	transport := h.transport
	if !h.started || transport == nil {
		h.mu.Unlock()
		return nil
	}
	h.started = false
	h.transport = nil
	h.mu.Unlock()

	if ctx == nil {
		ctx = context.Background()
	}

	_ = transport.disconnect(ctx)
	transport.closeWithError(nil)
	return nil
}

func (h *Handle) Call(ctx context.Context, function string, args ...any) (any, error) {
	if ctx == nil {
		ctx = context.Background()
	}

	h.mu.Lock()
	transport := h.transport
	target := h.defaultTargetPeerID
	h.mu.Unlock()

	if transport == nil {
		return nil, &TransportError{Origin: ErrorOriginTransport, Err: fmt.Errorf("handle not started")}
	}
	if target == "" {
		return nil, &RouterError{
			Origin:  ErrorOriginRouter,
			Code:    "missing_target",
			Message: "no default target peer configured",
		}
	}

	callCtx := ctx
	var cancel context.CancelFunc
	if optionsTimeout := h.options.RequestTimeout; optionsTimeout > 0 {
		callCtx, cancel = contextWithTimeoutIfNeeded(ctx, optionsTimeout)
		defer cancel()
	}

	frame, err := transport.call(callCtx, target, function, args)
	if err != nil {
		return nil, err
	}

	switch frame.Envelope.Kind {
	case KindResponse:
		var response ResponseBody
		if err := DecodeMsgpack(frame.BodyBytes, &response); err != nil {
			return nil, &RouterError{
				Origin:  ErrorOriginProtocol,
				Code:    "malformed_response_body",
				Message: err.Error(),
			}
		}
		return response.Result, nil
	case KindError:
		body, err := decodeWireErrorBody(frame.BodyBytes)
		if err != nil {
			return nil, err
		}
		return nil, ErrorFromWire(body)
	default:
		return nil, &RouterError{
			Origin:  ErrorOriginProtocol,
			Code:    "unexpected_response_kind",
			Message: fmt.Sprintf("unexpected frame kind: %s", frame.Envelope.Kind),
		}
	}
}

func (h *Handle) QueryPeerExists(ctx context.Context, peerID string) (bool, error) {
	if ctx == nil {
		ctx = context.Background()
	}

	h.mu.Lock()
	transport := h.transport
	h.mu.Unlock()

	if transport == nil {
		return false, &TransportError{Origin: ErrorOriginTransport, Err: fmt.Errorf("handle not started")}
	}

	queryCtx := ctx
	var cancel context.CancelFunc
	if h.options.RequestTimeout > 0 {
		queryCtx, cancel = contextWithTimeoutIfNeeded(ctx, h.options.RequestTimeout)
		defer cancel()
	}

	response, err := transport.queryExists(queryCtx, peerID)
	if err != nil {
		return false, err
	}
	return response.Exists, nil
}

func (h *Handle) QueryPeerGet(ctx context.Context, peerID string) (QueryGetResponseBody, error) {
	if ctx == nil {
		ctx = context.Background()
	}

	h.mu.Lock()
	transport := h.transport
	h.mu.Unlock()

	if transport == nil {
		return QueryGetResponseBody{}, &TransportError{Origin: ErrorOriginTransport, Err: fmt.Errorf("handle not started")}
	}

	queryCtx := ctx
	var cancel context.CancelFunc
	if h.options.RequestTimeout > 0 {
		queryCtx, cancel = contextWithTimeoutIfNeeded(ctx, h.options.RequestTimeout)
		defer cancel()
	}

	return transport.queryGet(queryCtx, peerID)
}

func normalizeConnectOptions(options ...ConnectOptions) ConnectOptions {
	if len(options) == 0 {
		return ConnectOptions{
			RequestTimeout:   defaultRequestTimeout,
			BootstrapTimeout: routerBootstrapTimeout,
		}
	}

	cfg := options[len(options)-1]
	if cfg.RequestTimeout <= 0 {
		cfg.RequestTimeout = defaultRequestTimeout
	}
	if cfg.BootstrapTimeout <= 0 {
		cfg.BootstrapTimeout = routerBootstrapTimeout
	}
	return cfg
}

func contextWithTimeoutIfNeeded(ctx context.Context, timeout time.Duration) (context.Context, context.CancelFunc) {
	if ctx == nil {
		ctx = context.Background()
	}
	if _, ok := ctx.Deadline(); ok {
		return ctx, func() {}
	}
	if timeout <= 0 {
		return ctx, func() {}
	}

	return context.WithTimeout(ctx, timeout)
}
