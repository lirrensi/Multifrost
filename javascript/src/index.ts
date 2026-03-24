/**
 * FILE: javascript/src/index.ts
 * PURPOSE: Publish the canonical Node v5 API surface from a single package entrypoint.
 * OWNS: Public re-exports for caller, process, service, protocol, and error modules.
 * EXPORTS: connect, Connection, Handle, spawn, ServiceProcess, ServiceWorker, ServiceContext, runService, protocol constants, error classes
 * DOCS: docs/spec.md, agent_chat/node_v5_api_surface_2026-03-24.md
 */

export * from "./protocol.js";
export * from "./errors.js";
export { connect, Connection, Handle } from "./connection.js";
export type { ConnectOptions } from "./connection.js";
export { spawn, ServiceProcess } from "./process.js";
export type { SpawnOptions, ProcessExitStatus } from "./process.js";
export { ServiceWorker, ServiceContext, runService } from "./service.js";
export type { ServiceContextOptions } from "./service.js";
