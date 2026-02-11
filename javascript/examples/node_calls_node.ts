import { ParentWorker } from "../src/multifrost";

async function main() {
    const worker = new ParentWorker(`../examples/math_worker.ts`, "tsx");
    await worker.start();

    // will run first | blocking
    worker.call.fibonacci(11).then(result => {
        console.log("fibo number => ", result);
    });

    const fact = await worker.call.factorial(11);
    console.log("factorial => ", fact);

    await worker.stop();
}
main().catch(console.error);
