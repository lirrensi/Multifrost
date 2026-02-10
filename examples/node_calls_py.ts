import { ParentWorker } from "../javascript/src/multifrost";

async function main() {
    const worker = new ParentWorker(`../examples/math_worker.py`, "python");
    await worker.start();

    // will run first | blocking
    worker.call.fibonacci(55).then(result => {
        console.log("fibo number => ", result);
    });

    const fact = await worker.call.factorial(15);
    console.log("factorial => ", fact);

    await worker.stop();
}
main().catch(console.error);
