# Multifrost PHP FAQ

## Can PHP scripts be service peers?

Yes. Run a PHP script with `runService()` and it will register as a service
peer. The script blocks waiting for calls — suitable for CLI-based services
(`php myservice.php`). Not suitable for FPM/CGI contexts (those are
request-response only).

## Does this work behind nginx/apache/FPM?

The caller peer pattern works in any PHP context (CLI, FPM, mod_php). A script
handling an HTTP request can connect to the router, call a service, get the
result, and return it to the client. The connection is short-lived.

For service peers, you must use CLI (`php myservice.php`) since they are
long-running.

## Why blocking instead of async?

PHP's natural execution model is blocking/synchronous. The Go binding uses the
same pattern. Async would require an event loop (ReactPHP, Amp), adding
complexity for no benefit in the common use case.

## What PHP version is required?

PHP 8.2 or later (due to readonly classes and other typed property features).

## Why `rybakit/msgpack` instead of the PECL extension?

The pure PHP library is more portable (no C extension install), handles str/bin
distinction properly, and is actively maintained. The PECL extension doesn't
support bin/ext types well.

## Why `phrity/websocket` instead of `textalk/websocket`?

`textalk/websocket` was archived. `phrity/websocket` is the maintained successor
by the same developer, with the same blocking API.

## Does the router auto-start work on Windows?

Yes, but through PowerShell rather than PHP's `proc_open`. PHP's `proc_open` on
some Windows builds creates child processes where Winsock fails to initialise
(error 10106), causing the Rust router binary to crash immediately on socket
bind.

The binding detects Windows and uses PowerShell's `Start-Process` (which
delegates to .NET's `Process.Start`) to launch the router correctly. If
PowerShell is unavailable, the error message tells you how to start the router
manually.

See `docs/arch.md` (Windows Router Bootstrap section) for the full story.
