// FILE: golang/protocol.go
// PURPOSE: Define the canonical v5 wire shapes and protocol constants for the Go binding.
// OWNS: Envelope, peer classes, router-owned body structs, protocol constants, envelope validation.
// EXPORTS: ProtocolKey, ProtocolVersion, RouterPeerID, Kind*, Query*, PeerClass, Envelope, RegisterBody, RegisterAckBody, QueryBody, QueryExistsResponseBody, QueryGetResponseBody, CallBody, ResponseBody, ErrorBody, ValidateEnvelope.
// DOCS: docs/spec.md, agent_chat/go_v5_api_surface_2026-03-25.md
package multifrost

import (
	"fmt"
	"math"
	"time"

	"github.com/google/uuid"
)

const ProtocolKey = "multifrost_ipc_v5"
const ProtocolVersion = uint32(5)
const RouterPeerID = "router"

const DefaultRouterPort = 9981
const RouterPortEnv = "MULTIFROST_ROUTER_PORT"
const RouterLogPathSuffix = ".multifrost/router.log"
const RouterLockPathSuffix = ".multifrost/router.lock"
const RouterBinEnv = "MULTIFROST_ROUTER_BIN"
const EntrypointPathEnv = "MULTIFROST_ENTRYPOINT_PATH"

const (
	KindRegister   = "register"
	KindQuery      = "query"
	KindCall       = "call"
	KindResponse   = "response"
	KindError      = "error"
	KindHeartbeat  = "heartbeat"
	KindDisconnect = "disconnect"
)

const (
	QueryPeerExists = "peer.exists"
	QueryPeerGet    = "peer.get"
)

type PeerClass string

const (
	PeerClassService PeerClass = "service"
	PeerClassCaller  PeerClass = "caller"
)

type Envelope struct {
	V     uint32  `msgpack:"v"`
	Kind  string  `msgpack:"kind"`
	MsgID string  `msgpack:"msg_id"`
	From  string  `msgpack:"from"`
	To    string  `msgpack:"to"`
	TS    float64 `msgpack:"ts"`
}

type RegisterBody struct {
	PeerID string    `msgpack:"peer_id"`
	Class  PeerClass `msgpack:"class"`
}

type RegisterAckBody struct {
	Accepted bool    `msgpack:"accepted"`
	Reason   *string `msgpack:"reason,omitempty"`
}

type QueryBody struct {
	Query  string `msgpack:"query"`
	PeerID string `msgpack:"peer_id"`
}

type QueryExistsResponseBody struct {
	PeerID    string     `msgpack:"peer_id"`
	Exists    bool       `msgpack:"exists"`
	Class     *PeerClass `msgpack:"class,omitempty"`
	Connected bool       `msgpack:"connected"`
}

type QueryGetResponseBody struct {
	PeerID    string     `msgpack:"peer_id"`
	Exists    bool       `msgpack:"exists"`
	Class     *PeerClass `msgpack:"class,omitempty"`
	Connected bool       `msgpack:"connected"`
}

type CallBody struct {
	Function string `msgpack:"function"`
	Args     []any  `msgpack:"args"`
}

type ResponseBody struct {
	Result any `msgpack:"result"`
}

type ErrorBody struct {
	Code    string  `msgpack:"code"`
	Message string  `msgpack:"message"`
	Kind    string  `msgpack:"kind"`
	Stack   *string `msgpack:"stack,omitempty"`
	Details any     `msgpack:"details,omitempty"`
}

type Frame struct {
	Envelope  Envelope
	BodyBytes []byte
}

func nowTS() float64 {
	return float64(time.Now().UnixNano()) / 1e9
}

func newMsgID() string {
	return uuid.NewString()
}

func ValidateEnvelope(envelope Envelope) error {
	if envelope.V != ProtocolVersion {
		return fmt.Errorf("invalid protocol version: %d", envelope.V)
	}
	if envelope.Kind == "" {
		return fmt.Errorf("missing envelope kind")
	}
	if envelope.MsgID == "" {
		return fmt.Errorf("missing envelope msg_id")
	}
	if envelope.From == "" {
		return fmt.Errorf("missing envelope from")
	}
	if envelope.To == "" {
		return fmt.Errorf("missing envelope to")
	}
	if math.IsNaN(envelope.TS) || math.IsInf(envelope.TS, 0) || envelope.TS <= 0 {
		return fmt.Errorf("invalid envelope ts")
	}
	return nil
}

func NewEnvelope(kind, from, to string) Envelope {
	return Envelope{
		V:     ProtocolVersion,
		Kind:  kind,
		MsgID: newMsgID(),
		From:  from,
		To:    to,
		TS:    nowTS(),
	}
}
