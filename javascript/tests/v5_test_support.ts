/**
 * FILE: javascript/tests/v5_test_support.ts
 * PURPOSE: Share small path, process, and router helpers across the v5 JavaScript test files.
 * OWNS: Repo path resolution, router binary fallback, and generic child-process helpers for tests.
 * EXPORTS: repoRoot, javascriptRoot, rustRoot, ensureRouterBinaryEnv, examplePath, rustExampleBinary, spawnBinary, waitForExit, sleep, waitForPeerExists, getFreePort
 * DOCS: docs/spec.md
 */

import { spawn, type ChildProcess } from "child_process";
import * as net from "net";
import * as path from "path";
import { fileURLToPath } from "url";

export function repoRoot(): string {
    return path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
}

export function javascriptRoot(): string {
    return path.join(repoRoot(), "javascript");
}

export function rustRoot(): string {
    return path.join(repoRoot(), "rust");
}

export function ensureRouterBinaryEnv(): string {
    const fallback = path.join(repoRoot(), "router", "target", "debug", "multifrost-router");
    if (!process.env.MULTIFROST_ROUTER_BIN) {
        process.env.MULTIFROST_ROUTER_BIN = fallback;
    }
    return process.env.MULTIFROST_ROUTER_BIN;
}

export function examplePath(fileName: string): string {
    return path.join(javascriptRoot(), "examples", fileName);
}

export function rustExampleBinary(name: string): string {
    const extension = process.platform === "win32" ? ".exe" : "";
    return path.join(rustRoot(), "target", "debug", "examples", `${name}${extension}`);
}

export function spawnBinary(
    command: string,
    args: string[] = [],
    options: {
        env?: NodeJS.ProcessEnv;
        cwd?: string;
        stdio?: "inherit" | "ignore" | "pipe";
    } = {}
): ChildProcess {
    return spawn(command, args, {
        env: {
            ...process.env,
            ...options.env,
        },
        cwd: options.cwd,
        stdio: options.stdio ?? "pipe",
    });
}

export function waitForExit(child: ChildProcess): Promise<{ code: number | null; signal: NodeJS.Signals | null }> {
    return new Promise((resolve, reject) => {
        child.once("error", reject);
        child.once("exit", (code, signal) => {
            resolve({ code, signal });
        });
    });
}

export function sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
}

export async function waitForPeerExists(
    handle: { queryPeerExists(peerId: string): Promise<boolean> },
    peerId: string,
    timeoutMs: number = 10_000
): Promise<void> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
        if (await handle.queryPeerExists(peerId)) {
            return;
        }
        await sleep(50);
    }

    throw new Error(`peer ${peerId} did not appear in router registry`);
}

export async function getFreePort(): Promise<number> {
    return await new Promise<number>((resolve, reject) => {
        const server = net.createServer();
        server.listen(0, "127.0.0.1", () => {
            const address = server.address();
            if (address && typeof address === "object") {
                const { port } = address;
                server.close(() => resolve(port));
                return;
            }
            server.close(() => reject(new Error("failed to obtain a free port")));
        });
        server.once("error", reject);
    });
}
