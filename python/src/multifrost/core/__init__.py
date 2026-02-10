"""
Core modules for async-first IPC implementation.
"""

from .message import ComlinkMessage, MessageType, APP_NAME
from .async_worker import ParentWorker, RemoteCallError
from .child import ChildWorker
from .sync_wrapper import SyncWrapper, SyncProxy, AsyncProxy

__all__ = [
    "ComlinkMessage",
    "MessageType",
    "APP_NAME",
    "RemoteCallError",
    "ParentWorker",
    "ChildWorker",
    "SyncWrapper",
    "SyncProxy",
    "AsyncProxy",
]
