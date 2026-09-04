class QwenPawError(Exception):
    """Base SDK error."""


class TransportClosedError(QwenPawError):
    """Raised when the App Server transport closes."""


class RequestTimeoutError(QwenPawError):
    """Raised when an App Protocol request times out."""


class ProtocolVersionError(QwenPawError):
    """Raised when Core and SDK protocol versions do not match."""


class RpcRequestError(QwenPawError):
    """An error returned by an App Protocol request."""

    def __init__(self, code: int, message: str) -> None:
        self.code = code
        self.rpc_message = message
        super().__init__(f"QwenPaw Core error {code}: {message}")
