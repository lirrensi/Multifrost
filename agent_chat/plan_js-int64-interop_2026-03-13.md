# Plan: JavaScript Int64 Interop
_Make the JavaScript runtime obey the documented numeric contract for signed int64 values, reject unsafe native-number inputs before packing, and verify the change with unit and end-to-end tests._

---

# Checklist
- [x] Step 1: Inspect current JavaScript MessagePack entry points
- [x] Step 2: Replace default msgpackr pack/unpack calls with explicit packer instances
- [x] Step 3: Rewrite numeric sanitization for the documented contract
- [x] Step 4: Add unit tests for int64 boundaries and rejection paths
- [x] Step 5: Add end-to-end tests for JavaScript parent to Python child int64 round-trips
- [x] Step 6: Run targeted JavaScript unit tests
- [ ] Step 7: Run cross-language end-to-end tests

---

## Context

- Repository root: `C:\Users\rx\001_Code\100_M\Multifrost`
- Main JavaScript implementation file: `javascript/src/multifrost.ts`
- JavaScript message serialization tests: `javascript/tests/message.test.ts`
- JavaScript parent to Python child smoke tests: `javascript/tests/e2e_minimal.test.ts`
- Cross-language end-to-end suite: `e2e/test_e2e.ts`
- Python worker used by end-to-end tests: `e2e/workers/math_worker.py`
- The current problem is in `javascript/src/multifrost.ts`: `ComlinkMessage.pack()` uses `msgpack.encode(...)` with defaults and `ComlinkMessage.unpack()` uses `msgpack.decode(...)` with defaults. The current sanitizer only converts `NaN` and `Infinity` to `null` and does not enforce the signed int64 contract.

## Prerequisites

- `javascript/node_modules` must already exist. If `javascript/node_modules` is missing, stop and report that dependencies must be installed with `npm install` in `javascript` before continuing.
- `python` must be available on `PATH`.
- Running `python "C:\Users\rx\001_Code\100_M\Multifrost\e2e\workers\math_worker.py"` from an environment configured for this repo must be able to import `multifrost`. If that import fails, stop and report the Python environment issue. Do not continue to end-to-end tests.
- Do not change any docs in this plan. The docs were already updated and are now the target behavior.

## Scope Boundaries

- Do not modify `golang/`
- Do not modify `rust/`
- Do not modify `python/`
- Do not modify `docs/`
- Do not change restart, heartbeat, lifecycle, registry, or logging behavior in `javascript/src/multifrost.ts`

---

## Steps

### Step 1: Inspect current JavaScript MessagePack entry points

Open `javascript/src/multifrost.ts`. Confirm that `ComlinkMessage.pack()` calls `msgpack.encode(...)` and `ComlinkMessage.unpack()` calls `msgpack.decode(...)`. Confirm that the numeric sanitization logic lives in `sanitizeForMsgpack()` and `deepSanitize()` near the top of the file.

✅ Success: The exact functions and call sites are identified before editing `javascript/src/multifrost.ts`.
❌ If failed: Stop and report that `javascript/src/multifrost.ts` no longer matches the expected structure. Do not continue with this plan until the current structure is reviewed.

### Step 2: Replace default msgpackr pack/unpack calls with explicit packer instances

Edit `javascript/src/multifrost.ts`.

1. Replace the namespace import `import * as msgpack from "msgpackr";` with named imports that allow explicit instances.
2. Create one shared encoder instance and one shared decoder instance near `APP_NAME`.
3. Configure the encoder for standard cross-language maps only. Do not enable record extensions.
4. Configure the decoder so signed int64 values decode without silent precision loss. Use `int64AsType: "auto"` unless the current code structure makes `"bigint"` mandatory. Prefer `"auto"` because it preserves normal numbers for safe values and `bigint` for unsafe values.
5. Update `ComlinkMessage.pack()` to call the shared encoder instance.
6. Update `ComlinkMessage.unpack()` to call the shared decoder instance.

Do not change any other message framing or transport code in this step.

✅ Success: `javascript/src/multifrost.ts` uses explicit `msgpackr` packer/unpacker instances, and `ComlinkMessage.pack()` / `ComlinkMessage.unpack()` no longer rely on default module-level behavior.
❌ If failed: Revert only the partial `msgpackr` import and instance edits in `javascript/src/multifrost.ts`, save the file in a compilable state, and stop.

### Step 3: Rewrite numeric sanitization for the documented contract

Edit `javascript/src/multifrost.ts`.

1. Keep recursive traversal for arrays and plain objects.
2. Preserve the current `NaN` / `Infinity` / `-Infinity` normalization to `null`.
3. Add explicit handling for JavaScript `number` integers outside `Number.MIN_SAFE_INTEGER` to `Number.MAX_SAFE_INTEGER`.
4. When a caller passes an integer `number` outside the safe integer range, throw a `RangeError` with a message that tells the caller to use `bigint` or an explicit encoding such as a decimal string.
5. Add explicit handling for `bigint` values.
6. Permit `bigint` values only when they are inside signed int64 range `[-9223372036854775808n, 9223372036854775807n]`.
7. When a caller passes a `bigint` outside signed int64 range, throw a `RangeError` with a message that tells the caller to encode the value explicitly.
8. Leave finite non-integer `number` values untouched.
9. Do not silently clamp any integer values.

Keep the implementation small and local to the message serialization boundary. Do not add a new public helper API in this step.

✅ Success: The sanitizer enforces the documented contract: finite floats stay numeric, non-finite floats become `null`, safe native integers pass, signed-int64 `bigint` values pass, and out-of-contract integers throw instead of being silently changed.
❌ If failed: Remove the new rejection logic, restore a compilable version of `javascript/src/multifrost.ts`, and stop. Do not guess at fallback behavior.

### Step 4: Add unit tests for int64 boundaries and rejection paths

Edit `javascript/tests/message.test.ts`.

Add new test blocks that cover all of the following cases:

1. Packing and unpacking a CALL with `9223372036854775807n` preserves the exact value.
2. Packing and unpacking a CALL with `-9223372036854775808n` preserves the exact value.
3. Packing and unpacking a CALL with a normal safe integer such as `42` still returns a JavaScript `number`.
4. Packing a CALL with `Number.MAX_SAFE_INTEGER + 1` throws a `RangeError`.
5. Packing a CALL with `9223372036854775808n` throws a `RangeError`.
6. Existing `NaN` / `Infinity` normalization still returns `null` after round-trip.

Use the existing local test harness in `javascript/tests/message.test.ts`. Do not introduce a new test framework.

✅ Success: `javascript/tests/message.test.ts` contains direct assertions for the signed int64 boundaries and both rejection paths.
❌ If failed: Remove only the new test blocks from `javascript/tests/message.test.ts`, save the file in runnable state, and stop.

### Step 5: Add end-to-end tests for JavaScript parent to Python child int64 round-trips

Edit `javascript/tests/e2e_minimal.test.ts`.

Add one new test function dedicated to numeric boundary behavior for `ParentWorker.spawn(PYTHON_WORKER, "python")`.

Inside the new test function:

1. Start the Python worker using the same pattern as the existing tests.
2. Call `worker.call.echo(9223372036854775807n)` and assert that the response equals `9223372036854775807n`.
3. Call `worker.call.echo(-9223372036854775808n)` and assert that the response equals `-9223372036854775808n`.
4. Attempt `worker.call.echo(Number.MAX_SAFE_INTEGER + 1)` and assert that the call rejects locally with `RangeError` before transport.
5. Stop the worker in the existing `finally` pattern.

Add the new test function to `runAllTests()` in `javascript/tests/e2e_minimal.test.ts`.

Do not modify the Python worker in `e2e/workers/math_worker.py`; `echo()` already round-trips the value.

✅ Success: `javascript/tests/e2e_minimal.test.ts` has one explicit cross-language numeric-boundary test and `runAllTests()` executes it.
❌ If failed: Remove only the new test function and `runAllTests()` call from `javascript/tests/e2e_minimal.test.ts`, save the file in runnable state, and stop.

### Step 6: Run targeted JavaScript unit tests

From `C:\Users\rx\001_Code\100_M\Multifrost\javascript`, run these commands in order:

1. `npm run test:message`
2. `npm run typecheck`

Both commands must pass before continuing.

✅ Success: `npm run test:message` exits with code `0` and `npm run typecheck` exits with code `0`.
❌ If failed: Stop. Record the full failing command output. Do not continue to Step 7.

### Step 7: Run cross-language end-to-end tests

From `C:\Users\rx\001_Code\100_M\Multifrost\javascript`, run these commands in order:

1. `npx tsx tests/e2e_minimal.test.ts`
2. `npx tsx ../e2e/test_e2e.ts`

If the second command fails because of an unrelated pre-existing test outside the new numeric path, record that separately after confirming whether the new numeric assertions passed in `tests/e2e_minimal.test.ts`.

✅ Success: `npx tsx tests/e2e_minimal.test.ts` passes including the new bigint/int64 assertions. If `npx tsx ../e2e/test_e2e.ts` also passes, the full verification is complete.
❌ If failed: Stop. Record the exact failing command and the first failing assertion. Do not make unrelated fixes.

---

## Verification

The plan is complete only when all of the following are true:

- `javascript/src/multifrost.ts` uses explicit `msgpackr` encoder/decoder instances.
- `javascript/src/multifrost.ts` throws on out-of-contract native integer inputs instead of silently clamping or rounding.
- `javascript/tests/message.test.ts` proves signed int64 boundary round-trips and rejection behavior.
- `javascript/tests/e2e_minimal.test.ts` proves JavaScript parent to Python child round-trips for signed int64 boundary values.
- `npm run test:message` passes.
- `npm run typecheck` passes.
- `npx tsx tests/e2e_minimal.test.ts` passes.

If `npx tsx ../e2e/test_e2e.ts` fails, include the full failure in the handoff and explicitly state whether the failure is in the new numeric path or an unrelated pre-existing path.

## Rollback

If a critical failure cannot be resolved, restore only the JavaScript files touched by this plan using the git working tree state before execution:

1. Review the changed files with `git diff -- javascript/src/multifrost.ts javascript/tests/message.test.ts javascript/tests/e2e_minimal.test.ts`
2. Revert the three JavaScript files to the pre-plan state using a non-destructive file restore method approved for the repo workflow.
3. Leave `docs/` unchanged.
4. Report that the JavaScript int64 interop change was rolled back before merge.

Plan complete. Handing off to Executor.
