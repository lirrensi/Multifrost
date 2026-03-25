/**
 * FILE: javascript/tests/v5_unit.test.ts
 * PURPOSE: Exercise the v5 JavaScript frame, transport, bootstrap, and caller/service round-trip behavior.
 * OWNS: Local protocol correctness, router bootstrap coordination, and Node-only v5 runtime checks.
 * EXPORTS: None
 * DOCS: docs/spec.md, docs/msgpack_interop.md, agent_chat/node_v5_api_surface_2026-03-24.md
 */

import assert from "node:assert/strict";
import { chmod, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import * as os from "node:os";
import * as path from "node:path";
import WebSocket, { WebSocketServer } from "ws";
import { connect } from "../src/connection.js";
import { decodeBody, decodeEnvelope, decodeFrame, encodeBody, encodeFrame } from "../src/frame.js";
import { RegistrationFailureError, RouterError, TransportFailureError } from "../src/errors.js";
import { bootstrapRouter } from "../src/router_bootstrap.js";
import { WebSocketTransport } from "../src/transport.js";
import {
    KIND_CALL,
    KIND_QUERY,
    KIND_RESPONSE,
    PROTOCOL_VERSION,
    QueryBody,
    RegisterAckBody,
    RegisterBody,
    caller,
    service,
} from "../src/protocol.js";
import { spawn } from "../src/process.js";
import {
    ensureRouterBinaryEnv,
    examplePath,
    getFreePort,
    javascriptRoot,
    sleep,
    waitForPeerExists,
} from "./v5_test_support.js";

type TestFn = () => Promise<void> | void;

async function run(name: string, fn: TestFn): Promise<void> {
    try {
        await fn();
        console.log(`ok - ${name}`);
    } catch (error) {
        console.error(`not ok - ${name}`);
        throw error;
    }
}

function nowSeconds(): number {
    return Date.now() / 1000;
}

async function withTimeout<T>(promise: Promise<T>, timeoutMs: number, label: string): Promise<T> {
    return await Promise.race([
        promise,
        new Promise<T>((_, reject) =>
            setTimeout(() => reject(new Error(`${label} timed out after ${timeoutMs}ms`)), timeoutMs)
        ),
    ]);
}

function toBuffer(raw: unknown): Buffer {
    if (Buffer.isBuffer(raw)) {
        return raw;
    }

    if (Array.isArray(raw)) {
        return Buffer.concat(raw.map(part => toBuffer(part)));
    }

    if (raw instanceof ArrayBuffer) {
        return Buffer.from(raw);
    }

    if (ArrayBuffer.isView(raw)) {
        return Buffer.from(raw.buffer, raw.byteOffset, raw.byteLength);
    }

    return Buffer.from(raw as Uint8Array);
}

async function startFakeRouter(
    accept: boolean,
    onRuntimeMessage?: (socket: WebSocket, frame: { envelope: ReturnType<typeof decodeEnvelope>; bodyBytes: Uint8Array }) => void | Promise<void>
): Promise<{ endpoint: string; port: number; close: () => Promise<void> }> {
    const server = new WebSocketServer({ host: "127.0.0.1", port: 0 });
    await new Promise<void>((resolve, reject) => {
        server.once("listening", () => resolve());
        server.once("error", reject);
    });

    const address = server.address();
    if (!address || typeof address === "string") {
        throw new Error("failed to bind fake router server");
    }

    server.on("connection", socket => {
        socket.once("message", raw => {
            const frame = decodeFrame(toBuffer(raw));
            const registerEnvelope = decodeEnvelope(frame.envelopeBytes);
            const registerBody = decodeBody<RegisterBody>(frame.bodyBytes);

            const ackBody: RegisterAckBody = accept
                ? { accepted: true, reason: null }
                : { accepted: false, reason: "duplicate peer_id" };

            assert.equal(registerEnvelope.v, PROTOCOL_VERSION);
            assert.equal(registerEnvelope.kind, "register");
            assert.equal(registerBody.class, caller);

            const ackEnvelope = {
                v: PROTOCOL_VERSION,
                kind: KIND_RESPONSE,
                msg_id: registerEnvelope.msg_id,
                from: "router",
                to: registerEnvelope.from,
                ts: nowSeconds(),
            };

            socket.send(encodeFrame(ackEnvelope, encodeBody(ackBody)));

            if (!accept || !onRuntimeMessage) {
                return;
            }

            socket.on("message", runtimeRaw => {
                const runtimeFrame = decodeFrame(toBuffer(runtimeRaw));
                void Promise.resolve(
                    onRuntimeMessage(socket, {
                        envelope: decodeEnvelope(runtimeFrame.envelopeBytes),
                        bodyBytes: runtimeFrame.bodyBytes,
                    })
                ).catch(() => undefined);
            });
        });
    });

    return {
        endpoint: `ws://127.0.0.1:${address.port}`,
        port: address.port,
        close: async () => {
            for (const client of server.clients) {
                try {
                    client.terminate();
                } catch {
                    // Best effort cleanup.
                }
            }
            await new Promise<void>(resolve => server.close(() => resolve()));
        },
    };
}

async function spawnMathService(routerPort?: number) {
    ensureRouterBinaryEnv();
    const serviceEntrypoint = examplePath("math_worker_service.ts");
    return await spawn(
        serviceEntrypoint,
        `node ./node_modules/tsx/dist/cli.mjs ${serviceEntrypoint}`,
        {
            cwd: javascriptRoot(),
            routerPort,
        }
    );
}

async function testFrameEncodeDecode(): Promise<void> {
    const envelope = {
        v: PROTOCOL_VERSION,
        kind: KIND_CALL,
        msg_id: "msg-1",
        from: "caller-1",
        to: "service-1",
        ts: nowSeconds(),
    };
    const body = Buffer.from([0x82, 0xa3, 0x61, 0x72, 0x67, 0x01]);

    const parts = decodeFrame(encodeFrame(envelope, body));
    assert.deepEqual(Array.from(parts.bodyBytes), Array.from(body));
}

async function testBodyPreservation(): Promise<void> {
    const envelope = {
        v: PROTOCOL_VERSION,
        kind: KIND_RESPONSE,
        msg_id: "msg-2",
        from: "service-1",
        to: "caller-1",
        ts: nowSeconds(),
    };
    const body = Buffer.from([0x83, 0xa3, 0x66, 0x6f, 0x6f, 0x01, 0xa3, 0x62, 0x61, 0x72, 0x02]);

    const parts = decodeFrame(encodeFrame(envelope, body));
    assert.deepEqual(Array.from(parts.bodyBytes), Array.from(body));
}

async function testRegisterAckSuccessAndRejection(): Promise<void> {
    const success = await startFakeRouter(true);
    try {
        const transport = await WebSocketTransport.connect({
            endpoint: success.endpoint,
            peerId: "caller-1",
            peerClass: caller,
            registerTimeoutMs: 2_000,
        });
        assert.equal(transport.isActive, true);
        await transport.close();
    } finally {
        await success.close();
    }

    const failure = await startFakeRouter(false);
    try {
        await assert.rejects(
            WebSocketTransport.connect({
                endpoint: failure.endpoint,
                peerId: "caller-2",
                peerClass: caller,
                registerTimeoutMs: 2_000,
            }),
            RegistrationFailureError
        );
    } finally {
        await failure.close();
    }
}

async function testQueryAndRoundTrip(): Promise<void> {
    const routerPort = await getFreePort();
    const serviceProcess = await spawnMathService(routerPort);
    const connection = connect("math-service", { routerPort });
    const handle = connection.handle();

    try {
        await handle.start();
        await waitForPeerExists(handle, "math-service");

        assert.equal(await handle.queryPeerExists("math-service"), true);

        const peer = await handle.queryPeerGet("math-service");
        assert.equal(peer.exists, true);
        assert.equal(peer.class, service);

        assert.equal(await handle.call.add(10, 20), 30);
        assert.equal(await handle.call.multiply(6, 7), 42);
        assert.equal(await handle.call.factorial(5), 120);
    } finally {
        await handle.stop().catch(() => undefined);
        await serviceProcess.stop().catch(() => undefined);
    }
}

async function testDisconnectInvalidation(): Promise<void> {
    const routerPort = await getFreePort();
    const serviceProcess = await spawnMathService(routerPort);
    const connection = connect("math-service", { routerPort });
    const handle = connection.handle();

    try {
        await handle.start();
        await waitForPeerExists(handle, "math-service");

        await serviceProcess.stop();
        await sleep(250);

        await assert.rejects(handle.call.add(1, 2));
    } finally {
        await handle.stop().catch(() => undefined);
        await serviceProcess.stop().catch(() => undefined);
    }
}

async function testDuplicatePeerIdRejection(): Promise<void> {
    const routerPort = await getFreePort();
    const first = await spawnMathService(routerPort);
    const connection = connect("math-service", { routerPort });
    const handle = connection.handle();
    await handle.start();
    await waitForPeerExists(handle, "math-service");

    const duplicate = await spawnMathService(routerPort);
    try {
        const exit = await withTimeout(duplicate.wait(), 15_000, "duplicate service exit");
        assert.notEqual(exit.code, 0);
    } finally {
        await handle.stop().catch(() => undefined);
        await first.stop().catch(() => undefined);
        await duplicate.stop().catch(() => undefined);
    }
}

async function testRequestTimeoutDoesNotPoisonLaterQueries(): Promise<void> {
    let queryCount = 0;
    const fakeRouter = await startFakeRouter(true, async (socket, frame) => {
        if (frame.envelope.kind !== KIND_QUERY) {
            return;
        }

        queryCount += 1;
        if (queryCount === 1) {
            return;
        }

        const query = decodeBody<QueryBody>(frame.bodyBytes);
        const queryPeerId = query.peer_id;
        const ackEnvelope = {
            v: PROTOCOL_VERSION,
            kind: KIND_RESPONSE,
            msg_id: frame.envelope.msg_id,
            from: "router",
            to: frame.envelope.from,
            ts: nowSeconds(),
        };

        socket.send(
            encodeFrame(
                ackEnvelope,
                encodeBody({
                    peer_id: queryPeerId,
                    exists: false,
                    class: null,
                    connected: false,
                })
            )
        );
    });

    const handle = connect("math-service", {
        callerPeerId: "caller-timeout",
        requestTimeoutMs: 100,
        routerPort: fakeRouter.port,
    }).handle();

    try {
        await handle.start();
        await assert.rejects(handle.queryPeerExists("slow-peer"), TransportFailureError);
        assert.equal(await handle.queryPeerExists("slow-peer"), false);
    } finally {
        await handle.stop().catch(() => undefined);
        await fakeRouter.close();
    }
}

async function testHandleStartRejectsMissingTargetWithEagerQuery(): Promise<void> {
    const fakeRouter = await startFakeRouter(true, async (socket, frame) => {
        if (frame.envelope.kind !== KIND_QUERY) {
            return;
        }

        const query = decodeBody<QueryBody>(frame.bodyBytes);
        const ackEnvelope = {
            v: PROTOCOL_VERSION,
            kind: KIND_RESPONSE,
            msg_id: frame.envelope.msg_id,
            from: "router",
            to: frame.envelope.from,
            ts: nowSeconds(),
        };

        socket.send(
            encodeFrame(
                ackEnvelope,
                encodeBody({
                    peer_id: query.peer_id,
                    exists: false,
                    class: null,
                    connected: false,
                })
            )
        );
    });

    const handle = connect("missing-service", {
        callerPeerId: "caller-eager",
        requestTimeoutMs: 200,
        eagerTargetQuery: "get",
        routerPort: fakeRouter.port,
    }).handle();

    try {
        await assert.rejects(handle.start(), RouterError);
        await assert.rejects(handle.queryPeerExists("missing-service"), TransportFailureError);
    } finally {
        await handle.stop().catch(() => undefined);
        await fakeRouter.close();
    }
}

async function testPendingRequestFailsWhenRouterCloses(): Promise<void> {
    const fakeRouter = await startFakeRouter(true, async (socket, frame) => {
        if (frame.envelope.kind !== KIND_CALL) {
            return;
        }

        socket.close();
    });

    const handle = connect("math-service", {
        callerPeerId: "caller-close",
        requestTimeoutMs: 500,
        routerPort: fakeRouter.port,
    }).handle();

    try {
        await handle.start();
        await assert.rejects(handle.call.add(1, 2), TransportFailureError);
        await assert.rejects(handle.queryPeerExists("math-service"), TransportFailureError);
    } finally {
        await handle.stop().catch(() => undefined);
        await fakeRouter.close();
    }
}

async function testBootstrapLockBehavior(): Promise<void> {
    if (process.platform === "win32") {
        return;
    }

    const tempDir = await mkdtemp(path.join(os.tmpdir(), "mf-lock-"));
    const counterFile = path.join(tempDir, "counter.txt");
    const pidFile = path.join(tempDir, "pid.txt");
    const scriptFile = path.join(tempDir, "fake_router.mjs");
    const lockPath = path.join(tempDir, "router.lock");
    const logPath = path.join(tempDir, "router.log");
    const port = await getFreePort();

    const wsPackagePath = path.join(javascriptRoot(), "node_modules", "ws");

    await writeFile(counterFile, "0");
    await writeFile(
        scriptFile,
        `#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { WebSocketServer } = require(${JSON.stringify(wsPackagePath)});

const counterFile = ${JSON.stringify(counterFile)};
const pidFile = ${JSON.stringify(pidFile)};
const port = Number(process.env.MULTIFROST_ROUTER_PORT);

writeFileSync(pidFile, String(process.pid));
const current = Number(readFileSync(counterFile, "utf8") || "0");
writeFileSync(counterFile, String(current + 1));

const server = new WebSocketServer({ host: "127.0.0.1", port });
server.on("connection", socket => {
  socket.on("message", () => {});
});

const shutdown = () => {
  server.close(() => process.exit(0));
};

process.on("SIGTERM", shutdown);
process.on("SIGINT", shutdown);
`
    );
    await chmod(scriptFile, 0o755);

    const options = {
        port,
        routerBin: scriptFile,
        lockPath,
        logPath,
        readinessTimeoutMs: 3_000,
        readinessPollIntervalMs: 50,
        reachabilityTimeoutMs: 100,
    };

    const firstRun = bootstrapRouter(options);
    const secondRun = bootstrapRouter(options);
    const [first, second] = await Promise.all([firstRun, secondRun]);

    try {
        assert.equal([first.started, second.started].filter(Boolean).length, 1);
        const counter = Number(await readFile(counterFile, "utf8"));
        assert.equal(counter, 1);
    } finally {
        const pidText = await readFile(pidFile, "utf8").catch(() => "");
        const pid = Number(pidText);
        if (Number.isFinite(pid) && pid > 0) {
            try {
                process.kill(pid, "SIGTERM");
            } catch {
                // Best effort cleanup.
            }
        }

        await sleep(150);
        await rm(tempDir, { recursive: true, force: true });
    }
}

async function main(): Promise<void> {
    await run("frame encode/decode", testFrameEncodeDecode);
    await run("body preservation", testBodyPreservation);
    await run("register ack success/rejection", testRegisterAckSuccessAndRejection);
    await run("query and round trip", testQueryAndRoundTrip);
    await run("disconnect invalidation", testDisconnectInvalidation);
    await run("duplicate peer id rejection", testDuplicatePeerIdRejection);
    await run("request timeout cleanup", testRequestTimeoutDoesNotPoisonLaterQueries);
    await run("eager target query missing target", testHandleStartRejectsMissingTargetWithEagerQuery);
    await run("pending request fails on router close", testPendingRequestFailsWhenRouterCloses);
    await run("router bootstrap lock behavior", testBootstrapLockBehavior);
}

void main().catch(error => {
    console.error(error);
    process.exitCode = 1;
});
