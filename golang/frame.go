// FILE: golang/frame.go
// PURPOSE: Encode and decode the v5 binary frame layout without touching application body bytes.
// OWNS: Frame codec, msgpack helpers, binary frame validation.
// EXPORTS: Frame, EncodeFrame, DecodeFrame, EncodeMsgpack, DecodeMsgpack, EncodeEnvelope, DecodeEnvelope.
// DOCS: docs/spec.md, agent_chat/go_v5_api_surface_2026-03-25.md
package multifrost

import (
	"encoding/binary"
	"fmt"

	"github.com/vmihailenco/msgpack/v5"
)

func EncodeMsgpack(value any) ([]byte, error) {
	return msgpack.Marshal(value)
}

func DecodeMsgpack(data []byte, target any) error {
	return msgpack.Unmarshal(data, target)
}

func EncodeEnvelope(envelope Envelope) ([]byte, error) {
	if err := ValidateEnvelope(envelope); err != nil {
		return nil, err
	}
	return EncodeMsgpack(envelope)
}

func DecodeEnvelope(data []byte) (Envelope, error) {
	var envelope Envelope
	if err := DecodeMsgpack(data, &envelope); err != nil {
		return Envelope{}, fmt.Errorf("decode envelope: %w", err)
	}
	if err := ValidateEnvelope(envelope); err != nil {
		return Envelope{}, err
	}
	return envelope, nil
}

func EncodeFrame(envelope Envelope, bodyBytes []byte) ([]byte, error) {
	envelopeBytes, err := EncodeEnvelope(envelope)
	if err != nil {
		return nil, err
	}

	out := make([]byte, 4+len(envelopeBytes)+len(bodyBytes))
	binary.BigEndian.PutUint32(out[:4], uint32(len(envelopeBytes)))
	copy(out[4:], envelopeBytes)
	copy(out[4+len(envelopeBytes):], bodyBytes)
	return out, nil
}

func DecodeFrame(data []byte) (Frame, error) {
	if len(data) < 4 {
		return Frame{}, fmt.Errorf("frame too short for envelope length")
	}

	envelopeLen := int(binary.BigEndian.Uint32(data[:4]))
	if envelopeLen < 0 {
		return Frame{}, fmt.Errorf("invalid envelope length")
	}
	if len(data) < 4+envelopeLen {
		return Frame{}, fmt.Errorf("frame shorter than declared envelope length")
	}

	envelope, err := DecodeEnvelope(data[4 : 4+envelopeLen])
	if err != nil {
		return Frame{}, err
	}

	bodyBytes := append([]byte(nil), data[4+envelopeLen:]...)
	return Frame{
		Envelope:  envelope,
		BodyBytes: bodyBytes,
	}, nil
}
