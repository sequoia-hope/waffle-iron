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


def test_default_resume_window_outlasts_a_backgrounded_tab() -> None:
    assert SESSION_RESUME_S >= 30 * 60


def test_resume_window_is_configurable_and_reports_page_away() -> None:
    clock = FakeClock()
    pairing = Pairing(clock, resume_s=60.0)
    code, _ = pairing.issue_code()
    session = pairing.admit_code(code).session
    assert session is not None
    pairing.page_disconnected(session, revoke=False)
    assert pairing.state() == "page_away"
    clock.advance(60.0)
    assert pairing.state() == "page_away"
    clock.advance(0.001)
    assert pairing.state() == "unpaired"
    assert pairing.admit_session(session).reason == "session_expired"


def test_revoked_session_is_not_page_away() -> None:
    pairing = Pairing(FakeClock())
    code, _ = pairing.issue_code()
    session = pairing.admit_code(code).session
    assert session is not None
    pairing.page_disconnected(session, revoke=True)
    assert pairing.state() == "unpaired"


PERSISTENT = "p" * 43


def test_persistent_code_is_reusable_and_never_expires() -> None:
    clock = FakeClock()
    pairing = Pairing(clock, persistent_code=PERSISTENT)
    code, expires_at = pairing.issue_code()
    assert code == PERSISTENT and expires_at is None
    first = pairing.admit_code(code)
    assert first.ok and first.session and not first.replaced
    pairing.page_disconnected(first.session, revoke=True)
    clock.advance(365 * 24 * 3600)
    again = pairing.admit_code(PERSISTENT)
    assert again.ok and again.session != first.session
    assert pairing.issue_code() == (PERSISTENT, None)


def test_persistent_code_replaces_a_live_page_and_ends_its_session() -> None:
    pairing = Pairing(FakeClock(), persistent_code=PERSISTENT)
    first = pairing.admit_code(PERSISTENT)
    second = pairing.admit_code(PERSISTENT)
    assert second.ok and second.replaced
    assert first.session is not None and second.session != first.session
    pairing.page_disconnected(first.session, revoke=True)  # the replaced socket closing late
    assert pairing.page_connected
    pairing.page_disconnected(second.session or "", revoke=False)
    assert pairing.admit_session(first.session).reason == "session_expired"


def test_a_random_code_is_still_refused_with_a_persistent_link() -> None:
    pairing = Pairing(FakeClock(), persistent_code=PERSISTENT)
    assert pairing.admit_code("x" * 43).reason == "invalid_code"
