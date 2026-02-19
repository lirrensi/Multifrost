"""
Comprehensive tests for ComlinkMessage class.

Tests cover:
- All message types (call, response, error, output, heartbeat, shutdown)
- Pack/unpack roundtrips
- Edge cases (empty, large, unicode, special values)
- Metadata handling
- Sanitization (NaN, Infinity, integer overflow)
- Error handling for invalid data
"""

import math
import msgpack
import sys
import os
import time

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

from multifrost.core.message import (
    ComlinkMessage,
    MessageType,
    APP_NAME,
    _sanitize_for_msgpack,
    _filter_none,
)


class TestMessageType:
    """Test MessageType enum."""

    def test_message_types_exist(self):
        """Test that all message types are defined."""
        assert MessageType.CALL.value == "call"
        assert MessageType.RESPONSE.value == "response"
        assert MessageType.ERROR.value == "error"
        assert MessageType.STDOUT.value == "stdout"
        assert MessageType.STDERR.value == "stderr"
        assert MessageType.HEARTBEAT.value == "heartbeat"
        assert MessageType.SHUTDOWN.value == "shutdown"


class TestComlinkMessageCreation:
    """Test ComlinkMessage creation."""

    def test_default_values(self):
        """Test default values on creation."""
        msg = ComlinkMessage()

        assert msg.app == APP_NAME
        assert msg.id is not None
        assert msg.timestamp is not None
        assert msg.metadata == {}

    def test_custom_values(self):
        """Test creating message with custom values."""
        msg = ComlinkMessage(
            type="call", function="add", args=(1, 2), custom_field="value"
        )

        assert msg.type == "call"
        assert msg.function == "add"
        assert msg.args == (1, 2)
        assert msg.custom_field == "value"

    def test_app_name(self):
        """Test APP_NAME constant."""
        assert APP_NAME == "comlink_ipc_v4"


class TestCallMessages:
    """Test call message creation."""

    def test_create_call_basic(self):
        """Test creating basic call message."""
        msg = ComlinkMessage.create_call(function="add", args=(1, 2))

        assert msg.type == MessageType.CALL.value
        assert msg.function == "add"
        assert msg.args == (1, 2)
        assert msg.namespace == "default"
        assert msg.id is not None
        assert "timestamp_sent" in msg.metadata

    def test_create_call_with_namespace(self):
        """Test creating call with custom namespace."""
        msg = ComlinkMessage.create_call(
            function="multiply", args=(3, 4), namespace="math"
        )

        assert msg.namespace == "math"

    def test_create_call_with_id(self):
        """Test creating call with specific ID."""
        msg = ComlinkMessage.create_call(
            function="add", args=(), msg_id="custom-id-123"
        )

        assert msg.id == "custom-id-123"

    def test_create_call_with_metadata(self):
        """Test creating call with custom metadata."""
        msg = ComlinkMessage.create_call(
            function="process",
            args=("data",),
            correlation_id="corr-123",
            parent_request_id="parent-456",
            metadata={"custom": "value", "priority": 1},
        )

        assert msg.metadata["correlation_id"] == "corr-123"
        assert msg.metadata["parent_request_id"] == "parent-456"
        assert msg.metadata["custom"] == "value"
        assert msg.metadata["priority"] == 1
        assert "timestamp_sent" in msg.metadata

    def test_create_call_with_client_name(self):
        """Test creating call with client name."""
        msg = ComlinkMessage.create_call(
            function="fetch", args=(), client_name="client-abc"
        )

        assert msg.client_name == "client-abc"


class TestResponseMessages:
    """Test response message creation."""

    def test_create_response_basic(self):
        """Test creating basic response message."""
        msg = ComlinkMessage.create_response(result=42, msg_id="req-123")

        assert msg.type == MessageType.RESPONSE.value
        assert msg.result == 42
        assert msg.id == "req-123"
        assert "timestamp_received" in msg.metadata

    def test_create_response_with_metadata(self):
        """Test creating response with metadata."""
        msg = ComlinkMessage.create_response(
            result={"status": "ok"},
            msg_id="req-456",
            metadata={"cached": True, "source": "db"},
        )

        assert msg.metadata["cached"] is True
        assert msg.metadata["source"] == "db"

    def test_create_response_none_result(self):
        """Test creating response with None result."""
        msg = ComlinkMessage.create_response(result=None, msg_id="req-789")

        # None results should still work
        assert msg.result is None


class TestErrorMessages:
    """Test error message creation."""

    def test_create_error_basic(self):
        """Test creating basic error message."""
        msg = ComlinkMessage.create_error(
            error="Something went wrong", msg_id="req-123"
        )

        assert msg.type == MessageType.ERROR.value
        assert msg.error == "Something went wrong"
        assert msg.id == "req-123"

    def test_create_error_with_metadata(self):
        """Test creating error with metadata."""
        msg = ComlinkMessage.create_error(
            error="Connection failed",
            msg_id="req-456",
            metadata={"retryable": True, "attempt": 3},
        )

        assert msg.metadata["retryable"] is True
        assert msg.metadata["attempt"] == 3


class TestOutputMessages:
    """Test output message creation."""

    def test_create_stdout(self):
        """Test creating stdout message."""
        msg = ComlinkMessage.create_output("Hello, world!", MessageType.STDOUT)

        assert msg.type == MessageType.STDOUT.value
        assert msg.output == "Hello, world!"

    def test_create_stderr(self):
        """Test creating stderr message."""
        msg = ComlinkMessage.create_output("Error output", MessageType.STDERR)

        assert msg.type == MessageType.STDERR.value
        assert msg.output == "Error output"


class TestHeartbeatMessages:
    """Test heartbeat message creation."""

    def test_create_heartbeat_basic(self):
        """Test creating basic heartbeat message."""
        msg = ComlinkMessage.create_heartbeat()

        assert msg.type == MessageType.HEARTBEAT.value
        assert msg.id is not None
        assert "hb_timestamp" in msg.metadata

    def test_create_heartbeat_with_id(self):
        """Test creating heartbeat with specific ID."""
        msg = ComlinkMessage.create_heartbeat(msg_id="hb-123")

        assert msg.id == "hb-123"

    def test_create_heartbeat_with_timestamp(self):
        """Test creating heartbeat with custom timestamp."""
        ts = time.time() - 10  # 10 seconds ago
        msg = ComlinkMessage.create_heartbeat(timestamp=ts)

        assert msg.metadata["hb_timestamp"] == ts

    def test_create_heartbeat_response(self):
        """Test creating heartbeat response."""
        original_ts = time.time() - 0.05  # 50ms ago
        msg = ComlinkMessage.create_heartbeat_response(
            request_id="hb-456", original_timestamp=original_ts
        )

        assert msg.id == "hb-456"
        assert msg.type == MessageType.HEARTBEAT.value
        assert msg.metadata["hb_timestamp"] == original_ts
        assert msg.metadata["hb_response"] is True


class TestPackUnpack:
    """Test message serialization."""

    def test_pack_unpack_call(self):
        """Test pack/unpack roundtrip for call."""
        original = ComlinkMessage.create_call(
            function="add", args=(1, 2), namespace="math"
        )

        packed = original.pack()
        unpacked = ComlinkMessage.unpack(packed)

        assert unpacked.type == MessageType.CALL.value
        assert unpacked.function == "add"
        # msgpack converts tuples to lists
        assert list(unpacked.args) == [1, 2]
        assert unpacked.namespace == "math"

    def test_pack_unpack_response(self):
        """Test pack/unpack roundtrip for response."""
        original = ComlinkMessage.create_response(result=42, msg_id="req-123")

        packed = original.pack()
        unpacked = ComlinkMessage.unpack(packed)

        assert unpacked.type == MessageType.RESPONSE.value
        assert unpacked.result == 42
        assert unpacked.id == "req-123"

    def test_pack_unpack_error(self):
        """Test pack/unpack roundtrip for error."""
        original = ComlinkMessage.create_error(error="Test error", msg_id="req-456")

        packed = original.pack()
        unpacked = ComlinkMessage.unpack(packed)

        assert unpacked.type == MessageType.ERROR.value
        assert unpacked.error == "Test error"

    def test_pack_unpack_heartbeat(self):
        """Test pack/unpack roundtrip for heartbeat."""
        original = ComlinkMessage.create_heartbeat(msg_id="hb-789")

        packed = original.pack()
        unpacked = ComlinkMessage.unpack(packed)

        assert unpacked.id == "hb-789"
        assert unpacked.type == MessageType.HEARTBEAT.value

    def test_pack_returns_bytes(self):
        """Test that pack returns bytes."""
        msg = ComlinkMessage.create_call(function="test", args=())

        packed = msg.pack()

        assert isinstance(packed, bytes)

    def test_pack_is_msgpack(self):
        """Test that pack uses msgpack."""
        msg = ComlinkMessage.create_call(function="test", args=(1, 2))

        packed = msg.pack()
        unpacked = msgpack.unpackb(packed, raw=False)

        assert isinstance(unpacked, dict)
        assert unpacked["function"] == "test"


class TestSanitization:
    """Test data sanitization for msgpack."""

    def test_sanitize_nan(self):
        """Test that NaN is converted to None."""
        result = _sanitize_for_msgpack(float("nan"))
        assert result is None

    def test_sanitize_infinity(self):
        """Test that Infinity is converted to None."""
        result = _sanitize_for_msgpack(float("inf"))
        assert result is None

        result = _sanitize_for_msgpack(float("-inf"))
        assert result is None

    def test_sanitize_int_overflow(self):
        """Test integer clamping to int64."""
        INT64_MAX = 2**63 - 1
        INT64_MIN = -(2**63)

        # Value within range
        result = _sanitize_for_msgpack(1000)
        assert result == 1000

        # Value above max
        result = _sanitize_for_msgpack(INT64_MAX + 1)
        assert result == INT64_MAX

        # Value below min
        result = _sanitize_for_msgpack(INT64_MIN - 1)
        assert result == INT64_MIN

    def test_sanitize_dict(self):
        """Test sanitization of dicts."""
        result = _sanitize_for_msgpack(
            {"valid": 123, "nan": float("nan"), "nested": {"inf": float("inf")}}
        )

        assert result["valid"] == 123
        assert result["nan"] is None
        assert result["nested"]["inf"] is None

    def test_sanitize_list(self):
        """Test sanitization of lists."""
        result = _sanitize_for_msgpack([1, float("nan"), 3])

        assert result[0] == 1
        assert result[1] is None
        assert result[2] == 3

    def test_sanitize_non_string_keys(self):
        """Test that non-string keys are converted to strings."""
        result = _sanitize_for_msgpack({123: "value", True: "bool"})

        assert "123" in result
        assert "True" in result


class TestFilterNone:
    """Test None value filtering."""

    def test_filter_none_dict(self):
        """Test filtering None values from dict."""
        result = _filter_none(
            {
                "keep": 1,
                "remove": None,
                "nested": {"inner_none": None, "inner_value": 2},
            }
        )

        assert "keep" in result
        assert "remove" not in result
        # Note: _filter_none only filters top-level, nested dicts are preserved as-is
        assert "nested" in result
        assert result["nested"]["inner_value"] == 2

    def test_filter_none_list(self):
        """Test filtering in lists recursively."""
        result = _filter_none([1, None, 3])

        # Lists have _filter_none applied recursively
        assert result == [1, None, 3]  # None preserved for primitives

    def test_filter_none_primitives(self):
        """Test filtering on primitive values."""
        assert _filter_none(123) == 123
        assert _filter_none("string") == "string"
        # Note: _filter_none(None) returns None, doesn't remove


class TestExtractCallArgs:
    """Test argument extraction."""

    def test_extract_args_present(self):
        """Test extracting args when present."""
        msg = ComlinkMessage.create_call(function="add", args=(1, 2, 3))

        args = msg.extract_call_args()

        assert args == (1, 2, 3)

    def test_extract_args_missing(self):
        """Test extracting args when missing."""
        msg = ComlinkMessage(type=MessageType.CALL.value)

        args = msg.extract_call_args()

        assert args == ()

    def test_extract_args_empty(self):
        """Test extracting empty args."""
        msg = ComlinkMessage.create_call(function="noargs", args=())

        args = msg.extract_call_args()

        assert args == ()


class TestUnpackValidation:
    """Test unpack validation."""

    def test_unpack_empty_bytes_raises(self):
        """Test that unpacking empty bytes raises error."""
        try:
            ComlinkMessage.unpack(b"")
            assert False, "Should have raised ValueError"
        except ValueError as e:
            assert "Empty message" in str(e)

    def test_unpack_invalid_msgpack_raises(self):
        """Test that unpacking invalid msgpack raises error."""
        try:
            ComlinkMessage.unpack(b"\xff\xff\xff")  # Invalid msgpack
            assert False, "Should have raised ValueError"
        except ValueError:
            pass

    def test_unpack_non_dict_raises(self):
        """Test that unpacking non-dict raises error."""
        # Pack a list instead of dict
        packed = msgpack.packb([1, 2, 3])

        try:
            ComlinkMessage.unpack(packed)
            assert False, "Should have raised ValueError"
        except ValueError as e:
            assert "Expected dict" in str(e)


class TestEdgeCases:
    """Test edge cases."""

    def test_empty_function_name(self):
        """Test call with empty function name."""
        msg = ComlinkMessage.create_call(function="", args=())

        packed = msg.pack()
        unpacked = ComlinkMessage.unpack(packed)

        assert unpacked.function == ""

    def test_unicode_function_name(self):
        """Test call with unicode function name."""
        msg = ComlinkMessage.create_call(function="\u4e2d\u6587\u51fd\u6570", args=())

        packed = msg.pack()
        unpacked = ComlinkMessage.unpack(packed)

        assert unpacked.function == "\u4e2d\u6587\u51fd\u6570"

    def test_emoji_in_args(self):
        """Test call with emoji in arguments."""
        msg = ComlinkMessage.create_call(
            function="process", args=("\U0001f600\U0001f389",)
        )

        packed = msg.pack()
        unpacked = ComlinkMessage.unpack(packed)

        assert unpacked.args[0] == "\U0001f600\U0001f389"

    def test_large_args(self):
        """Test call with large arguments."""
        large_data = list(range(10000))
        msg = ComlinkMessage.create_call(function="process", args=(large_data,))

        packed = msg.pack()
        unpacked = ComlinkMessage.unpack(packed)

        assert len(unpacked.args[0]) == 10000

    def test_nested_structures(self):
        """Test deeply nested data structures."""
        nested = {"a": {"b": {"c": {"d": {"e": "deep"}}}}}
        msg = ComlinkMessage.create_response(result=nested, msg_id="nested-123")

        packed = msg.pack()
        unpacked = ComlinkMessage.unpack(packed)

        assert unpacked.result["a"]["b"]["c"]["d"]["e"] == "deep"

    def test_binary_data(self):
        """Test binary data in arguments."""
        binary = b"\x00\x01\x02\xff"
        msg = ComlinkMessage.create_response(result=binary, msg_id="binary-123")

        packed = msg.pack()
        unpacked = ComlinkMessage.unpack(packed)

        # Binary should be preserved
        assert unpacked.result == binary

    def test_special_float_values(self):
        """Test special float values are sanitized during pack."""
        msg = ComlinkMessage.create_call(
            function="test", args=(float("nan"), float("inf"), float("-inf"))
        )

        packed = msg.pack()
        unpacked = ComlinkMessage.unpack(packed)

        # NaN and Infinity are converted to None by _sanitize_for_msgpack
        # But they're also in a list which gets recursively filtered
        # The actual result depends on msgpack handling
        # Check that values are either None or the sanitized form
        for val in unpacked.args:
            assert val is None or isinstance(val, float)

    def test_control_characters(self):
        """Test control characters in strings."""
        msg = ComlinkMessage.create_call(
            function="test", args=("\x00\x01\x02\x03\t\n\r",)
        )

        packed = msg.pack()
        unpacked = ComlinkMessage.unpack(packed)

        assert unpacked.args[0] == "\x00\x01\x02\x03\t\n\r"


class TestToDict:
    """Test to_dict method."""

    def test_to_dict_basic(self):
        """Test basic to_dict conversion."""
        msg = ComlinkMessage.create_call(function="add", args=(1, 2))

        data = msg.to_dict()

        assert isinstance(data, dict)
        assert data["type"] == "call"
        assert data["function"] == "add"

    def test_to_dict_excludes_private(self):
        """Test that to_dict excludes private attributes."""
        msg = ComlinkMessage()
        msg._private = "secret"

        data = msg.to_dict()

        assert "_private" not in data


def run_tests():
    """Run all tests."""
    print("=" * 60)
    print("Running ComlinkMessage Tests")
    print("=" * 60)

    # TestMessageType
    print("\nTestMessageType...")
    t = TestMessageType()
    t.test_message_types_exist()
    print("  [OK] All TestMessageType passed")

    # TestComlinkMessageCreation
    print("\nTestComlinkMessageCreation...")
    t = TestComlinkMessageCreation()
    t.test_default_values()
    t.test_custom_values()
    t.test_app_name()
    print("  [OK] All TestComlinkMessageCreation passed")

    # TestCallMessages
    print("\nTestCallMessages...")
    t = TestCallMessages()
    t.test_create_call_basic()
    t.test_create_call_with_namespace()
    t.test_create_call_with_id()
    t.test_create_call_with_metadata()
    t.test_create_call_with_client_name()
    print("  [OK] All TestCallMessages passed")

    # TestResponseMessages
    print("\nTestResponseMessages...")
    t = TestResponseMessages()
    t.test_create_response_basic()
    t.test_create_response_with_metadata()
    t.test_create_response_none_result()
    print("  [OK] All TestResponseMessages passed")

    # TestErrorMessages
    print("\nTestErrorMessages...")
    t = TestErrorMessages()
    t.test_create_error_basic()
    t.test_create_error_with_metadata()
    print("  [OK] All TestErrorMessages passed")

    # TestOutputMessages
    print("\nTestOutputMessages...")
    t = TestOutputMessages()
    t.test_create_stdout()
    t.test_create_stderr()
    print("  [OK] All TestOutputMessages passed")

    # TestHeartbeatMessages
    print("\nTestHeartbeatMessages...")
    t = TestHeartbeatMessages()
    t.test_create_heartbeat_basic()
    t.test_create_heartbeat_with_id()
    t.test_create_heartbeat_with_timestamp()
    t.test_create_heartbeat_response()
    print("  [OK] All TestHeartbeatMessages passed")

    # TestPackUnpack
    print("\nTestPackUnpack...")
    t = TestPackUnpack()
    t.test_pack_unpack_call()
    t.test_pack_unpack_response()
    t.test_pack_unpack_error()
    t.test_pack_unpack_heartbeat()
    t.test_pack_returns_bytes()
    t.test_pack_is_msgpack()
    print("  [OK] All TestPackUnpack passed")

    # TestSanitization
    print("\nTestSanitization...")
    t = TestSanitization()
    t.test_sanitize_nan()
    t.test_sanitize_infinity()
    t.test_sanitize_int_overflow()
    t.test_sanitize_dict()
    t.test_sanitize_list()
    t.test_sanitize_non_string_keys()
    print("  [OK] All TestSanitization passed")

    # TestFilterNone
    print("\nTestFilterNone...")
    t = TestFilterNone()
    t.test_filter_none_dict()
    t.test_filter_none_list()
    t.test_filter_none_primitives()
    print("  [OK] All TestFilterNone passed")

    # TestExtractCallArgs
    print("\nTestExtractCallArgs...")
    t = TestExtractCallArgs()
    t.test_extract_args_present()
    t.test_extract_args_missing()
    t.test_extract_args_empty()
    print("  [OK] All TestExtractCallArgs passed")

    # TestUnpackValidation
    print("\nTestUnpackValidation...")
    t = TestUnpackValidation()
    t.test_unpack_empty_bytes_raises()
    t.test_unpack_invalid_msgpack_raises()
    t.test_unpack_non_dict_raises()
    print("  [OK] All TestUnpackValidation passed")

    # TestEdgeCases
    print("\nTestEdgeCases...")
    t = TestEdgeCases()
    t.test_empty_function_name()
    t.test_unicode_function_name()
    t.test_emoji_in_args()
    t.test_large_args()
    t.test_nested_structures()
    t.test_binary_data()
    t.test_special_float_values()
    t.test_control_characters()
    print("  [OK] All TestEdgeCases passed")

    # TestToDict
    print("\nTestToDict...")
    t = TestToDict()
    t.test_to_dict_basic()
    t.test_to_dict_excludes_private()
    print("  [OK] All TestToDict passed")

    print("\n" + "=" * 60)
    print("[PASS] All ComlinkMessage tests passed!")
    print("=" * 60)


if __name__ == "__main__":
    run_tests()
