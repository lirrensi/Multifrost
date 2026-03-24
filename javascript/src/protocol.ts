/**
 * FILE: javascript/src/protocol.ts
 * PURPOSE: Define the canonical v5 protocol constants, peer classes, wire types, and envelope validation helpers.
 * OWNS: Shared router protocol constants and msgpack-serializable message shapes for Node v5.
 * EXPORTS: PROTOCOL_KEY, PROTOCOL_VERSION, DEFAULT_ROUTER_PORT, ROUTER_PORT_ENV, ROUTER_LOG_SUFFIX, ROUTER_LOCK_SUFFIX, service, caller, Envelope, RegisterBody, RegisterAckBody, QueryBody, QueryExistsResponseBody, QueryGetResponseBody, CallBody, ResponseBody, ErrorBody, validateEnvelopeFields
 * DOCS: docs/spec.md, docs/msgpack_interop.md
 */

import { ProtocolError } from "./errors.js";

export const PROTOCOL_KEY = "multifrost_ipc_v5";
export const PROTOCOL_VERSION = 5 as const;
export const DEFAULT_ROUTER_PORT = 9981;
export const ROUTER_PORT_ENV = "MULTIFROST_ROUTER_PORT";
export const ROUTER_LOG_SUFFIX = ".multifrost/router.log";
export const ROUTER_LOCK_SUFFIX = ".multifrost/router.lock";

export const service = "service" as const;
export const caller = "caller" as const;

export type PeerClass = typeof service | typeof caller;

export const KIND_REGISTER = "register" as const;
export const KIND_QUERY = "query" as const;
export const KIND_CALL = "call" as const;
export const KIND_RESPONSE = "response" as const;
export const KIND_ERROR = "error" as const;
export const KIND_HEARTBEAT = "heartbeat" as const;
export const KIND_DISCONNECT = "disconnect" as const;

export const QUERY_PEER_EXISTS = "peer.exists" as const;
export const QUERY_PEER_GET = "peer.get" as const;

export type EnvelopeKind =
    | typeof KIND_REGISTER
    | typeof KIND_QUERY
    | typeof KIND_CALL
    | typeof KIND_RESPONSE
    | typeof KIND_ERROR
    | typeof KIND_HEARTBEAT
    | typeof KIND_DISCONNECT;

export interface Envelope {
    v: number;
    kind: EnvelopeKind;
    msg_id: string;
    from: string;
    to: string;
    ts: number;
}

export interface RegisterBody {
    peer_id: string;
    class: PeerClass;
}

export interface RegisterAckBody {
    accepted: boolean;
    reason?: string | null;
}

export type QueryBody =
    | {
          query: typeof QUERY_PEER_EXISTS;
          peer_id: string;
      }
    | {
          query: typeof QUERY_PEER_GET;
          peer_id: string;
      };

export interface QueryExistsResponseBody {
    peer_id: string;
    exists: boolean;
    class?: PeerClass | null;
    connected: boolean;
}

export interface QueryGetResponseBody {
    peer_id: string;
    exists: boolean;
    class?: PeerClass | null;
    connected: boolean;
}

export interface CallBody {
    function: string;
    args: unknown[];
    namespace?: string;
}

export interface ResponseBody {
    result: unknown;
}

export interface ErrorBody {
    code: string;
    message: string;
    kind: string;
    stack?: string | null;
    details?: unknown;
}

type UnknownRecord = Record<string, unknown>;

function isPlainObject(value: unknown): value is UnknownRecord {
    if (value === null || typeof value !== "object") return false;
    const prototype = Object.getPrototypeOf(value);
    return prototype === Object.prototype || prototype === null;
}

function isString(value: unknown): value is string {
    return typeof value === "string";
}

function isNumber(value: unknown): value is number {
    return typeof value === "number" && Number.isFinite(value);
}

function isPeerClass(value: unknown): value is PeerClass {
    return value === service || value === caller;
}

function isEnvelopeKind(value: unknown): value is EnvelopeKind {
    return (
        value === KIND_REGISTER ||
        value === KIND_QUERY ||
        value === KIND_CALL ||
        value === KIND_RESPONSE ||
        value === KIND_ERROR ||
        value === KIND_HEARTBEAT ||
        value === KIND_DISCONNECT
    );
}

function requireField<T>(record: UnknownRecord, key: string, predicate: (value: unknown) => value is T): T {
    const value = record[key];
    if (!predicate(value)) {
        throw new ProtocolError(`Invalid or missing envelope field: ${key}`);
    }
    return value;
}

export function validateEnvelopeFields(value: unknown): Envelope {
    if (!isPlainObject(value)) {
        throw new ProtocolError("Envelope must be a plain object");
    }

    const envelope: UnknownRecord = value;
    const v = requireField(envelope, "v", isNumber);
    if (v !== PROTOCOL_VERSION) {
        throw new ProtocolError(`Unsupported protocol version: ${v}`);
    }

    const kind = requireField(envelope, "kind", isEnvelopeKind);
    const msg_id = requireField(envelope, "msg_id", isString);
    const from = requireField(envelope, "from", isString);
    const to = requireField(envelope, "to", isString);
    const ts = requireField(envelope, "ts", isNumber);

    return { v, kind, msg_id, from, to, ts };
}

export function isPeerClassValue(value: unknown): value is PeerClass {
    return isPeerClass(value);
}
