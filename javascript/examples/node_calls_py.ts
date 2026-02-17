import { ParentWorker } from "../src/multifrost";

async function main() {
    const worker = new ParentWorker(`../examples/math_worker.py`, "python");
    const handle = await worker.handle();

    // will run first | blocking
    handle.call.fibonacci(55).then(result => {
        console.log("fibo number => ", result);
    });

    const fact = await handle.call.factorial(15);
    console.log("factorial => ", fact);

    await handle.stop();
}
main().catch(console.error);
