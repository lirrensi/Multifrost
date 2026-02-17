Multifrost IPC Protocol
=======================

Status: Canonical spec derived from Python + JavaScript implementations.
Scope: Defines wire protocol, transport, lifecycle, and behavioral contracts for adapters.
Date: 2026-02-11

1. Overview
-----------

1.1 This protocol defines inter-process RPC over ZeroMQ using msgpack encoding.
1.2 Two roles exist: Parent (controller) and Child (worker).
1.3 Parent sends CALL messages; Child returns RESPONSE or ERROR.
1.4 Child may emit STDOUT/STDERR messages (Python does; JS does not forward yet).
1.5 Two lifecycle modes exist: Spawn mode and Connect mode.
1.6 Protocol app identifier is a fixed string, used for basic validation.
1.7 This document uses RFC 2119 keywords: MUST, SHOULD, MAY.

2. Terminology
--------------

2.1 Parent: Process that owns or connects to a worker and issues calls.
2.2 Child: Worker process that exposes callable methods.
2.3 Spawn Mode: Parent spawns Child and binds a port; Child connects.
2.4 Connect Mode: Child registers a service ID; Parent discovers and connects.
2.5 Registry: File-based service discovery database under the user's home directory.
2.6 Message: Msgpack-encoded map with mandatory core fields.
2.7 Envelope: ZeroMQ multipart framing around the message payload.
2.8 Namespace: String used to route messages to a specific Child namespace.
2.9 Client Name: Optional identifier for service targeting.
2.10 Pending Request: Parent-side state keyed by message id awaiting response.

3. Transport
------------

3.1 Transport is ZeroMQ over TCP.
3.2 Socket pattern is ROUTER (Child) <-> DEALER (Parent).
3.3 Parent in Spawn mode binds to tcp://*:<port>; Child connects to tcp://localhost:<port>.
3.4 Parent in Connect mode connects to tcp://localhost:<port>; Child binds to tcp://*:<port>.
3.5 Parent sends multipart: [empty_frame, message_bytes].
3.6 Child receives multipart: [sender_id, empty_frame, message_bytes].
3.7 Child responds with multipart: [sender_id, empty_frame, message_bytes].
3.8 Timeouts are set on sockets: RCVTIMEO=100ms, SNDTIMEO=100ms in Python.
3.9 The JavaScript implementation does not set explicit RCV/SND timeouts on the socket.

4. Encoding
-----------

4.1 Payload encoding is msgpack (Python: msgpack; JS: msgpackr).
4.2 Payload MUST be a map/dict at top level.
4.3 Payload MUST include core fields: app, id, type, timestamp.
4.4 Unknown fields MUST be ignored by receivers.
4.5 Message bytes MUST be valid msgpack; invalid payloads are discarded.

5. Core Fields
--------------

5.1 app: string, fixed to 'comlink_ipc_v4'.
5.2 id: string, UUID v4 recommended.
5.3 type: string, one of call|response|error|stdout|stderr|heartbeat|shutdown.
5.4 timestamp: number, seconds since Unix epoch (float).

6. Message Types
----------------

6.1 CALL
6.1.1 Required fields: function, args (can be empty), namespace (default 'default').
6.1.2 Optional fields: client_name.
6.1.3 function MUST be a string, the method name exposed by Child.
6.1.4 args MUST be an array/list of positional arguments.
6.1.5 namespace SHOULD be a string; Child ignores mismatched namespace.
6.1.6 Child MUST reject methods starting with '_' (private).

6.2 RESPONSE
6.2.1 Required fields: result.
6.2.2 id MUST match the CALL id.
6.2.3 result MAY be any msgpack-serializable value.

6.3 ERROR
6.3.1 Required fields: error (string).
6.3.2 id MUST match the CALL id.
6.3.3 error SHOULD include message and optionally stack/traceback.

6.4 STDOUT
6.4.1 Required fields: output.
6.4.2 id is not required by JS, but present in Python output messages.
6.4.3 Parent SHOULD log the output with worker name context.

6.5 STDERR
6.5.1 Required fields: output.
6.5.2 Parent SHOULD log to stderr with worker name context.

6.6 HEARTBEAT
6.6.1 Reserved for future; no behavioral contract in current canon.
6.7 SHUTDOWN
6.7.1 Child SHOULD stop processing when receiving shutdown.

7. Message Schema (Normative)
-----------------------------

7.1 Base schema:
7.1.1 app: string (MUST be 'comlink_ipc_v4').
7.1.2 id: string (MUST be unique per request/response pair).
7.1.3 type: string (see Message Types).
7.1.4 timestamp: number (seconds since epoch).
7.2 CALL fields:
7.2.1 function: string (MUST be callable on Child).
7.2.2 args: array (default empty).
7.2.3 namespace: string (default 'default').
7.2.4 client_name: string (optional).
7.3 RESPONSE fields:
7.3.1 result: any (optional in practice but SHOULD be present).
7.4 ERROR fields:
7.4.1 error: string (MUST be present).
7.5 OUTPUT fields:
7.5.1 output: string (MUST be present).

8. Parent Behavior
------------------

8.1 Parent MUST create and bind/connect a DEALER socket before sending calls.
8.2 Parent MUST track pending requests keyed by id.
8.3 Parent MUST resolve pending request on RESPONSE.
8.4 Parent MUST reject pending request on ERROR with RemoteCallError.
8.5 Parent SHOULD remove pending request on timeout.
8.6 Parent SHOULD log STDOUT and STDERR with a prefix.
8.7 Parent in spawn mode MUST set COMLINK_ZMQ_PORT in child env.
8.8 Python Parent passes '--worker' argument to child script.
8.9 JS Parent spawns via shell and does NOT pass '--worker'.
8.10 Parent MAY support auto-restart (Python has optional auto_restart).

9. Child Behavior
-----------------

9.1 Child MUST create a ROUTER socket.
9.2 Child MUST ignore messages where app != comlink_ipc_v4.
9.3 Child MUST ignore messages with mismatched namespace.
9.4 Child MUST validate function and id fields in CALL.
9.5 Child MUST reject calls to missing or non-callable functions.
9.6 Child MUST reject calls to private methods (leading underscore).
9.7 Child SHOULD send ERROR with stack/traceback (Python does).
9.8 Child SHOULD continue loop after handling errors.
9.9 Child SHOULD handle SHUTDOWN by stopping loop.

10. Lifecycle Modes
-------------------

10.1 Spawn Mode
10.1.1 Parent binds a port and spawns Child.
10.1.2 Parent sets COMLINK_ZMQ_PORT env var.
10.1.3 Child detects COMLINK_ZMQ_PORT and connects to parent.
10.1.4 Python Child expects COMLINK_WORKER_MODE=1 as a hint.
10.1.5 Child MUST exit if port env is invalid or outside 1024-65535.
10.2 Connect Mode
10.2.1 Child registers service_id, binds to allocated port.
10.2.2 Parent discovers service_id to get port.
10.2.3 Registry ensures uniqueness and PID liveness.

11. Service Registry
--------------------

11.1 Registry path: ~/.multifrost/services.json.
11.2 Lock path: ~/.multifrost/registry.lock (Python).
11.3 Registry is JSON map: service_id -> {port, pid, started}.
11.4 Registration checks if existing PID is alive.
11.5 If alive, registration fails with error.
11.6 If not alive, entry is overwritten.
11.7 Unregister removes entry only if PID matches.
11.8 Python uses file locking (fcntl/msvcrt).
11.9 JS uses proper-lockfile for locking.
11.10 JS PID check uses pidusage; Python uses psutil if installed else os.kill(0).

12. Timeouts and Retries
------------------------

12.1 Parent CALL in Python supports timeout seconds; raises TimeoutError.
12.2 Parent CALL in JS supports timeout ms; rejects Promise on timeout.
12.3 Python send retries up to 5 on zmq.Again.
12.4 JS send does not retry; it rejects on send error.
12.5 Child send retries once in Python on zmq.Again after 1ms sleep.
12.6 Child send in JS does not retry.

13. Error Semantics
-------------------

13.1 Errors are delivered as ERROR messages with error string.
13.2 Python includes traceback string appended to the message.
13.3 JS error only includes the error message string.
13.4 Parent maps ERROR to RemoteCallError.
13.5 If parent receives unknown id, it ignores the response.

14. STDOUT/STDERR Semantics
---------------------------

14.1 Python Child replaces sys.stdout and sys.stderr with ZMQ writers.
14.2 Python sends STDOUT/STDERR messages without ROUTER envelope context (uses same socket).
14.3 JS Child does not forward console output over ZMQ yet.
14.4 Parent logs stdout and stderr with prefix: [script_name STDOUT]: message.
14.5 Output messages are best-effort and may be dropped under load.

15. Namespace and Routing
-------------------------

15.1 namespace defaults to 'default'.
15.2 Child ignores CALLs with mismatched namespace.
15.3 Parent MAY pass namespace via per-call options.
15.4 client_name exists in schema but not used by core routing today.

16. Concurrency and Ordering
----------------------------

16.1 Parent may issue concurrent calls; pending requests are keyed by id.
16.2 Responses may be received out of order; id matching resolves them.
16.3 Child processes one message at a time in its loop.
16.4 Child is single-threaded in JS; Python uses blocking loop with sleeps.

17. Compatibility Notes (Python vs JS)
--------------------------------------

17.1 App name constant is identical: comlink_ipc_v4.
17.2 Core fields match: app, id, type, timestamp.
17.3 CALL fields match: function, args, namespace, client_name.
17.4 ERROR payloads differ (Python includes traceback).
17.5 STDOUT/STDERR delivery differs (Python sends; JS logs locally only).
17.6 Spawn child argv differs (Python adds --worker; JS does not).
17.7 Python Child in spawn mode expects COMLINK_WORKER_MODE=1 but does not enforce it.
17.8 JS Child logs a DEBUG line on connect in spawn mode.
17.9 JS Parent uses shell spawning; Python Parent uses direct subprocess.
17.10 Both implementations ignore unknown message types by default.

18. Security Considerations
---------------------------

18.1 Protocol is unauthenticated and unencrypted.
18.2 Only use on localhost or trusted networks.
18.3 Do not expose open ports to untrusted clients.
18.4 Validate inputs in worker methods; args are untrusted.

19. Versioning
--------------

19.1 This protocol corresponds to application id comlink_ipc_v4.
19.2 Breaking wire changes MUST change app id or be negotiated out of band.

20. Canonical Examples
----------------------

20.1 CALL message (JSON-equivalent):
20.1.1 {app:'comlink_ipc_v4', id:'<uuid>', type:'call', timestamp:1234.5, function:'add', args:[1,2], namespace:'default'}
20.2 RESPONSE message:
20.2.1 {app:'comlink_ipc_v4', id:'<uuid>', type:'response', timestamp:1234.6, result:3}
20.3 ERROR message:
20.3.1 {app:'comlink_ipc_v4', id:'<uuid>', type:'error', timestamp:1234.6, error:'ValueError: ...'}

21. Implementation Checklist (Adapters)
---------------------------------------

21.1 Use ROUTER/DEALER with the exact multipart frames.
21.2 Msgpack encode/decode with map top-level.
21.3 Enforce app id == comlink_ipc_v4.
21.4 Generate UUIDv4 ids for calls.
21.5 Support namespace filtering.
21.6 Reject private methods starting with '_'.
21.7 Return ERROR for missing function or invalid args.
21.8 Handle concurrent calls with id-based correlation.
21.9 Implement registry for connect mode.
21.10 Do not crash on unknown fields.

22. Field-by-Field Constraints
------------------------------

22.1 Field=app; Type=string; Constraint=MUST equal comlink_ipc_v4
22.2 Field=id; Type=string; Constraint=MUST be unique and non-empty
22.3 Field=type; Type=string; Constraint=MUST be valid message type
22.4 Field=timestamp; Type=number; Constraint=MUST be seconds since epoch
22.5 Field=function; Type=string; Constraint=MUST be present for CALL
22.6 Field=args; Type=array; Constraint=MUST be present for CALL; empty allowed
22.7 Field=namespace; Type=string; Constraint=OPTIONAL; default 'default'
22.8 Field=client_name; Type=string; Constraint=OPTIONAL; reserved
22.9 Field=result; Type=any; Constraint=MUST be present for RESPONSE
22.10 Field=error; Type=string; Constraint=MUST be present for ERROR
22.11 Field=output; Type=string; Constraint=MUST be present for STDOUT/STDERR

23. Detailed Flow (Spawn Mode)
------------------------------

23.1 Parent allocates free port.
23.2 Parent binds DEALER to tcp://*:<port>.
23.3 Parent spawns child with COMLINK_ZMQ_PORT env.
23.4 Child creates ROUTER and connects to tcp://localhost:<port>.
23.5 Parent starts message loop.
23.6 Parent sends CALL [empty_frame, payload].
23.7 Child receives [sender_id, empty_frame, payload].
23.8 Child processes and responds [sender_id, empty_frame, payload].
23.9 Parent matches id and resolves/rejects pending request.
23.10 Parent stop closes socket and terminates child.

24. Detailed Flow (Connect Mode)
--------------------------------

24.1 Child registers service_id in registry with PID and port.
24.2 Child binds ROUTER to tcp://*:<port>.
24.3 Parent discovers service_id and gets port.
24.4 Parent connects DEALER to tcp://localhost:<port>.
24.5 Parent sends CALL; Child responds.
24.6 Child unregisters on stop (if PID matches).

25. Error Handling Matrix
-------------------------

25.1 Missing function field -> ERROR: 'Message missing function field' (Python) or similar (JS).
25.2 Missing id field -> ERROR: 'Message missing id field'.
25.3 Function not found -> ERROR: 'Function <name> not found'.
25.4 Function not callable -> ERROR: '<name> is not callable'.
25.5 Private method -> ERROR: 'Cannot call private method <name>'.
25.6 Invalid app id -> message ignored.
25.7 Namespace mismatch -> message ignored.
25.8 Timeout -> Parent raises TimeoutError or rejects Promise.

26. Adapter Compliance Tests
----------------------------

26.1 Each adapter SHOULD implement tests for each message type.
26.2 Test CALL/RESPONSE with primitive types and nested structures.
26.3 Test ERROR propagation and wrapping in RemoteCallError.
26.4 Test namespace filtering.
26.5 Test registry connect mode.
26.6 Test spawn mode with env var.
26.7 Test timeout behavior.

27. Extended Examples (Pseudo)
------------------------------

27.1 Parent sends CALL
27.1.1 envelope: [empty, msgpack(payload)]
27.1.2 payload fields: app,id,type=function,etc.
27.2 Child receives and responds
27.2.1 envelope: [sender_id, empty, msgpack(payload)]
27.2.2 response type: response or error


30. Detailed Field Notes
------------------------

30.1 timestamp is float seconds in Python; JS uses Date.now()/1000.
30.2 args is a tuple in Python but packed as list via msgpack.
30.3 namespace defaults to 'default' if omitted.
30.4 client_name is present but unused in routing.
30.5 output messages in Python do not include id; JS logs locally only.
30.6 Parent ignores messages with missing id for response/error.

31. Socket Options
------------------

31.1 Python Child sets LINGER=1000 and SNDTIMEO=100.
31.2 Python Parent sets RCVTIMEO=100 and SNDTIMEO=100.
31.3 JS implementations rely on defaults.

32. Shutdown
------------

32.1 Parent stop closes socket and terminates child (spawn mode).
32.2 Child stop closes socket and unregisters service.
32.3 SHUTDOWN message type is defined but rarely used by Parent.

33. Backward/Forward Compatibility
----------------------------------

33.1 Receivers MUST ignore unknown fields.
33.2 Receivers SHOULD ignore unknown message types.
33.3 New fields MUST be optional or negotiated.

34. Adapter Conformance Checklist (Expanded)
--------------------------------------------

34.1 Validate payload is msgpack map.
34.2 Reject empty or non-bytes payloads.
34.3 Ensure app id matches.
34.4 Ignore messages lacking id unless type is stdout/stderr.
34.5 Preserve id across response/error.
34.6 Support at least CALL/RESPONSE/ERROR.
34.7 Support namespace filtering.
34.8 Implement service registry if connect mode is supported.
34.9 Use ROUTER/DEALER framing exactly.
34.10 Do not block event loop for long operations in Child handlers.

35. Test Vectors
----------------

35.1 Vector A: CALL add(1,2)
35.1.1 Expected RESPONSE result=3
35.2 Vector B: CALL missing function
35.2.1 Expected ERROR with message mentioning 'function' missing
35.3 Vector C: CALL to _private
35.3.1 Expected ERROR 'Cannot call private method'

36. Large Payloads
------------------

36.1 Msgpack payload size is bounded by memory and ZMQ limits.
36.2 Adapters SHOULD avoid extremely large stdout/stderr messages.
36.3 No chunking is defined in the current protocol.

37. Known Divergences
---------------------

37.1 Python Child forwards stdout/stderr; JS Child does not.
37.2 Python Parent adds '--worker' arg; JS Parent does not.
37.3 Python Child validates port range; JS Child validates port range too.
37.4 Python ERROR includes traceback; JS ERROR includes only message.
37.5 Python send has retry loop; JS send is single attempt.

38. Suggested Extensions (Non-normative)
----------------------------------------

38.1 Implement heartbeat to detect stale connections.
38.2 Add request cancellation support.
38.3 Add streaming responses.
38.4 Add authentication layer.

39. Appendix: Registry File Example
-----------------------------------

39.1 Example JSON:
39.1.1 {
39.1.2   "math-service": {"port": 5555, "pid": 12345, "started": "2026-02-11T12:00:00"}
39.1.3 }

40. Appendix: Python API Surface
--------------------------------

40.1 ParentWorker.spawn(script_path, executable=python)
40.2 ParentWorker.connect(service_id, timeout=5.0)
40.3 worker.handle() -> async handle
40.4 worker.handle_sync() -> sync handle
40.5 handle.start(), handle.stop()
40.6 handle.call.<method>(*args)
40.7 ChildWorker(service_id=None)
40.8 ChildWorker.run(), list_functions()

41. Appendix: JS API Surface
----------------------------

41.1 ParentWorker.spawn(scriptPath, executable='node')
41.2 ParentWorker.connect(serviceId, timeout=5000)
41.3 worker.handle() -> async handle
41.4 handle.start(), handle.stop()
41.5 handle.call.<method>(*args)
41.6 ChildWorker(serviceId?)
41.7 ChildWorker.run(), listFunctions()

42. Appendix: Message Construction
----------------------------------

42.1 Python uses uuid.uuid4 for id.
42.2 JS uses crypto.randomUUID for id.
42.3 Python uses time.time for timestamp.
42.4 JS uses Date.now()/1000 for timestamp.