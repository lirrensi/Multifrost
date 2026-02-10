"""
Message handling for ZeroMQ communication.
"""

import uuid
import time
import msgpack
from enum import Enum
from typing import Any, Optional


class MessageType(Enum):
    """Standardized message types for ZeroMQ communication."""

    CALL = "call"
    RESPONSE = "response"
    ERROR = "error"
    STDOUT = "stdout"
    STDERR = "stderr"
    HEARTBEAT = "heartbeat"
    SHUTDOWN = "shutdown"


APP_NAME = "comlink_ipc_v2"


class ComlinkMessage:
    """
    Message container for ZeroMQ communication.

    Core fields (always present):
    - app: str = APP_NAME
    - id: str = unique UUID
    - type: str = "call"|"response"|"error"|"stdout"|"stderr"|"heartbeat"|"shutdown"
    - timestamp: float = unix timestamp

    Call message fields:
    - function: str = function name to call
    - args: List = positional arguments
    - namespace: str = namespace (default: 'default')

    Response message fields:
    - result: Any = return value from function

    Error message fields:
    - error: str = error message/traceback

    Output message fields:
    - output: str = stdout/stderr content
    """

    def __init__(self, **kwargs):
        # Set defaults
        self.app = APP_NAME
        self.id = str(uuid.uuid4())
        self.type = None
        self.timestamp = time.time()

        # Override with any provided values
        for key, value in kwargs.items():
            setattr(self, key, value)

    @classmethod
    def create_call(
        cls,
        function: str,
        args: tuple = (),
        namespace: str = "default",
        msg_id: str = None,
    ):
        """Create a function call message."""
        return cls(
            type=MessageType.CALL.value,
            id=msg_id or str(uuid.uuid4()),
            function=function,
            args=args,
            namespace=namespace,
        )

    @classmethod
    def create_response(cls, result: Any, msg_id: str):
        """Create a response message."""
        return cls(type=MessageType.RESPONSE.value, id=msg_id, result=result)

    @classmethod
    def create_error(cls, error: str, msg_id: str):
        """Create an error message."""
        return cls(type=MessageType.ERROR.value, id=msg_id, error=error)

    @classmethod
    def create_output(cls, output: str, msg_type: MessageType):
        """Create stdout/stderr output message."""
        return cls(type=msg_type.value, output=output)

    def extract_call_args(self):
        """Extract args from simple list format."""
        if hasattr(self, "args"):
            return tuple(self.args)
        return tuple()

    def to_dict(self) -> dict:
        """Convert to plain dict."""
        return {
            key: value
            for key, value in self.__dict__.items()
            if not key.startswith("_")
        }

    def pack(self) -> bytes:
        """Pack message for ZeroMQ transmission."""
        return msgpack.packb(self.to_dict())

    @classmethod
    def unpack(cls, data: bytes) -> "ComlinkMessage":
        """Unpack message from ZeroMQ transmission with validation."""
        try:
            # Validate input
            if not isinstance(data, bytes):
                raise ValueError(f"Expected bytes, got {type(data)}")

            if len(data) == 0:
                raise ValueError("Empty message data")

            # Unpack with msgpack
            unpacked = msgpack.unpackb(data, raw=False)

            # Validate it's a dict
            if not isinstance(unpacked, dict):
                raise ValueError(f"Expected dict from msgpack, got {type(unpacked)}")

            # Create message object
            return cls(**unpacked)

        except msgpack.exceptions.ExtraData as e:
            raise msgpack.exceptions.ExtraData(f"Message contains extra data: {e}")
        except msgpack.exceptions.UnpackException as e:
            raise msgpack.exceptions.UnpackException(f"Failed to unpack message: {e}")
        except Exception as e:
            raise ValueError(f"Failed to create ComlinkMessage: {e}")
