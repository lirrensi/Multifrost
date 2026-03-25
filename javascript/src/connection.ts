/**
 * FILE: javascript/src/connection.ts
 * PURPOSE: Expose the caller-side v5 connection descriptor and live handle with proxy-based remote calls.
 * OWNS: Connection, Handle, caller bootstrap flow, request correlation, and query/call convenience methods.
 * EXPORTS: ConnectOptions, connect, Connection, Handle
 * DOCS: docs/spec.md, agent_chat/node_v5_api_surface_2026-03-24.md
 */

import { randomUUID } from "crypto";
import {
    decodeBody,
    encodeBody,
} from "./frame.js";
import {
    caller,
    CallBody,
    ErrorBody,
    Envelope,
    KIND_CALL,
    KIND_DISCONNECT,
    KIND_ERROR,
    KIND_QUERY,
    KIND_RESPONSE,
    QueryBody,
    QueryExistsResponseBody,
    QueryGetResponseBody,
    ResponseBody,
    service,
} from "./protocol.js";
import {
    BootstrapFailureError,
    RemoteCallError,
    RouterError,
    TransportFailureError,
} from "./errors.js";
import {
    bootstrapRouter,
    resolveRouterEndpoint,
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

export interface ConnectOptions {
    callerPeerId?: string;
    routerPort?: number;
    routerBin?: string;
    requestTimeoutMs?: number;
    bootstrapTimeoutMs?: number;
    eagerTargetQuery?: false | "exists" | "get";
}

type PendingDecoder<T> = (frame: RoutedFrame) => T;

interface PendingRequest<T> {
    resolve: (value: unknown) => void;
    reject: (error: Error) => void;
    decoder: PendingDecoder<T>;
    timer: ReturnType<typeof setTimeout>;
}

export function connect(targetPeerId: string, options: ConnectOptions = {}): Connection {
    return new Connection(targetPeerId, options);
}

export class Connection {
    readonly targetPeerId: string;
    readonly options: ConnectOptions;

    constructor(targetPeerId: string, options: ConnectOptions = {}) {
        this.targetPeerId = targetPeerId;
        this.options = options;
    }

    handle(): Handle {
        return new Handle(this);
    }
}

export class Handle {
    public readonly call: Record<string, (...args: unknown[]) => Promise<unknown>>;
    private readonly connection: Connection;
    private transport?: WebSocketTransport;
    private callerPeerId?: string;
    private started = false;
    private readonly pendingRequests = new Map<string, PendingRequest<unknown>>();

    constructor(connection: Connection) {
        this.connection = connection;
        this.call = new Proxy({} as Record<string, (...args: unknown[]) => Promise<unknown>>, {
            get: (_target, property) => {
                if (property === "then" || typeof property !== "string") {
                    return undefined;
                }

                return async (...args: unknown[]) => this.callRemote(property, args);
            },
        });
    }

    async start(): Promise<void> {
        if (this.started && this.transport) {
            return;
        }

        const bootstrapOptions: RouterBootstrapOptions = {
            port: this.connection.options.routerPort,
            routerBin: this.connection.options.routerBin,
            readinessTimeoutMs: this.connection.options.bootstrapTimeoutMs ?? DEFAULT_BOOTSTRAP_TIMEOUT_MS,
        };
        const bootstrap = await bootstrapRouter(bootstrapOptions);
        this.callerPeerId = this.connection.options.callerPeerId ?? `caller-${randomUUID()}`;

        const callbacks: TransportCallbacks = {
            response: frame => this.resolvePending(frame),
            error: frame => this.rejectPending(frame),
            close: error => this.rejectAllPending(
                error ?? new TransportFailureError("caller transport closed")
            ),
        };

        const transportOptions: TransportConnectOptions = {
            endpoint: bootstrap.endpoint,
            peerId: this.callerPeerId,
            peerClass: caller,
            callbacks,
            registerTimeoutMs: this.connection.options.requestTimeoutMs ?? DEFAULT_REQUEST_TIMEOUT_MS,
        };

        try {
            this.transport = await WebSocketTransport.connect(transportOptions);
            this.started = true;

            if (this.connection.options.eagerTargetQuery) {
                await this.validateTargetPeer(this.connection.options.eagerTargetQuery);
            }
        } catch (error) {
            await this.stop();
            if (error instanceof Error) {
                throw error;
            }
            throw new BootstrapFailureError(String(error), { cause: error });
        }
    }

    async stop(): Promise<void> {
        const transport = this.transport;
        this.transport = undefined;
        this.started = false;

        if (!transport) {
            this.rejectAllPending(new TransportFailureError("caller handle stopped"));
            return;
        }

        try {
            if (!transport.isClosed) {
                const envelope = this.buildEnvelope(KIND_DISCONNECT, "router");
                await transport.send(envelope, new Uint8Array());
            }
        } catch {
            // Best-effort graceful disconnect only.
        } finally {
            await transport.close();
            this.rejectAllPending(new TransportFailureError("caller handle stopped"));
        }
    }

    async queryPeerExists(peerId: string): Promise<boolean> {
        const response = await this.sendRequest<QueryExistsResponseBody>(
            KIND_QUERY,
            "router",
            {
                query: "peer.exists",
                peer_id: peerId,
            } satisfies QueryBody,
            frame => decodeBody<QueryExistsResponseBody>(frame.bodyBytes)
        );
        return response.exists;
    }

    async queryPeerGet(peerId: string): Promise<QueryGetResponseBody> {
        return this.sendRequest<QueryGetResponseBody>(
            KIND_QUERY,
            "router",
            {
                query: "peer.get",
                peer_id: peerId,
            } satisfies QueryBody,
            frame => decodeBody<QueryGetResponseBody>(frame.bodyBytes)
        );
    }

    private async validateTargetPeer(mode: false | "exists" | "get"): Promise<void> {
        if (mode === "exists") {
            const exists = await this.queryPeerExists(this.connection.targetPeerId);
            if (!exists) {
                throw new RouterError(`target peer '${this.connection.targetPeerId}' does not exist`);
            }
            return;
        }

        const details = await this.queryPeerGet(this.connection.targetPeerId);
        if (!details.exists) {
            throw new RouterError(`target peer '${this.connection.targetPeerId}' does not exist`);
        }
        if (!details.connected) {
            throw new RouterError(`target peer '${this.connection.targetPeerId}' is not connected`);
        }
        if (details.class !== service) {
            throw new RouterError(`target peer '${this.connection.targetPeerId}' is not a service`);
        }
    }

    private async callRemote(functionName: string, args: unknown[]): Promise<unknown> {
        return this.sendRequest<ResponseBody>(
            KIND_CALL,
            this.connection.targetPeerId,
            {
                function: functionName,
                args,
                namespace: "default",
            } satisfies CallBody,
            frame => decodeBody<ResponseBody>(frame.bodyBytes)
        ).then(response => response.result);
    }

    private async sendRequest<T>(
        kind: typeof KIND_CALL | typeof KIND_QUERY,
        to: string,
        body: CallBody | QueryBody,
        decode: PendingDecoder<T>
    ): Promise<T> {
        if (!this.started || !this.transport || !this.callerPeerId) {
            throw new TransportFailureError("caller handle is not started");
        }

        const msgId = randomUUID();
        const envelope = this.buildEnvelope(kind, to, msgId);
        const requestTimeoutMs = this.connection.options.requestTimeoutMs ?? DEFAULT_REQUEST_TIMEOUT_MS;

        return await new Promise<T>((resolve, reject) => {
            const timer = setTimeout(() => {
                this.pendingRequests.delete(msgId);
                reject(new TransportFailureError(`request timed out after ${requestTimeoutMs}ms`));
            }, requestTimeoutMs);

            this.pendingRequests.set(msgId, {
                resolve: value => resolve(value as T),
                reject,
                decoder: decode,
                timer,
            });

            void this.transport!
                .send(envelope, encodeBody(body))
                .catch(error => {
                    clearTimeout(timer);
                    this.pendingRequests.delete(msgId);
                    reject(
                        error instanceof Error
                            ? error
                            : new TransportFailureError(String(error), { cause: error })
                    );
                });
        });
    }

    private resolvePending(frame: RoutedFrame): void {
        const pending = this.pendingRequests.get(frame.envelope.msg_id);
        if (!pending) {
            return;
        }

        this.pendingRequests.delete(frame.envelope.msg_id);
        clearTimeout(pending.timer);

        try {
            pending.resolve(pending.decoder(frame));
        } catch (error) {
            pending.reject(
                error instanceof Error
                    ? error
                    : new TransportFailureError(String(error), { cause: error })
            );
        }
    }

    private rejectPending(frame: RoutedFrame): void {
        const pending = this.pendingRequests.get(frame.envelope.msg_id);
        if (!pending) {
            return;
        }

        this.pendingRequests.delete(frame.envelope.msg_id);
        clearTimeout(pending.timer);

        const errorBody = decodeBody<ErrorBody>(frame.bodyBytes);
        pending.reject(this.errorFromBody(errorBody));
    }

    private rejectAllPending(error: Error): void {
        for (const [msgId, pending] of this.pendingRequests.entries()) {
            clearTimeout(pending.timer);
            pending.reject(error);
            this.pendingRequests.delete(msgId);
        }
    }

    private errorFromBody(body: ErrorBody): Error {
        if (body.kind === "application" || body.kind === "service") {
            return new RemoteCallError(body.message, {
                details: body,
            });
        }

        return new RouterError(body.message, {
            details: body,
        });
    }

    private buildEnvelope(kind: typeof KIND_CALL | typeof KIND_QUERY | typeof KIND_DISCONNECT, to: string, msgId: string = randomUUID()): Envelope {
        if (!this.callerPeerId) {
            throw new TransportFailureError("caller identity not initialized");
        }

        return {
            v: 5,
            kind,
            msg_id: msgId,
            from: this.callerPeerId,
            to,
            ts: Date.now() / 1000,
        };
    }
}
