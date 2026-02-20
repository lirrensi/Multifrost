/**
 * Quick debug test - JavaScript Parent -> JavaScript Child
 */

import { ParentWorker } from "../javascript/src/multifrost.js";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const E2E_DIR = join(__dirname, "e2e", "workers");
const JS_WORKER = join(E2E_DIR, "math_worker.ts");

async function main() {
    console.log("=== Quick E2E Test ===");
    console.log("Worker path:", JS_WORKER);

    const worker = ParentWorker.spawn(JS_WORKER, "npx tsx");
    console.log("Worker created, starting...");

    await worker.start();
    console.log("Started, waiting...");

    await new Promise(r => setTimeout(r, 2000));
    console.log("Calling add(10, 20)...");

    const result = await worker.call.add(10, 20);
    console.log("Result:", result);

    await worker.stop();
    console.log("DONE!");
}

main().catch(e => {
    console.error("Error:", e.message);
    console.error(e.stack);
    process.exit(1);
});