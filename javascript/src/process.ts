/**
 * FILE: javascript/src/process.ts
 * PURPOSE: Launch and manage spawned service processes without coupling them to caller transport state.
 * OWNS: SpawnOptions, ServiceProcess, child process startup, shutdown, and exit waiting.
 * EXPORTS: SpawnOptions, ServiceProcess, ProcessExitStatus, spawn
 * DOCS: docs/spec.md, agent_chat/node_v5_api_surface_2026-03-24.md
 */

import { spawn as spawnChildProcess, type ChildProcess } from "child_process";
import * as fs from "fs/promises";
import * as path from "path";

const DEFAULT_STDIO: "ignore" = "ignore";

export interface SpawnOptions {
    routerPort?: number;
    env?: NodeJS.ProcessEnv;
    cwd?: string;
    shell?: boolean;
}

export interface ProcessExitStatus {
    code: number | null;
    signal: NodeJS.Signals | null;
}

export class ServiceProcess {
    private child: ChildProcess | null;
    private readonly exited: Promise<ProcessExitStatus>;
    private readonly resolveExit: (status: ProcessExitStatus) => void;
    private readonly treeKill: boolean;
    private exitStatus?: ProcessExitStatus;

    constructor(child: ChildProcess, treeKill: boolean = false) {
        this.child = child;
        this.treeKill = treeKill;
        let resolveExit!: (status: ProcessExitStatus) => void;
        this.exited = new Promise<ProcessExitStatus>(resolve => {
            resolveExit = resolve;
        });
        this.resolveExit = resolveExit;

        child.once("exit", (code, signal) => {
            this.exitStatus = { code, signal };
            this.resolveExit(this.exitStatus);
            this.child = null;
        });
    }

    id(): number | undefined {
        return this.child?.pid;
    }

    async stop(): Promise<void> {
        if (this.child && this.exitStatus === undefined) {
            try {
                if (this.treeKill && this.child.pid && process.platform !== "win32") {
                    process.kill(-this.child.pid, "SIGTERM");
                } else {
                    this.child.kill();
                }
            } catch {
                // Best effort only.
            }
        }

        await this.wait();
    }

    async wait(): Promise<ProcessExitStatus> {
        if (this.exitStatus) {
            return this.exitStatus;
        }

        return await this.exited;
    }
}

async function canonicalizeEntrypointPath(serviceEntrypoint: string): Promise<string> {
    const absolutePath = path.resolve(serviceEntrypoint);
    try {
        return await fs.realpath(absolutePath);
    } catch {
        return absolutePath;
    }
}

function buildSpawnEnv(
    serviceEntrypointPath: string,
    options: SpawnOptions
): NodeJS.ProcessEnv {
    const env: NodeJS.ProcessEnv = {
        ...process.env,
        ...options.env,
        MULTIFROST_ENTRYPOINT_PATH: serviceEntrypointPath,
    };

    if (typeof options.routerPort === "number" && Number.isFinite(options.routerPort)) {
        env.MULTIFROST_ROUTER_PORT = String(options.routerPort);
    }

    return env;
}

async function waitForSpawn(child: ChildProcess): Promise<void> {
    await new Promise<void>((resolve, reject) => {
        child.once("spawn", () => resolve());
        child.once("error", error => reject(error));
    });
}

function splitCommandLine(executable: string): { command: string; args: string[] } {
    const parts = executable.trim().split(/\s+/).filter(Boolean);
    if (parts.length === 0) {
        throw new Error("spawn executable must not be empty");
    }

    return {
        command: parts[0],
        args: parts.slice(1),
    };
}

export async function spawn(
    serviceEntrypoint: string,
    executable: string,
    options: SpawnOptions = {}
): Promise<ServiceProcess> {
    const entrypointPath = await canonicalizeEntrypointPath(serviceEntrypoint);
    const env = buildSpawnEnv(entrypointPath, options);
    const { command, args } = splitCommandLine(executable);
    const child = spawnChildProcess(command, args, {
        cwd: options.cwd,
        env,
        shell: false,
        stdio: DEFAULT_STDIO,
    });

    await waitForSpawn(child);
    return new ServiceProcess(child);
}
