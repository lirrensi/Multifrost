/**
 * Example: Multiple parents connecting to a single worker.
 * 
 * This demonstrates the ROUTER/DEALER architecture where multiple
 * parent processes can connect to and call functions on a single
 * worker service.
 * 
 * Usage:
 * 1. First, run math_worker_service.ts in a separate terminal
 * 2. Then run this script to see multiple parents connecting
 * 
 * npx ts-node examples/math_worker_service.ts  # Terminal 1
 * npx ts-node examples/parent_connects.ts      # Terminal 2
 */

import { ParentWorker } from "../src/multifrost.js";


async function parentTask(parentId: number): Promise<void> {
    console.log(`Parent ${parentId}: Connecting to math-service...`);

    try {
        const worker = await ParentWorker.connect("math-service", 5000);
        const handle = await worker.handle();
        console.log(`Parent ${parentId}: Connected!`);

        for (let i = 0; i < 3; i++) {
            const result = await handle.call.add(parentId * 10, i);
            console.log(`Parent ${parentId}: ${parentId * 10} + ${i} = ${result}`);
            await new Promise(resolve => setTimeout(resolve, 500));
        }

        await handle.stop();
        console.log(`Parent ${parentId}: Done`);
    } catch (e) {
        console.error(`Parent ${parentId}: Error - ${e}`);
    }
}


async function main(): Promise<void> {
    console.log("Starting multiple parents connecting to math-service...");
    console.log("Make sure math_worker_service.ts is running!\n");

    // Run 3 parents concurrently
    await Promise.all([
        parentTask(1),
        parentTask(2),
        parentTask(3),
    ]);

    console.log("\nAll parents finished!");
}


main();
