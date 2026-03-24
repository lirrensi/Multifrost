/**
 * FILE: javascript/src/service.ts
 * PURPOSE: Expose the service-side v5 runtime surface for peer registration and method dispatch.
 * OWNS: ServiceContext, ServiceWorker, runService, and the service peer bootstrap flow.
 * EXPORTS: ServiceContext, ServiceContextOptions, ServiceWorker, runService
 * DOCS: docs/spec.md, agent_chat/node_v5_api_surface_2026-03-24.md
 */

import { randomUUID } from "crypto";
import * as fs from "fs/promises";
import * as path from "path";
import {
    decodeBody,
    encodeBody,
} from "./frame.js";
import {
    CallBody,
    ErrorBody,
    Envelope,
    KIND_CALL,
    KIND_DISCONNECT,
    KIND_ERROR,
    KIND_RESPONSE,
    ResponseBody,
    service,
} from "./protocol.js";
import {
    BootstrapFailureError,
    ProtocolError,
    RemoteCallError,
    TransportFailureError,
} from "./errors.js";
import {
    bootstrapRouter,
    RouterBootstrapOptions,
} from "./router_bootstrap.js";
import {
    RoutedFrame,
    TransportCallbacks,
    TransportConnectOptions,
    WebSocketTransport,
} from "./transport.js";

const DEFAULT_REQUEST_TIMEOUT_MS = 30_000;
const DEFAULT_BOOTSTRAP_TIMEOUT_MS = 10_000;

export interface ServiceContextOptions {
    peerId?: string;
    routerPort?: number;
    routerBin?: string;
    entrypointPath?: string;
    requestTimeoutMs?: number;
    bootstrapTimeoutMs?: number;
}

export class ServiceContext {
    readonly peerId?: string;
    readonly routerPort?: number;
    readonly routerBin?: string;
    readonly entrypointPath?: string;
    readonly requestTimeoutMs?: number;
    readonly bootstrapTimeoutMs?: number;

    constructor(options: ServiceContextOptions = {}) {
        this.peerId = options.peerId;
        this.routerPort = options.routerPort;
        this.routerBin = options.routerBin;
        this.entrypointPath = options.entrypointPath;
        this.requestTimeoutMs = options.requestTimeoutMs;
        this.bootstrapTimeoutMs = options.bootstrapTimeoutMs;
    }
}

type MethodCandidate = (...args: unknown[]) => unknown | Promise<unknown>;

function nowSeconds(): number {
    return Date.now() / 1000;
}

async function canonicalizePathLike(value: string): Promise<string> {
    const resolved = path.resolve(value);
    try {
        return await fs.realpath(resolved);
    } catch {
        return resolved;
    }
}

async function resolveServicePeerId(context: ServiceContext): Promise<string> {
    if (context.peerId && context.peerId.trim()) {
        return context.peerId;
    }

    const fromEnv = process.env.MULTIFROST_ENTRYPOINT_PATH;
    if (fromEnv && fromEnv.trim()) {
        return canonicalizePathLike(fromEnv);
    }

    const fromArgv = process.argv[1] ?? process.cwd();
    return canonicalizePathLike(fromArgv);
}

function toErrorBody(message: string, kind: string, code: string, details?: unknown): ErrorBody {
    return {
        code,
        message,
        kind,
        stack: undefined,
        details,
    };
}

function isCallableMethodName(name: string): boolean {
    if (
        name === "constructor" ||
        name === "dispatch" ||
        name === "callableMethods" ||
        name === "then" ||
        name.startsWith("_")
    ) {
        return false;
    }

    return true;
}

export abstract class ServiceWorker {
    callableMethods(): string[] {
        const methods = new Set<string>();
        let prototype = Object.getPrototypeOf(this);

        while (prototype && prototype !== ServiceWorker.prototype) {
            for (const name of Object.getOwnPropertyNames(prototype)) {
                if (!isCallableMethodName(name)) {
                    continue;
                }

                const descriptor = Object.getOwnPropertyDescriptor(prototype, name);
                if (!descriptor || typeof descriptor.value !== "function") {
                    continue;
                }

                methods.add(name);
            }

            prototype = Object.getPrototypeOf(prototype);
        }

        return [...methods].sort();
    }

    async dispatch(functionName: string, args: unknown[]): Promise<unknown> {
        if (!isCallableMethodName(functionName) || !this.callableMethods().includes(functionName)) {
            throw new RemoteCallError(`function not found: ${functionName}`);
        }

        const candidate = (this as Record<string, unknown>)[functionName];
        if (typeof candidate !== "function") {
            throw new RemoteCallError(`function not callable: ${functionName}`);
        }

        return await Promise.resolve((candidate as MethodCandidate).apply(this, args));
    }
}

export async function runService(
    worker: ServiceWorker,
    context: ServiceContext = new ServiceContext()
): Promise<void> {
    const peerId = await resolveServicePeerId(context);
    const bootstrapOptions: RouterBootstrapOptions = {
        port: context.routerPort,
        routerBin: context.routerBin,
        readinessTimeoutMs: context.bootstrapTimeoutMs ?? DEFAULT_BOOTSTRAP_TIMEOUT_MS,
    };

    const bootstrap = await bootstrapRouter(bootstrapOptions);
    let transport: WebSocketTransport | undefined;

    const callbacks: TransportCallbacks = {
        call: async frame => {
            if (!transport || transport.isClosed) {
                return;
            }

            await handleCallFrame(worker, transport, peerId, frame);
        },
        close: error => {
            if (error) {
                return;
            }
        },
    };

    const transportOptions: TransportConnectOptions = {
        endpoint: bootstrap.endpoint,
        peerId,
        peerClass: service,
        callbacks,
        registerTimeoutMs: context.requestTimeoutMs ?? DEFAULT_REQUEST_TIMEOUT_MS,
    };

    transport = await WebSocketTransport.connect(transportOptions);

    try {
        await transport.waitForClose();
    } finally {
        await transport.close();
    }
}

async function handleCallFrame(
    worker: ServiceWorker,
    transport: WebSocketTransport,
    peerId: string,
    frame: RoutedFrame
): Promise<void> {
    let call: CallBody;
    try {
        call = decodeBody<CallBody>(frame.bodyBytes);
    } catch (error) {
        await sendError(
            transport,
            peerId,
            frame.envelope.from,
            frame.envelope.msg_id,
            "malformed_call",
            "application",
            error instanceof Error ? error.message : String(error)
        );
        return;
    }

    if (call.namespace && call.namespace !== "default") {
        await sendError(
            transport,
            peerId,
            frame.envelope.from,
            frame.envelope.msg_id,
            "invalid_namespace",
            "application",
            `unsupported namespace: ${call.namespace}`
        );
        return;
    }

    try {
        const result = await worker.dispatch(call.function, call.args);
        await sendResponse(transport, peerId, frame.envelope.from, frame.envelope.msg_id, result);
    } catch (error) {
        await sendError(
            transport,
            peerId,
            frame.envelope.from,
            frame.envelope.msg_id,
            "application_error",
            "application",
            error instanceof Error ? error.message : String(error),
            error
        );
    }
}

async function sendResponse(
    transport: WebSocketTransport,
    fromPeerId: string,
    toPeerId: string,
    msgId: string,
    result: unknown
): Promise<void> {
    const envelope: Envelope = {
        v: 5,
        kind: KIND_RESPONSE,
        msg_id: msgId,
        from: fromPeerId,
        to: toPeerId,
        ts: nowSeconds(),
    };

    const body: ResponseBody = { result: result === undefined ? null : result };
    await transport.send(envelope, encodeBody(body));
}

async function sendError(
    transport: WebSocketTransport,
    fromPeerId: string,
    toPeerId: string,
    msgId: string,
    code: string,
    kind: string,
    message: string,
    details?: unknown
): Promise<void> {
    const envelope: Envelope = {
        v: 5,
        kind: KIND_ERROR,
        msg_id: msgId,
        from: fromPeerId,
        to: toPeerId,
        ts: nowSeconds(),
    };

    await transport.send(envelope, encodeBody(toErrorBody(message, kind, code, details)));
}
