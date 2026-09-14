"""Pairing codes and the session token (spec §2.2, §3.1 P2-P8, P11-P12).

Pure state, no I/O. Time comes from an injected clock (seconds, epoch-based
so `expires_at` can be reported as wall time), which is what lets the tests
expire a code without sleeping.
"""

from __future__ import annotations

import base64
import hmac
import secrets
import time
from collections.abc import Callable
from dataclasses import dataclass

CODE_TTL_S = 300.0
SESSION_RESUME_S = 120.0


def new_token() -> str:
    """32 random bytes, base64url without padding."""
    return base64.urlsafe_b64encode(secrets.token_bytes(32)).rstrip(b"=").decode("ascii")


def _same(a: str | None, b: object) -> bool:
    if a is None or not isinstance(b, str):
        return False
    return hmac.compare_digest(a.encode("utf-8"), b.encode("utf-8"))


@dataclass(frozen=True)
class Admission:
    ok: bool
    session: str | None = None
    reason: str | None = None
    resumed: bool = False


class Pairing:
    def __init__(self, clock: Callable[[], float] = time.time) -> None:
        self._clock = clock
        self._code: str | None = None
        self._code_expires_at = 0.0
        self._session: str | None = None
        self._page_connected = False
        self._disconnected_at: float | None = None

    @property
    def page_connected(self) -> bool:
        return self._page_connected

    @property
    def code_expires_at(self) -> float | None:
        return self._code_expires_at if self._code is not None else None

    def state(self) -> str:
        """`paired`, `awaiting_consent` or `unpaired`."""
        if self._page_connected:
            return "paired"
        if self._code is not None and self._clock() < self._code_expires_at:
            return "awaiting_consent"
        return "unpaired"

    def issue_code(self) -> tuple[str, float]:
        """New single-use code. Revokes the live session and any earlier unused code (P11)."""
        self._code = new_token()
        self._code_expires_at = self._clock() + CODE_TTL_S
        self._session = None
        self._page_connected = False
        self._disconnected_at = None
        return self._code, self._code_expires_at

    def admit_code(self, code: object) -> Admission:
        valid = _same(self._code, code) and self._clock() < self._code_expires_at
        if valid:
            # Single use: a valid code is spent the moment it is presented.
            self._code = None
        if self._page_connected:
            return Admission(False, reason="already_paired")
        if not valid:
            return Admission(False, reason="invalid_code")
        self._session = new_token()
        self._page_connected = True
        self._disconnected_at = None
        return Admission(True, session=self._session)

    def admit_session(self, token: object) -> Admission:
        if self._page_connected:
            return Admission(False, reason="already_paired")
        within_window = (
            self._disconnected_at is not None
            and self._clock() - self._disconnected_at <= SESSION_RESUME_S
        )
        if not (_same(self._session, token) and within_window):
            return Admission(False, reason="session_expired")
        self._page_connected = True
        self._disconnected_at = None
        return Admission(True, session=self._session, resumed=True)

    def page_disconnected(self, session: str, *, revoke: bool) -> None:
        """The page holding `session` went away. No-op if that session is no longer current."""
        if not _same(self._session, session):
            return
        self._page_connected = False
        if revoke:
            self._session = None
            self._disconnected_at = None
        else:
            self._disconnected_at = self._clock()
