// FILE: golang/transport.go
// PURPOSE: Own the live WebSocket peer transport used by caller handles and service runners.
// OWNS: Dial/register/read/write/pending-request routing for v5 peers.
// EXPORTS: none.
// DOCS: docs/spec.md, agent_chat/go_v5_api_surface_2026-03-25.md
package multifrost

import (
	"context"
	"errors"
	"fmt"
	"sync"
	"sync/atomic"

	"github.com/coder/websocket"
)

type transportReply struct {
	frame Frame
	err   error
}

type peerTransport struct {
	conn       *websocket.Conn
	peerID     string
	class      PeerClass
	writeMu    sync.Mutex
	pendingMu  sync.Mutex
	pending    map[string]chan transportReply
	dispatch   func(Frame)
	closed     chan struct{}
	closeErr   atomic.Value
	closedOnce sync.Once
}

func dialPeerTransport(
	ctx context.Context,
	endpoint string,
	peerID string,
	class PeerClass,
	dispatch func(Frame),
) (*peerTransport, error) {
	if ctx == nil {
		ctx = context.Background()
	}

	conn, _, err := websocket.Dial(ctx, endpoint, &websocket.DialOptions{})
	if err != nil {
		return nil, &TransportError{Origin: ErrorOriginTransport, Err: err}
	}

	transport := &peerTransport{
		conn:     conn,
		peerID:   peerID,
		class:    class,
		pending:  make(map[string]chan transportReply),
		dispatch: dispatch,
		closed:   make(chan struct{}),
	}

	if err := transport.register(ctx); err != nil {
		_ = conn.Close(websocket.StatusInternalError, "")
		return nil, err
	}

	go transport.readLoop()
	return transport, nil
}

func (t *peerTransport) register(ctx context.Context) error {
	body := RegisterBody{
		PeerID: t.peerID,
		Class:  t.class,
	}
	bodyBytes, err := EncodeMsgpack(body)
	if err != nil {
		return &TransportError{Origin: ErrorOriginTransport, Err: err}
	}

	envelope := NewEnvelope(KindRegister, t.peerID, RouterPeerID)
	frameBytes, err := EncodeFrame(envelope, bodyBytes)
	if err != nil {
		return err
	}

	if err := t.writeBinary(ctx, frameBytes); err != nil {
		return err
	}

	msgType, bytes, err := t.conn.Read(ctx)
	if err != nil {
		return t.wrapReadError(err)
	}
	if msgType != websocket.MessageBinary {
		return &RouterError{
			Origin:  ErrorOriginProtocol,
			Code:    "non_binary_message",
			Message: "router must send binary websocket frames",
		}
	}

	frame, err := DecodeFrame(bytes)
	if err != nil {
		return &RouterError{
			Origin:  ErrorOriginProtocol,
			Code:    "malformed_frame",
			Message: err.Error(),
		}
	}

	switch frame.Envelope.Kind {
	case KindResponse:
		var ack RegisterAckBody
		if err := DecodeMsgpack(frame.BodyBytes, &ack); err != nil {
			return &RegistrationError{
				Origin: ErrorOriginRegistration,
				Err:    err,
			}
		}
		if !ack.Accepted {
			reason := "router rejected registration"
			if ack.Reason != nil && *ack.Reason != "" {
				reason = *ack.Reason
			}
			return &RegistrationError{
				Origin: ErrorOriginRegistration,
				Reason: reason,
			}
		}
		return nil
	case KindError:
		var body ErrorBody
		if err := DecodeMsgpack(frame.BodyBytes, &body); err != nil {
			return &RegistrationError{
				Origin: ErrorOriginRegistration,
				Err:    err,
			}
		}
		return &RegistrationError{
			Origin: ErrorOriginRegistration,
			Reason: body.Message,
		}
	default:
		return &RegistrationError{
			Origin: ErrorOriginRegistration,
			Reason: fmt.Sprintf("unexpected register reply kind: %s", frame.Envelope.Kind),
		}
	}
}

func (t *peerTransport) request(ctx context.Context, envelope Envelope, bodyBytes []byte) (Frame, error) {
	if t.isClosed() {
		return Frame{}, t.transportClosedError()
	}

	replyCh := make(chan transportReply, 1)
	t.pendingMu.Lock()
	t.pending[envelope.MsgID] = replyCh
	t.pendingMu.Unlock()

	frameBytes, err := EncodeFrame(envelope, bodyBytes)
	if err != nil {
		t.removePending(envelope.MsgID)
		return Frame{}, err
	}

	if err := t.writeBinary(ctx, frameBytes); err != nil {
		t.removePending(envelope.MsgID)
		return Frame{}, err
	}

	select {
	case reply := <-replyCh:
		return reply.frame, reply.err
	case <-ctx.Done():
		t.removePending(envelope.MsgID)
		return Frame{}, ctx.Err()
	case <-t.closed:
		t.removePending(envelope.MsgID)
		return Frame{}, t.transportClosedError()
	}
}

func (t *peerTransport) send(ctx context.Context, envelope Envelope, bodyBytes []byte) error {
	frameBytes, err := EncodeFrame(envelope, bodyBytes)
	if err != nil {
		return err
	}
	return t.writeBinary(ctx, frameBytes)
}

func (t *peerTransport) call(ctx context.Context, targetPeerID string, function string, args []any) (Frame, error) {
	bodyBytes, err := EncodeMsgpack(CallBody{Function: function, Args: args})
	if err != nil {
		return Frame{}, &TransportError{Origin: ErrorOriginTransport, Err: err}
	}
	envelope := NewEnvelope(KindCall, t.peerID, targetPeerID)
	return t.request(ctx, envelope, bodyBytes)
}

func (t *peerTransport) queryExists(ctx context.Context, peerID string) (QueryExistsResponseBody, error) {
	bodyBytes, err := EncodeMsgpack(QueryBody{Query: QueryPeerExists, PeerID: peerID})
	if err != nil {
		return QueryExistsResponseBody{}, &TransportError{Origin: ErrorOriginTransport, Err: err}
	}

	frame, err := t.request(ctx, NewEnvelope(KindQuery, t.peerID, RouterPeerID), bodyBytes)
	if err != nil {
		return QueryExistsResponseBody{}, err
	}
	if frame.Envelope.Kind == KindError {
		body, decodeErr := decodeWireErrorBody(frame.BodyBytes)
		if decodeErr != nil {
			return QueryExistsResponseBody{}, decodeErr
		}
		return QueryExistsResponseBody{}, ErrorFromWire(body)
	}

	var response QueryExistsResponseBody
	if err := DecodeMsgpack(frame.BodyBytes, &response); err != nil {
		return QueryExistsResponseBody{}, &RouterError{
			Origin:  ErrorOriginProtocol,
			Code:    "malformed_query_response",
			Message: err.Error(),
		}
	}
	return response, nil
}

func (t *peerTransport) queryGet(ctx context.Context, peerID string) (QueryGetResponseBody, error) {
	bodyBytes, err := EncodeMsgpack(QueryBody{Query: QueryPeerGet, PeerID: peerID})
	if err != nil {
		return QueryGetResponseBody{}, &TransportError{Origin: ErrorOriginTransport, Err: err}
	}

	frame, err := t.request(ctx, NewEnvelope(KindQuery, t.peerID, RouterPeerID), bodyBytes)
	if err != nil {
		return QueryGetResponseBody{}, err
	}
	if frame.Envelope.Kind == KindError {
		body, decodeErr := decodeWireErrorBody(frame.BodyBytes)
		if decodeErr != nil {
			return QueryGetResponseBody{}, decodeErr
		}
		return QueryGetResponseBody{}, ErrorFromWire(body)
	}

	var response QueryGetResponseBody
	if err := DecodeMsgpack(frame.BodyBytes, &response); err != nil {
		return QueryGetResponseBody{}, &RouterError{
			Origin:  ErrorOriginProtocol,
			Code:    "malformed_query_response",
			Message: err.Error(),
		}
	}
	return response, nil
}

func (t *peerTransport) disconnect(ctx context.Context) error {
	bodyBytes := []byte{}
	envelope := NewEnvelope(KindDisconnect, t.peerID, RouterPeerID)
	if err := t.send(ctx, envelope, bodyBytes); err != nil {
		return err
	}
	return nil
}

func (t *peerTransport) readLoop() {
	defer t.closeWithError(nil)

	for {
		msgType, bytes, err := t.conn.Read(context.Background())
		if err != nil {
			t.closeWithError(t.wrapReadError(err))
			return
		}
		if msgType != websocket.MessageBinary {
			t.closeWithError(&RouterError{
				Origin:  ErrorOriginProtocol,
				Code:    "non_binary_message",
				Message: "router must send binary websocket frames",
			})
			return
		}

		frame, err := DecodeFrame(bytes)
		if err != nil {
			t.closeWithError(&RouterError{
				Origin:  ErrorOriginProtocol,
				Code:    "malformed_frame",
				Message: err.Error(),
			})
			return
		}

		if t.deliverPending(frame) {
			continue
		}

		if t.dispatch != nil {
			frameCopy := frame
			go t.dispatch(frameCopy)
		}

		if frame.Envelope.Kind == KindDisconnect {
			t.closeWithError(&TransportError{
				Origin: ErrorOriginTransport,
				Err:    errors.New("peer disconnected"),
			})
			return
		}
	}
}

func (t *peerTransport) deliverPending(frame Frame) bool {
	t.pendingMu.Lock()
	replyCh, ok := t.pending[frame.Envelope.MsgID]
	if ok {
		delete(t.pending, frame.Envelope.MsgID)
	}
	t.pendingMu.Unlock()
	if !ok {
		return false
	}

	var replyErr error
	if frame.Envelope.Kind == KindError {
		body, decodeErr := decodeWireErrorBody(frame.BodyBytes)
		if decodeErr != nil {
			replyErr = decodeErr
		} else {
			replyErr = ErrorFromWire(body)
		}
	}
	replyCh <- transportReply{frame: frame, err: replyErr}
	close(replyCh)
	return true
}

func (t *peerTransport) removePending(msgID string) {
	t.pendingMu.Lock()
	delete(t.pending, msgID)
	t.pendingMu.Unlock()
}

func (t *peerTransport) writeBinary(ctx context.Context, bytes []byte) error {
	if t.isClosed() {
		return t.transportClosedError()
	}

	t.writeMu.Lock()
	defer t.writeMu.Unlock()

	if t.isClosed() {
		return t.transportClosedError()
	}

	if err := t.conn.Write(ctx, websocket.MessageBinary, bytes); err != nil {
		wrapped := t.wrapReadError(err)
		t.closeWithError(wrapped)
		return wrapped
	}
	return nil
}

func (t *peerTransport) closeWithError(err error) {
	t.closedOnce.Do(func() {
		if err != nil {
			t.closeErr.Store(err)
		}
		close(t.closed)
		t.cancelPending(err)
		_ = t.conn.Close(websocket.StatusNormalClosure, "")
	})
}

func (t *peerTransport) cancelPending(err error) {
	if err == nil {
		err = t.transportClosedError()
	}

	t.pendingMu.Lock()
	defer t.pendingMu.Unlock()
	for msgID, replyCh := range t.pending {
		select {
		case replyCh <- transportReply{err: err}:
		default:
		}
		close(replyCh)
		delete(t.pending, msgID)
	}
}

func (t *peerTransport) transportClosedError() error {
	if value := t.closeErr.Load(); value != nil {
		if err, ok := value.(error); ok && err != nil {
			return err
		}
	}
	return &TransportError{Origin: ErrorOriginTransport, Err: errors.New("transport closed")}
}

func (t *peerTransport) wrapReadError(err error) error {
	if err == nil {
		return nil
	}
	return &TransportError{Origin: ErrorOriginTransport, Err: err}
}

func (t *peerTransport) isClosed() bool {
	select {
	case <-t.closed:
		return true
	default:
		return false
	}
}

func decodeWireErrorBody(bodyBytes []byte) (ErrorBody, error) {
	var body ErrorBody
	if err := DecodeMsgpack(bodyBytes, &body); err != nil {
		return ErrorBody{}, &RouterError{
			Origin:  ErrorOriginProtocol,
			Code:    "malformed_error_body",
			Message: err.Error(),
		}
	}
	return body, nil
}
