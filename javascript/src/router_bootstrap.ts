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

interface LockFileData {
    format: string;
    pid: number;
    router_pid: number | null;
    port: number;
    created_at_unix: number;
    expires_at_unix: number;
    status: string;
    language: string;
}

interface AcquiredLock {
    path: string;
    data: LockFileData;
    update(fields: Partial<LockFileData>): Promise<void>;
    release(): Promise<void>;
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

async function isProcessAlive(pid: number): Promise<boolean> {
    if (pid <= 0) return false;
    try {
        if (process.platform === "win32") {
            const { execFile } = await import("child_process");
            const { promisify } = await import("util");
            const execFileAsync = promisify(execFile);
            const { stdout } = await execFileAsync("tasklist", [
                "/FI",
                `PID eq ${pid}`,
                "/FO",
                "csv",
                "/NH",
            ]);
            return stdout.includes(`"${pid}"`);
        } else {
            return process.kill(pid, 0);
        }
    } catch {
        return false;
    }
}

async function evaluateExistingLock(lockPath: string): Promise<string> {
    let raw: string;
    try {
        raw = await fs.readFile(lockPath, "utf-8");
    } catch {
        return "reclaim";
    }

    let data: any;
    try {
        data = JSON.parse(raw);
    } catch {
        return "reclaim";
    }

    if (typeof data !== "object" || data.format !== "v1") {
        return "reclaim";
    }

    const now = Date.now() / 1000;
    if (data.expires_at_unix < now) {
        return "reclaim";
    }

    if (!(await isProcessAlive(data.pid))) {
        return "reclaim";
    }

    if (data.status === "failed") {
        return "reclaim";
    }

    if (data.router_pid != null && (await isProcessAlive(data.router_pid))) {
        return "skip_spawn";
    }

    return "wait";
}

async function acquireStartupLock(
    lockPath: string,
    port: number,
    timeoutMs: number
): Promise<AcquiredLock | null> {
    await fs.mkdir(path.dirname(lockPath), { recursive: true });

    const deadline = Date.now() + timeoutMs;

    while (true) {
        try {
            const lockData: LockFileData = {
                format: "v1",
                pid: process.pid,
                router_pid: null,
                port: port,
                created_at_unix: Date.now() / 1000,
                expires_at_unix: (Date.now() + timeoutMs) / 1000,
                status: "starting",
                language: "javascript",
            };

            // Atomic exclusive-create
            await fs.writeFile(lockPath, JSON.stringify(lockData, null, 2), { flag: "wx" });

            return {
                path: lockPath,
                data: lockData,
                update: async (fields: Partial<LockFileData>) => {
                    Object.assign(lockData, fields);
                    try {
                        await fs.writeFile(lockPath, JSON.stringify(lockData, null, 2));
                    } catch {
                        // best-effort
                    }
                },
                release: async () => {
                    try {
                        await fs.unlink(lockPath);
                    } catch {
                        // best-effort
                    }
                },
            };
        } catch (error: any) {
            if (error?.code !== "EEXIST" && error?.code !== "EACCES") {
                throw new BootstrapFailureError(`unexpected lock error: ${error.message}`, {
                    cause: error,
                });
            }

            // Evaluate existing lock
            const result = await evaluateExistingLock(lockPath);

            switch (result) {
                case "reclaim":
                    try {
                        await fs.unlink(lockPath);
                    } catch {
                        /* race, someone else got it */
                    }
                    continue;
                case "skip_spawn":
                    return null; // signal: don't spawn, wait for router
                case "wait":
                    if (Date.now() >= deadline) {
                        throw new BootstrapTimeoutError(
                            `timed out waiting for router bootstrap lock at ${lockPath}`
                        );
                    }
                    await sleep(DEFAULT_POLL_INTERVAL_MS);
                    continue;
            }
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

    const lock = await acquireStartupLock(lockPath, port, readinessTimeoutMs);

    // skip_spawn: someone else has an alive router, wait for it
    if (lock === null) {
        const deadline = Date.now() + readinessTimeoutMs;
        while (Date.now() < deadline) {
            if (await routerIsReachable(endpoint, reachabilityTimeoutMs)) {
                return { endpoint, port, lockPath, logPath, routerBin, started: false };
            }
            await sleep(readinessPollIntervalMs);
        }
        throw new BootstrapTimeoutError(
            `router did not become reachable at ${endpoint} within ${readinessTimeoutMs}ms`
        );
    }

    let child: ChildProcess | undefined;

    try {
        if (await routerIsReachable(endpoint, reachabilityTimeoutMs)) {
            await lock.update({ status: "ready" });
            return { endpoint, port, lockPath, logPath, routerBin, started: false };
        }

        child = spawnRouterProcess(routerBin, port, logPath);
        await lock.update({ router_pid: child.pid ?? null });

        const deadline = Date.now() + readinessTimeoutMs;
        let spawnError: Error | undefined;

        child.once("error", (error) => {
            spawnError = error instanceof Error ? error : new Error(String(error));
        });

        while (Date.now() < deadline) {
            if (spawnError) {
                await lock.update({ status: "failed" });
                throw new BootstrapFailureError(
                    `router process failed to start: ${spawnError.message}`,
                    { cause: spawnError }
                );
            }

            if (child.exitCode !== null) {
                await lock.update({ status: "failed" });
                throw new BootstrapFailureError(
                    `router process exited before readiness (code=${child.exitCode}, signal=${child.signalCode ?? "null"})`
                );
            }

            if (await routerIsReachable(endpoint, reachabilityTimeoutMs)) {
                await lock.update({ status: "ready" });
                return { endpoint, port, lockPath, logPath, routerBin, started: true };
            }

            await sleep(readinessPollIntervalMs);
        }

        // Timeout — router did not become reachable within the deadline
        await lock.update({ status: "failed" });
        try {
            child.kill();
        } catch {
            // Best effort only.
        }

        throw new BootstrapTimeoutError(
            `router did not become reachable at ${endpoint} within ${readinessTimeoutMs}ms`
        );
    } catch (error) {
        if (lock !== null) {
            await lock.update({ status: "failed" }).catch(() => {});
        }
        if (error instanceof BootstrapTimeoutError || error instanceof BootstrapFailureError) {
            throw error;
        }

        throw new BootstrapFailureError(
            error instanceof Error ? error.message : String(error),
            { cause: error }
        );
    } finally {
        await lock.release().catch(() => undefined);
    }
}
