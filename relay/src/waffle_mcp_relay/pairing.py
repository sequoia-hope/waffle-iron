"""Pairing codes and the session token (spec §2.2, §3.1 P2-P8, P11-P12, P16-P18).

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
SESSION_RESUME_S = 1800.0


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
    replaced: bool = False  # a persistent code took over from a live page (P18)


class Pairing:
    def __init__(
        self,
        clock: Callable[[], float] = time.time,
        *,
        resume_s: float = SESSION_RESUME_S,
        persistent_code: str | None = None,
    ) -> None:
        self._clock = clock
        self._resume_s = resume_s
        self._persistent_code = persistent_code
        self._code: str | None = None
        self._code_expires_at = 0.0
        self._session: str | None = None
        self._page_connected = False
        self._disconnected_at: float | None = None

    @property
    def page_connected(self) -> bool:
        return self._page_connected

    @property
    def persistent(self) -> bool:
        return self._persistent_code is not None

    @property
    def code_expires_at(self) -> float | None:
        return self._code_expires_at if self._code is not None else None

    def state(self) -> str:
        """`paired`, `page_away`, `awaiting_consent` or `unpaired`."""
        if self._page_connected:
            return "paired"
        if self._resumable():
            return "page_away"
        if self._code is not None and self._clock() < self._code_expires_at:
            return "awaiting_consent"
        return "unpaired"

    def _resumable(self) -> bool:
        return (
            self._session is not None
            and self._disconnected_at is not None
            and self._clock() - self._disconnected_at <= self._resume_s
        )

    def issue_code(self) -> tuple[str, float | None]:
        """A pairing code; revokes the live session and any earlier unused code (P11).

        With a persistent code (P17) that code is returned every time and has
        no expiry (`None`).
        """
        self._session = None
        self._page_connected = False
        self._disconnected_at = None
        if self._persistent_code is not None:
            self._code = None
            return self._persistent_code, None
        self._code = new_token()
        self._code_expires_at = self._clock() + CODE_TTL_S
        return self._code, self._code_expires_at

    def admit_code(self, code: object) -> Admission:
        if _same(self._persistent_code, code):
            # Reusable, never expires; a consented click may replace a live page.
            replaced = self._page_connected
            self._code = None
            return self._start_session(replaced=replaced)
        valid = _same(self._code, code) and self._clock() < self._code_expires_at
        if valid:
            # Single use: a valid code is spent the moment it is presented.
            self._code = None
        if self._page_connected:
            return Admission(False, reason="already_paired")
        if not valid:
            return Admission(False, reason="invalid_code")
        return self._start_session(replaced=False)

    def _start_session(self, *, replaced: bool) -> Admission:
        self._session = new_token()
        self._page_connected = True
        self._disconnected_at = None
        return Admission(True, session=self._session, replaced=replaced)

    def admit_session(self, token: object) -> Admission:
        if self._page_connected:
            return Admission(False, reason="already_paired")
        if not (_same(self._session, token) and self._resumable()):
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
