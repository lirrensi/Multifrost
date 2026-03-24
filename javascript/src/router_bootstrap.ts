/**
 * FILE: javascript/src/router_bootstrap.ts
 * PURPOSE: Ensure the shared v5 router is reachable, or start it under the coordinated startup lock.
 * OWNS: Router port resolution, lock handling, log path resolution, readiness polling, and process bootstrap.
 * EXPORTS: resolveRouterPort, resolveRouterEndpoint, defaultRouterLockPath, defaultRouterLogPath, routerIsReachable, bootstrapRouter, RouterBootstrapResult
 * DOCS: docs/spec.md, rust/src/router_bootstrap.rs
 */

import { spawn, ChildProcess } from "child_process";
import { closeSync, openSync } from "fs";
import * as fs from "fs/promises";
import lockfile from "proper-lockfile";
import * as path from "path";
import * as os from "os";
import WebSocket from "ws";
import {
    DEFAULT_ROUTER_PORT,
    ROUTER_LOCK_SUFFIX,
    ROUTER_LOG_SUFFIX,
    ROUTER_PORT_ENV,
} from "./protocol.js";
import { BootstrapFailureError, BootstrapTimeoutError } from "./errors.js";

const DEFAULT_READY_TIMEOUT_MS = 10_000;
const DEFAULT_POLL_INTERVAL_MS = 100;
const DEFAULT_REACHABILITY_TIMEOUT_MS = 250;
const LOCK_STALE_MS = 15_000;

export interface RouterBootstrapOptions {
    port?: number;
    routerBin?: string;
    lockPath?: string;
    logPath?: string;
    readinessTimeoutMs?: number;
    readinessPollIntervalMs?: number;
    reachabilityTimeoutMs?: number;
}

export interface RouterBootstrapResult {
    endpoint: string;
    port: number;
    lockPath: string;
    logPath: string;
    routerBin: string;
    started: boolean;
}

function sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
}

function toRouterPort(value: unknown, fallback: number): number {
    if (typeof value !== "string") {
        return fallback;
    }

    const parsed = Number.parseInt(value, 10);
    return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

export function resolveRouterPort(options: RouterBootstrapOptions = {}): number {
    if (typeof options.port === "number" && Number.isFinite(options.port) && options.port > 0) {
        return options.port;
    }

    return toRouterPort(process.env[ROUTER_PORT_ENV], DEFAULT_ROUTER_PORT);
}

export function resolveRouterEndpoint(port: number = resolveRouterPort()): string {
    return `ws://127.0.0.1:${port}`;
}

export function defaultRouterLockPath(): string {
    return path.join(os.homedir(), ROUTER_LOCK_SUFFIX);
}

export function defaultRouterLogPath(): string {
    return path.join(os.homedir(), ROUTER_LOG_SUFFIX);
}

export function resolveRouterBin(options: RouterBootstrapOptions = {}): string {
    return options.routerBin ?? process.env.MULTIFROST_ROUTER_BIN ?? "multifrost-router";
}

export async function routerIsReachable(
    endpoint: string,
    timeoutMs: number = DEFAULT_REACHABILITY_TIMEOUT_MS
): Promise<boolean> {
    return await new Promise<boolean>(resolve => {
        const socket = new WebSocket(endpoint);
        let settled = false;

        const finish = (value: boolean) => {
            if (settled) return;
            settled = true;
            clearTimeout(timer);
            try {
                socket.terminate();
            } catch {
                // Ignore termination failures during probe cleanup.
            }
            resolve(value);
        };

        const timer = setTimeout(() => finish(false), timeoutMs);
        socket.once("open", () => finish(true));
        socket.once("error", () => finish(false));
        socket.once("close", () => finish(false));
    });
}

async function ensureLockFile(lockPath: string): Promise<void> {
    await fs.mkdir(path.dirname(lockPath), { recursive: true });
    await fs.writeFile(lockPath, "", { flag: "a" });
}

async function acquireStartupLock(
    lockPath: string,
    timeoutMs: number
): Promise<() => Promise<void>> {
    const deadline = Date.now() + timeoutMs;

    while (true) {
        try {
            return await lockfile.lock(lockPath, {
                stale: LOCK_STALE_MS,
                realpath: false,
                retries: 0,
            });
        } catch (error) {
            const code = typeof error === "object" && error !== null ? (error as { code?: string }).code : undefined;
            if (code !== "ELOCKED") {
                throw error;
            }

            if (Date.now() >= deadline) {
                throw new BootstrapTimeoutError(
                    `timed out waiting for router bootstrap lock at ${lockPath}`
                );
            }

            await sleep(DEFAULT_POLL_INTERVAL_MS);
        }
    }
}

function spawnRouterProcess(routerBin: string, port: number, logPath: string): ChildProcess {
    const logFd = openSync(logPath, "a");
    const child = spawn(routerBin, [], {
        env: {
            ...process.env,
            [ROUTER_PORT_ENV]: String(port),
        },
        detached: true,
        stdio: ["ignore", logFd, logFd],
    });
    child.unref();

    child.once("exit", () => {
        try {
            closeSync(logFd);
        } catch {
            // Ignore close failures.
        }
    });

    child.once("error", () => {
        try {
            closeSync(logFd);
        } catch {
            // Ignore close failures.
        }
    });

    return child;
}

export async function bootstrapRouter(
    options: RouterBootstrapOptions = {}
): Promise<RouterBootstrapResult> {
    const port = resolveRouterPort(options);
    const endpoint = resolveRouterEndpoint(port);
    const lockPath = options.lockPath ?? defaultRouterLockPath();
    const logPath = options.logPath ?? defaultRouterLogPath();
    const routerBin = resolveRouterBin(options);
    const readinessTimeoutMs = options.readinessTimeoutMs ?? DEFAULT_READY_TIMEOUT_MS;
    const readinessPollIntervalMs = options.readinessPollIntervalMs ?? DEFAULT_POLL_INTERVAL_MS;
    const reachabilityTimeoutMs = options.reachabilityTimeoutMs ?? DEFAULT_REACHABILITY_TIMEOUT_MS;

    if (await routerIsReachable(endpoint, reachabilityTimeoutMs)) {
        return { endpoint, port, lockPath, logPath, routerBin, started: false };
    }

    await ensureLockFile(lockPath);
    const release = await acquireStartupLock(lockPath, readinessTimeoutMs);

    let child: ChildProcess | undefined;
    let spawnError: Error | undefined;

    try {
        if (await routerIsReachable(endpoint, reachabilityTimeoutMs)) {
            return { endpoint, port, lockPath, logPath, routerBin, started: false };
        }

        child = spawnRouterProcess(routerBin, port, logPath);
        child.once("error", error => {
            spawnError = error instanceof Error ? error : new Error(String(error));
        });

        const deadline = Date.now() + readinessTimeoutMs;
        while (Date.now() < deadline) {
            if (spawnError) {
                throw new BootstrapFailureError(
                    `router process failed to start: ${spawnError.message}`,
                    { cause: spawnError }
                );
            }

            if (child.exitCode !== null) {
                throw new BootstrapFailureError(
                    `router process exited before readiness (code=${child.exitCode}, signal=${child.signalCode ?? "null"})`
                );
            }

            if (await routerIsReachable(endpoint, reachabilityTimeoutMs)) {
                return { endpoint, port, lockPath, logPath, routerBin, started: true };
            }

            await sleep(readinessPollIntervalMs);
        }

        try {
            child.kill();
        } catch {
            // Best effort only.
        }

        throw new BootstrapTimeoutError(
            `router did not become reachable at ${endpoint} within ${readinessTimeoutMs}ms`
        );
    } catch (error) {
        if (error instanceof BootstrapTimeoutError || error instanceof BootstrapFailureError) {
            throw error;
        }

        throw new BootstrapFailureError(
            error instanceof Error ? error.message : String(error),
            { cause: error }
        );
    } finally {
        await release().catch(() => undefined);
    }
}
