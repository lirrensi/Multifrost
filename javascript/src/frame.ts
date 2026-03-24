/**
 * FILE: javascript/src/frame.ts
 * PURPOSE: Encode and decode the canonical v5 websocket frame layout without mutating routed body bytes.
 * OWNS: Frame length prefixing, msgpack envelope/body encoding, and malformed-frame rejection.
 * EXPORTS: encodeFrame, decodeFrame, encodeMsgpackValue, decodeMsgpackValue, encodeEnvelope, decodeEnvelope, encodeBody, decodeBody, FrameParts
 * DOCS: docs/spec.md, docs/msgpack_interop.md
 */

import { Packr, Unpackr } from "msgpackr";
import {
    Envelope,
    validateEnvelopeFields,
} from "./protocol.js";
import { ProtocolError } from "./errors.js";

const SIGNED_INT64_MIN = -(1n << 63n);
const SIGNED_INT64_MAX = (1n << 63n) - 1n;

const MSGPACK_PACKER = new Packr({ useRecords: false });
const MSGPACK_UNPACKER = new Unpackr({
    useRecords: false,
    int64AsType: "auto" as unknown as "string" | "number" | "bigint",
});

export interface FrameParts {
    envelopeBytes: Uint8Array;
    bodyBytes: Uint8Array;
}

type PlainRecord = Record<string, unknown>;

function isPlainObject(value: unknown): value is PlainRecord {
    if (value === null || typeof value !== "object") return false;
    const prototype = Object.getPrototypeOf(value);
    return prototype === Object.prototype || prototype === null;
}

function sanitizeMsgpackValue<T>(value: T): T {
    if (typeof value === "number") {
        if (!Number.isFinite(value)) {
            return null as T;
        }

        if (Number.isInteger(value) && !Number.isSafeInteger(value)) {
            throw new RangeError(
                "Unsafe integer numbers must be encoded as bigint or an explicit application string."
            );
        }

        return value;
    }

    if (typeof value === "bigint") {
        if (value < SIGNED_INT64_MIN || value > SIGNED_INT64_MAX) {
            throw new RangeError(
                "BigInt values outside the signed int64 range must be encoded explicitly."
            );
        }

        return value as T;
    }

    if (Array.isArray(value)) {
        return value.map(item => sanitizeMsgpackValue(item)) as T;
    }

    if (isPlainObject(value)) {
        const result: PlainRecord = {};
        for (const [key, entry] of Object.entries(value)) {
            result[key] = sanitizeMsgpackValue(entry);
        }
        return result as T;
    }

    return value;
}

function toUint8Array(value: Uint8Array): Uint8Array {
    return value instanceof Uint8Array ? value : new Uint8Array(value);
}

function readEnvelopeLength(frame: Uint8Array): number {
    if (frame.byteLength < 4) {
        throw new ProtocolError("Frame too short to contain an envelope length");
    }

    const view = new DataView(frame.buffer, frame.byteOffset, frame.byteLength);
    const envelopeLength = view.getUint32(0, false);
    if (envelopeLength <= 0) {
        throw new ProtocolError("Frame declared an empty envelope");
    }
    return envelopeLength;
}

export function encodeMsgpackValue<T>(value: T): Uint8Array {
    return MSGPACK_PACKER.pack(sanitizeMsgpackValue(value));
}

export function decodeMsgpackValue<T = unknown>(bytes: Uint8Array): T {
    return MSGPACK_UNPACKER.unpack(bytes) as T;
}

export function encodeEnvelope(envelope: Envelope): Uint8Array {
    return encodeMsgpackValue(validateEnvelopeFields(envelope));
}

export function decodeEnvelope(bytes: Uint8Array): Envelope {
    return validateEnvelopeFields(decodeMsgpackValue<unknown>(bytes));
}

export function encodeBody<T>(body: T): Uint8Array {
    return encodeMsgpackValue(body);
}

export function decodeBody<T = unknown>(bytes: Uint8Array): T {
    return decodeMsgpackValue<T>(bytes);
}

export function encodeFrame(envelope: Envelope, bodyBytes: Uint8Array): Uint8Array {
    const envelopeBytes = encodeEnvelope(envelope);
    const body = toUint8Array(bodyBytes);
    const frame = Buffer.allocUnsafe(4 + envelopeBytes.byteLength + body.byteLength);

    frame.writeUInt32BE(envelopeBytes.byteLength, 0);
    frame.set(envelopeBytes, 4);
    frame.set(body, 4 + envelopeBytes.byteLength);

    return frame;
}

export function decodeFrame(buffer: Uint8Array): FrameParts {
    const frame = toUint8Array(buffer);
    const envelopeLength = readEnvelopeLength(frame);
    if (frame.byteLength < 4 + envelopeLength) {
        throw new ProtocolError("Frame is shorter than its declared envelope length");
    }

    const envelopeBytes = frame.subarray(4, 4 + envelopeLength);
    const bodyBytes = frame.subarray(4 + envelopeLength);
    return { envelopeBytes, bodyBytes };
}
