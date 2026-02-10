/**
 * Math worker that runs as a standalone microservice.
 * 
 * Run this first, then run parent_connects.ts to connect to it.
 * 
 * npx ts-node examples/math_worker_service.ts
 */

import { ChildWorker } from "../javascript/src/multifrost.js";


class MathWorker extends ChildWorker {
    constructor() {
        // Register with service_id for connect mode
        super("math-service");
    }

    add(a: number, b: number): number {
        console.error(`Adding ${a} + ${b}`);
        return a + b;
    }

    multiply(a: number, b: number): number {
        console.error(`Multiplying ${a} * ${b}`);
        return a * b;
    }

    power(base: number, exp: number): number {
        console.error(`Computing ${base}^${exp}`);
        return Math.pow(base, exp);
    }
}


console.error("Starting MathWorker as microservice...");
const worker = new MathWorker();
worker.run();
