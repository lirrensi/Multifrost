"""
Message handling for ZeroMQ communication.
"""

import uuid
import time
import math
import msgpack
from enum import Enum
from typing import Any, Optional, Dict


class MessageType(Enum):
    """Standardized message types for ZeroMQ communication."""

    CALL = "call"
    RESPONSE = "response"
    ERROR = "error"
    STDOUT = "stdout"
    STDERR = "stderr"
    HEARTBEAT = "heartbeat"
    SHUTDOWN = "shutdown"


APP_NAME = "comlink_ipc_v3"


def _sanitize_for_msgpack(obj: Any) -> Any:
    """Sanitize object for msgpack interop safety across languages.

    Handles:
    - NaN/Infinity → null
    - Integer overflow → clamp to int64
    - Non-string keys → string conversion
    - Binary data → proper binary type
    """
    INT64_MAX = 2**63 - 1
    INT64_MIN = -(2**63)

    if isinstance(obj, dict):
        return {str(k): _sanitize_for_msgpack(v) for k, v in obj.items()}
    elif isinstance(obj, list):
        return [_sanitize_for_msgpack(v) for v in obj]
    elif isinstance(obj, float):
        if math.isnan(obj) or math.isinf(obj):
            return None
        return obj
    elif isinstance(obj, int):
        return max(INT64_MIN, min(INT64_MAX, obj))
    return obj


def _filter_none(obj: Any) -> Any:
    """Remove None values from dict for optional field handling."""
    if isinstance(obj, dict):
        return {k: v for k, v in obj.items() if v is not None}
    elif isinstance(obj, list):
        return [_filter_none(v) for v in obj]
    return obj


class ComlinkMessage:
    """
    Message container for ZeroMQ communication.

    Core fields (always present):
    - app: str = APP_NAME
    - id: str = unique UUID
    - type: str = "call"|"response"|"error"|"stdout"|"stderr"|"heartbeat"|"shutdown"
    - timestamp: float = unix timestamp
    - metadata: dict = optional metadata for tracing, timing, context

    Call message fields:
    - function: str = function name to call
    - args: List = positional arguments
    - namespace: str = namespace (default: 'default')
    - client_name: str = optional client identifier for service targeting

    Response message fields:
    - result: Any = return value from function

    Error message fields:
    - error: str = error message/traceback

    Output message fields:
    - output: str = stdout/stderr content

    Metadata fields (optional):
    - correlation_id: str = for distributed tracing across services
    - parent_request_id: str = for nested call chains
    - timestamp_sent: float = when request was sent
    - timestamp_received: float = when response was received
    - custom: dict = user-defined metadata
    """

    def __init__(self, **kwargs):
        # Set defaults
        self.app = APP_NAME
        self.id = str(uuid.uuid4())
        self.type = None
        self.timestamp = time.time()
        self.client_name = None
        self.metadata: Dict[str, Any] = {}

        # Override with any provided values
        for key, value in kwargs.items():
            setattr(self, key, value)

    @classmethod
    def create_call(
        cls,
        function: str,
        args: tuple = (),
        namespace: str = "default",
        msg_id: Optional[str] = None,
        client_name: Optional[str] = None,
        correlation_id: Optional[str] = None,
        parent_request_id: Optional[str] = None,
        metadata: Optional[Dict[str, Any]] = None,
    ):
        """Create a function call message with optional tracing metadata."""
        msg_metadata: Dict[str, Any] = metadata.copy() if metadata else {}
        msg_metadata["timestamp_sent"] = time.time()

        if correlation_id:
            msg_metadata["correlation_id"] = correlation_id
        if parent_request_id:
            msg_metadata["parent_request_id"] = parent_request_id

        return cls(
            type=MessageType.CALL.value,
            id=msg_id or str(uuid.uuid4()),
            function=function,
            args=args,
            namespace=namespace,
            client_name=client_name,
            metadata=msg_metadata,
        )

    @classmethod
    def create_response(
        cls,
        result: Any,
        msg_id: str,
        metadata: Optional[Dict[str, Any]] = None,
    ):
        """Create a response message with optional metadata."""
        msg_metadata: Dict[str, Any] = metadata.copy() if metadata else {}
        msg_metadata["timestamp_received"] = time.time()
        return cls(
            type=MessageType.RESPONSE.value,
            id=msg_id,
            result=result,
            metadata=msg_metadata,
        )

    @classmethod
    def create_error(
        cls,
        error: str,
        msg_id: str,
        metadata: Optional[Dict[str, Any]] = None,
    ):
        """Create an error message with optional metadata."""
        msg_metadata: Dict[str, Any] = metadata.copy() if metadata else {}
        msg_metadata["timestamp_received"] = time.time()
        return cls(
            type=MessageType.ERROR.value,
            id=msg_id,
            error=error,
            metadata=msg_metadata,
        )

    @classmethod
    def create_output(cls, output: str, msg_type: MessageType):
        """Create stdout/stderr output message."""
        return cls(type=msg_type.value, output=output)

    @classmethod
    def create_heartbeat(
        cls,
        msg_id: Optional[str] = None,
        timestamp: Optional[float] = None,
        is_response: bool = False,
    ):
        """
        Create a heartbeat message.

        Args:
            msg_id: Optional message ID (auto-generated if not provided)
            timestamp: Timestamp to include (defaults to now)
            is_response: If True, this is a heartbeat response

        Returns:
            ComlinkMessage with type "heartbeat"
        """
        msg_metadata = {"hb_timestamp": timestamp or time.time()}
        return cls(
            type=MessageType.HEARTBEAT.value,
            id=msg_id or str(uuid.uuid4()),
            metadata=msg_metadata,
        )

    @classmethod
    def create_heartbeat_response(cls, request_id: str, original_timestamp: float):
        """
        Create a heartbeat response message.

        Args:
            request_id: The ID of the heartbeat request
            original_timestamp: The timestamp from the request (for RTT calculation)

        Returns:
            ComlinkMessage with type "heartbeat" (response)
        """
        return cls(
            type=MessageType.HEARTBEAT.value,
            id=request_id,
            metadata={
                "hb_timestamp": original_timestamp,
                "hb_response": True,
            },
        )

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
        """Pack message for ZeroMQ transmission with interop safety."""
        data = self.to_dict()
        sanitized = _sanitize_for_msgpack(data)
        filtered = _filter_none(sanitized)
        return msgpack.packb(filtered, use_bin_type=True)

    @classmethod
    def unpack(cls, data: bytes) -> "ComlinkMessage":
        """Unpack message from ZeroMQ transmission with validation."""
        try:
            # Validate input
            if not isinstance(data, bytes):
                raise ValueError(f"Expected bytes, got {type(data)}")

            if len(data) == 0:
                raise ValueError("Empty message data")

            # Unpack with size limits (DoS protection)
            unpacked = msgpack.unpackb(
                data,
                raw=False,
                max_bin_len=10 * 1024 * 1024,
                max_str_len=10 * 1024 * 1024,
                max_array_len=100_000,
                max_map_len=100_000,
            )

            # Validate it's a dict
            if not isinstance(unpacked, dict):
                raise ValueError(f"Expected dict from msgpack, got {type(unpacked)}")

            # Create message object
            return cls(**unpacked)

        except msgpack.exceptions.ExtraData as e:
            raise ValueError(f"Message contains extra data: {e}")
        except msgpack.exceptions.UnpackException as e:
            raise ValueError(f"Failed to unpack message: {e}")
        except Exception as e:
            raise ValueError(f"Failed to create ComlinkMessage: {e}")
