/**
 * FILE: javascript/src/errors.ts
 * PURPOSE: Provide stable v5 error classes for transport, bootstrap, registration, router, protocol, and remote failures.
 * OWNS: Public Node v5 error taxonomy and origin markers for routing and connection failures.
 * EXPORTS: MultifrostError, ProtocolError, TransportFailureError, BootstrapFailureError, BootstrapTimeoutError, RegistrationFailureError, RouterError, RemoteCallError
 * DOCS: docs/spec.md
 */

export type ErrorOrigin = "protocol" | "transport" | "bootstrap" | "registration" | "router" | "service";

export interface MultifrostErrorOptions {
    code?: string;
    origin?: ErrorOrigin;
    details?: unknown;
    cause?: unknown;
}

export class MultifrostError extends Error {
    public readonly code: string;
    public readonly origin?: ErrorOrigin;
    public readonly details?: unknown;

    constructor(name: string, message: string, options: MultifrostErrorOptions = {}) {
        super(message, options.cause === undefined ? undefined : { cause: options.cause });
        this.name = name;
        this.code = options.code ?? name;
        this.origin = options.origin;
        this.details = options.details;
    }
}

export class ProtocolError extends MultifrostError {
    constructor(message: string, options: Omit<MultifrostErrorOptions, "origin" | "code"> = {}) {
        super("ProtocolError", message, {
            ...options,
            code: "MF_PROTOCOL_ERROR",
            origin: "protocol",
        });
    }
}

export class TransportFailureError extends MultifrostError {
    constructor(message: string, options: Omit<MultifrostErrorOptions, "origin" | "code"> = {}) {
        super("TransportFailureError", message, {
            ...options,
            code: "MF_TRANSPORT_FAILURE",
            origin: "transport",
        });
    }
}

export class BootstrapFailureError extends MultifrostError {
    constructor(message: string, options: MultifrostErrorOptions = {}) {
        super("BootstrapFailureError", message, {
            ...options,
            code: "MF_BOOTSTRAP_FAILURE",
            origin: "bootstrap",
        });
    }
}

export class BootstrapTimeoutError extends BootstrapFailureError {
    constructor(message: string, options: MultifrostErrorOptions = {}) {
        super(message, {
            ...options,
            code: "MF_BOOTSTRAP_TIMEOUT",
            origin: "bootstrap",
        });
        this.name = "BootstrapTimeoutError";
    }
}

export class RegistrationFailureError extends MultifrostError {
    constructor(message: string, options: Omit<MultifrostErrorOptions, "origin" | "code"> = {}) {
        super("RegistrationFailureError", message, {
            ...options,
            code: "MF_REGISTRATION_FAILURE",
            origin: "registration",
        });
    }
}

export class RouterError extends MultifrostError {
    constructor(message: string, options: Omit<MultifrostErrorOptions, "origin" | "code"> = {}) {
        super("RouterError", message, {
            ...options,
            code: "MF_ROUTER_ERROR",
            origin: "router",
        });
    }
}

export class RemoteCallError extends MultifrostError {
    constructor(message: string, options: Omit<MultifrostErrorOptions, "origin" | "code"> = {}) {
        super("RemoteCallError", message, {
            ...options,
            code: "MF_REMOTE_CALL_ERROR",
            origin: "service",
        });
    }
}
