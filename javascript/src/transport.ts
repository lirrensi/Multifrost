/**
 * FILE: javascript/src/transport.ts
 * PURPOSE: Own the live WebSocket peer transport and route binary frames by v5 message kind.
 * OWNS: Socket lifecycle, binary frame send/receive, register handshake, and kind-based dispatch.
 * EXPORTS: WebSocketTransport, TransportCallbacks, RoutedFrame, TransportConnectOptions
 * DOCS: docs/spec.md
 */

import { randomUUID } from "crypto";
import WebSocket, { RawData } from "ws";
import {
    decodeBody,
    decodeEnvelope,
    decodeFrame,
    encodeBody,
    encodeFrame,
} from "./frame.js";
import {
    caller,
    Envelope,
    EnvelopeKind,
    KIND_CALL,
    KIND_DISCONNECT,
    KIND_ERROR,
    KIND_HEARTBEAT,
    KIND_QUERY,
    KIND_REGISTER,
    KIND_RESPONSE,
    PeerClass,
    RegisterAckBody,
    RegisterBody,
    service,
    validateEnvelopeFields,
} from "./protocol.js";
import {
    BootstrapFailureError,
    ProtocolError,
    RegistrationFailureError,
    TransportFailureError,
} from "./errors.js";

export interface RoutedFrame {
    envelope: Envelope;
    bodyBytes: Uint8Array;
}

export interface TransportCallbacks {
    response?: (frame: RoutedFrame) => void | Promise<void>;
    error?: (frame: RoutedFrame) => void | Promise<void>;
    query?: (frame: RoutedFrame) => void | Promise<void>;
    call?: (frame: RoutedFrame) => void | Promise<void>;
    heartbeat?: (frame: RoutedFrame) => void | Promise<void>;
    disconnect?: (frame: RoutedFrame) => void | Promise<void>;
    close?: (error?: Error) => void | Promise<void>;
}

export interface TransportConnectOptions {
    endpoint: string;
    peerId: string;
    peerClass: PeerClass;
    callbacks?: TransportCallbacks;
    registerTimeoutMs?: number;
}

function isBinaryLike(data: RawData): boolean {
    return typeof data !== "string";
}

function rawDataToBytes(data: RawData): Uint8Array {
    if (Buffer.isBuffer(data)) {
        return data;
    }

    if (Array.isArray(data)) {
        return Buffer.concat(data.map(part => (Buffer.isBuffer(part) ? part : Buffer.from(part))));
    }

    return Buffer.from(data);
}

function toFrame(raw: RawData): RoutedFrame {
    const bytes = rawDataToBytes(raw);
    const parts = decodeFrame(bytes);
    return {
        envelope: decodeEnvelope(parts.envelopeBytes),
        bodyBytes: parts.bodyBytes,
    };
}

function nowSeconds(): number {
    return Date.now() / 1000;
}

export class WebSocketTransport {
    private readonly socket: WebSocket;
    private readonly endpoint: string;
    private readonly peerId: string;
    private readonly peerClass: PeerClass;
    private readonly callbacks: TransportCallbacks;
    private readonly registerTimeoutMs: number;
    private closed = false;
    private active = false;
    private readonly closedPromise: Promise<void>;
    private readonly closedResolve: () => void;

    private constructor(
        socket: WebSocket,
        options: TransportConnectOptions,
        closedPromise: Promise<void>,
        closedResolve: () => void
    ) {
        this.socket = socket;
        this.endpoint = options.endpoint;
        this.peerId = options.peerId;
        this.peerClass = options.peerClass;
        this.callbacks = options.callbacks ?? {};
        this.registerTimeoutMs = options.registerTimeoutMs ?? 5_000;
        this.closedPromise = closedPromise;
        this.closedResolve = closedResolve;
    }

    static async connect(options: TransportConnectOptions): Promise<WebSocketTransport> {
        const socket = new WebSocket(options.endpoint);
        let closedResolve!: () => void;
        const closedPromise = new Promise<void>(resolve => {
            closedResolve = resolve;
        });

        const transport = new WebSocketTransport(
            socket,
            options,
            closedPromise,
            closedResolve
        );

        await transport.waitForRegister();
        transport.active = true;
        return transport;
    }

    get isActive(): boolean {
        return this.active && !this.closed;
    }

    get isClosed(): boolean {
        return this.closed;
    }

    get endpointUrl(): string {
        return this.endpoint;
    }

    get peer(): { peerId: string; peerClass: PeerClass } {
        return {
            peerId: this.peerId,
            peerClass: this.peerClass,
        };
    }

    async send(envelope: Envelope, bodyBytes: Uint8Array): Promise<void> {
        if (!this.isActive) {
            throw new TransportFailureError("transport is not active");
        }

        const frame = encodeFrame(validateEnvelopeFields(envelope), bodyBytes);
        await this.sendBytes(frame);
    }

    async sendBody(envelope: Envelope, body: unknown): Promise<void> {
        await this.send(envelope, encodeBody(body));
    }

    async close(): Promise<void> {
        if (this.closed) {
            return;
        }

        this.closed = true;
        try {
            if (this.socket.readyState === WebSocket.OPEN || this.socket.readyState === WebSocket.CONNECTING) {
                this.socket.close();
            }
        } finally {
            this.closedResolve();
        }
    }

    waitForClose(): Promise<void> {
        return this.closedPromise;
    }

    private async waitForRegister(): Promise<void> {
        const registerMsgId = randomUUID();
        const registerEnvelope: Envelope = {
            v: 5,
            kind: KIND_REGISTER,
            msg_id: registerMsgId,
            from: this.peerId,
            to: "router",
            ts: nowSeconds(),
        };
        const registerBody: RegisterBody = {
            peer_id: this.peerId,
            class: this.peerClass,
        };
        const registerAck = new Promise<void>((resolve, reject) => {
            const timeout = setTimeout(() => {
                cleanup();
                reject(
                    new RegistrationFailureError(
                        `registration timed out after ${this.registerTimeoutMs}ms`
                    )
                );
            }, this.registerTimeoutMs);

            const cleanup = () => clearTimeout(timeout);

            const onError = (error: Error) => {
                cleanup();
                reject(new TransportFailureError(error.message, { cause: error }));
            };

            const onClose = () => {
                cleanup();
                if (!this.active) {
                    reject(new TransportFailureError("socket closed before registration completed"));
                } else {
                    resolve();
                }
            };

            this.socket.once("error", onError);
            this.socket.once("close", onClose);

            const onMessage = (data: RawData, isBinary: boolean) => {
                if (!isBinary || !isBinaryLike(data)) {
                    cleanup();
                    reject(new ProtocolError("router sent a non-binary register reply"));
                    return;
                }

                try {
                    const frame = toFrame(data);
                    if (frame.envelope.msg_id !== registerMsgId) {
                        cleanup();
                        reject(
                            new RegistrationFailureError(
                                `unexpected message during registration: ${frame.envelope.kind}`
                            )
                        );
                        return;
                    }

                    if (frame.envelope.kind !== KIND_RESPONSE) {
                        cleanup();
                        reject(
                            new RegistrationFailureError(
                                `unexpected register reply kind: ${frame.envelope.kind}`
                            )
                        );
                        return;
                    }

                    const ack = decodeBody<RegisterAckBody>(frame.bodyBytes);
                    if (!ack.accepted) {
                        cleanup();
                        reject(
                            new RegistrationFailureError(ack.reason ?? "router rejected registration")
                        );
                        return;
                    }

                    cleanup();
                    this.socket.off("error", onError);
                    this.socket.off("close", onClose);
                    this.socket.off("message", onMessage);
                    resolve();
                } catch (error) {
                    cleanup();
                    reject(
                        error instanceof Error
                            ? error
                            : new RegistrationFailureError(String(error))
                    );
                }
            };

            this.socket.on("message", onMessage);
        });

        const open = new Promise<void>((resolve, reject) => {
            this.socket.once("open", () => {
                void this.sendBytes(encodeFrame(registerEnvelope, encodeBody(registerBody)))
                    .then(() => resolve())
                    .catch(reject);
            });
            this.socket.once("error", (error: Error) => {
                reject(new TransportFailureError(error.message, { cause: error }));
            });
        });

        await open;
        await registerAck;
        this.attachRuntimeListeners();
    }

    private attachRuntimeListeners(): void {
        this.socket.on("message", (data: RawData, isBinary: boolean) => {
            if (!isBinary || !isBinaryLike(data)) {
                this.failTransport(new ProtocolError("received non-binary websocket message"));
                return;
            }

            let frame: RoutedFrame;
            try {
                frame = toFrame(data);
            } catch (error) {
                this.failTransport(error instanceof Error ? error : new ProtocolError(String(error)));
                return;
            }

            this.dispatchFrame(frame);
        });

        this.socket.on("close", () => {
            this.closed = true;
            this.active = false;
            this.closedResolve();
            void Promise.resolve(this.callbacks.close?.()).catch(() => undefined);
        });

        this.socket.on("error", (error: Error) => {
            this.failTransport(new TransportFailureError(error.message, { cause: error }));
        });
    }

    private dispatchFrame(frame: RoutedFrame): void {
        const handler = (() => {
            switch (frame.envelope.kind) {
                case KIND_RESPONSE:
                    return this.callbacks.response;
                case KIND_ERROR:
                    return this.callbacks.error;
                case KIND_QUERY:
                    return this.callbacks.query;
                case KIND_CALL:
                    return this.callbacks.call;
                case KIND_HEARTBEAT:
                    return this.callbacks.heartbeat;
                case KIND_DISCONNECT:
                    return this.callbacks.disconnect;
                default:
                    return undefined;
            }
        })();
        if (!handler || typeof handler !== "function") {
            return;
        }

        void Promise.resolve(handler(frame)).catch(() => undefined);
    }

    private failTransport(error: Error): void {
        if (this.closed) {
            return;
        }

        this.closed = true;
        this.active = false;
        try {
            this.socket.terminate();
        } catch {
            // Ignore close failures.
        }
        this.closedResolve();
        void Promise.resolve(this.callbacks.close?.(error)).catch(() => undefined);
    }

    private async sendBytes(bytes: Uint8Array): Promise<void> {
        if (this.closed) {
            throw new TransportFailureError("transport is closed");
        }

        await new Promise<void>((resolve, reject) => {
            this.socket.send(bytes, { binary: true }, (error: Error | undefined) => {
                if (error) {
                    reject(new TransportFailureError(error.message, { cause: error }));
                } else {
                    resolve();
                }
            });
        });
    }
}
