"""§2.2 pairing codes and session tokens, with an injected clock."""

from __future__ import annotations

import base64
import re

from support import FakeClock

from waffle_mcp_relay.pairing import CODE_TTL_S, SESSION_RESUME_S, Pairing, new_token


def test_token_is_32_random_bytes_base64url() -> None:
    token = new_token()
    assert re.fullmatch(r"[A-Za-z0-9_-]{43}", token)
    assert len(base64.urlsafe_b64decode(token + "=")) == 32
    assert new_token() != token


def test_code_is_single_use() -> None:
    pairing = Pairing(FakeClock())
    code, _ = pairing.issue_code()
    first = pairing.admit_code(code)
    assert first.ok and first.session
    pairing.page_disconnected(first.session, revoke=False)
    assert pairing.admit_code(code).reason == "invalid_code"


def test_code_expires_after_300_s() -> None:
    clock = FakeClock()
    pairing = Pairing(clock)
    code, expires_at = pairing.issue_code()
    assert expires_at == clock.now + CODE_TTL_S
    clock.advance(CODE_TTL_S - 0.001)
    assert pairing.state() == "awaiting_consent"
    clock.advance(0.001)
    assert pairing.state() == "unpaired"
    assert pairing.admit_code(code).reason == "invalid_code"


def test_wrong_code_does_not_spend_the_real_one() -> None:
    pairing = Pairing(FakeClock())
    code, _ = pairing.issue_code()
    assert pairing.admit_code("x" * 43).reason == "invalid_code"
    assert pairing.admit_code(None).reason == "invalid_code"
    assert pairing.admit_code(code).ok


def test_second_page_refused_while_paired() -> None:
    pairing = Pairing(FakeClock())
    code, _ = pairing.issue_code()
    session = pairing.admit_code(code).session
    assert pairing.admit_code(code).reason == "already_paired"
    assert pairing.admit_session(session).reason == "already_paired"


def test_session_resume_window() -> None:
    clock = FakeClock()
    pairing = Pairing(clock)
    code, _ = pairing.issue_code()
    session = pairing.admit_code(code).session
    assert session is not None
    pairing.page_disconnected(session, revoke=False)
    clock.advance(SESSION_RESUME_S)
    resumed = pairing.admit_session(session)
    assert resumed.ok and resumed.resumed and resumed.session == session

    pairing.page_disconnected(session, revoke=False)
    clock.advance(SESSION_RESUME_S + 0.001)
    assert pairing.admit_session(session).reason == "session_expired"


def test_user_disconnect_revokes_session() -> None:
    pairing = Pairing(FakeClock())
    code, _ = pairing.issue_code()
    session = pairing.admit_code(code).session
    assert session is not None
    pairing.page_disconnected(session, revoke=True)
    assert pairing.admit_session(session).reason == "session_expired"
    assert pairing.state() == "unpaired"


def test_new_code_revokes_live_session_and_stale_disconnect_is_ignored() -> None:
    pairing = Pairing(FakeClock())
    code, _ = pairing.issue_code()
    old = pairing.admit_code(code).session
    assert old is not None
    new_code, _ = pairing.issue_code()
    assert pairing.state() == "awaiting_consent"
    new_session = pairing.admit_code(new_code).session
    pairing.page_disconnected(old, revoke=False)  # the revoked page's socket closing late
    assert pairing.page_connected
    assert pairing.admit_session(old).reason == "already_paired"
    assert new_session != old
