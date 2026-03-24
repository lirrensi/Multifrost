package multifrost

import (
	"bytes"
	"testing"
)

func TestFrameEncodeDecodePreservesBodyBytes(t *testing.T) {
	envelope := Envelope{
		V:     ProtocolVersion,
		Kind:  KindCall,
		MsgID: "msg-1",
		From:  "caller",
		To:    "service",
		TS:    1.2345,
	}
	body := []byte{0x82, 0xa3, 'f', 'o', 'o', 0x01}

	encoded, err := EncodeFrame(envelope, body)
	if err != nil {
		t.Fatalf("encode frame: %v", err)
	}

	decoded, err := DecodeFrame(encoded)
	if err != nil {
		t.Fatalf("decode frame: %v", err)
	}

	if decoded.Envelope != envelope {
		t.Fatalf("unexpected envelope: %#v", decoded.Envelope)
	}
	if !bytes.Equal(decoded.BodyBytes, body) {
		t.Fatalf("body bytes changed: got %v want %v", decoded.BodyBytes, body)
	}
}

func TestEnvelopeValidationRejectsMissingFields(t *testing.T) {
	envelope := Envelope{
		V:     ProtocolVersion,
		Kind:  KindCall,
		MsgID: "",
		From:  "caller",
		To:    "service",
		TS:    1.0,
	}

	if err := ValidateEnvelope(envelope); err == nil {
		t.Fatal("expected validation failure for missing msg_id")
	}
}
