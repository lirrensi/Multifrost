"""
FILE: python/src/multifrost/sync.py
PURPOSE: Provide the synchronous v5 caller convenience layer over the async handle.
OWNS: HandleSync and its background event-loop thread.
EXPORTS: HandleSync.
DOCS: docs/spec.md; agent_chat/python_v5_api_surface_2026-03-25.md
"""

from __future__ import annotations

import asyncio
import threading
from collections.abc import Callable, Coroutine
from concurrent.futures import Future as ConcurrentFuture
from typing import Any, TypeVar

from .connection import Connection, Handle
from .errors import TransportError
from .protocol import QueryGetResponseBody

T = TypeVar("T")


class _BackgroundLoop:
    def __init__(self) -> None:
        self._loop = asyncio.new_event_loop()
        self._ready = threading.Event()
        self._thread = threading.Thread(target=self._run, daemon=True)
        self._thread.start()
        self._ready.wait()

    def _run(self) -> None:
        asyncio.set_event_loop(self._loop)
        self._ready.set()
        self._loop.run_forever()
        pending = asyncio.all_tasks(self._loop)
        for task in pending:
            task.cancel()
        if pending:
            self._loop.run_until_complete(
                asyncio.gather(*pending, return_exceptions=True)
            )
        self._loop.close()

    def run(self, coro: Coroutine[Any, Any, T]) -> T:
        future: ConcurrentFuture[T] = asyncio.run_coroutine_threadsafe(coro, self._loop)
        return future.result()

    def stop(self) -> None:
        self._loop.call_soon_threadsafe(self._loop.stop)
        self._thread.join()


class _SyncCallNamespace:
    def __init__(self, owner: HandleSync) -> None:
        self._owner = owner

    def __getattr__(self, function_name: str) -> Callable[..., Any]:
        def invoke(*args: Any, namespace: str | None = None) -> Any:
            return self._owner._run(
                self._owner._handle._invoke_remote(
                    function_name,
                    list(args),
                    namespace=namespace,
                )
            )

        return invoke


class HandleSync:
    """Synchronous caller handle backed by one background event loop."""

    def __init__(self, connection: Connection) -> None:
        self._handle = Handle(connection)
        self._loop = _BackgroundLoop()
        self.call = _SyncCallNamespace(self)
        self._closed = False

    @property
    def peer_id(self) -> str:
        return self._handle.peer_id

    @property
    def target_peer_id(self) -> str:
        return self._handle.target_peer_id

    def start(self) -> None:
        self._run(self._handle.start())

    def stop(self) -> None:
        try:
            self._run(self._handle.stop())
        finally:
            if not self._closed:
                self._loop.stop()
                self._closed = True

    def query_peer_exists(self, peer_id: str) -> bool:
        return self._run(self._handle.query_peer_exists(peer_id))

    def query_peer_get(self, peer_id: str) -> QueryGetResponseBody:
        return self._run(self._handle.query_peer_get(peer_id))

    def _run(self, coro: Coroutine[Any, Any, T]) -> T:
        if self._closed:
            raise TransportError("HandleSync has been stopped")
        return self._loop.run(coro)

    def __enter__(self) -> HandleSync:
        self.start()
        return self

    def __exit__(self, exc_type: Any, exc: Any, tb: Any) -> None:
        self.stop()


__all__ = ["HandleSync"]
