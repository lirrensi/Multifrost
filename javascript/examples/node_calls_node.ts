import { ParentWorker } from "../src/multifrost";

async function main() {
    const worker = new ParentWorker(`../examples/math_worker.ts`, "tsx");
    const handle = await worker.handle();

    // will run first | blocking
    handle.call.fibonacci(11).then(result => {
        console.log("fibo number => ", result);
    });

    const fact = await handle.call.factorial(11);
    console.log("factorial => ", fact);

    await handle.stop();
}
main().catch(console.error);
