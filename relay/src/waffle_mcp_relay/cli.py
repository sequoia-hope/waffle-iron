"""`waffle-mcp-relay` entry point.

stdout carries JSON-RPC only (I13): configuration errors and logs go to stderr.
"""

from __future__ import annotations

import asyncio
import logging
import os
import sys
from collections.abc import Sequence

from waffle_mcp_relay.config import ConfigError, build_config


def main(argv: Sequence[str] | None = None) -> int:
    logging.basicConfig(
        stream=sys.stderr, level=logging.INFO, format="waffle-mcp-relay: %(message)s"
    )
    try:
        config = build_config(argv, os.environ)
    except ConfigError as err:
        print(str(err), file=sys.stderr)
        return 2

    from waffle_mcp_relay.server import run_relay

    try:
        asyncio.run(run_relay(config))
    except OSError as err:
        print(f"cannot listen on {config.listen_address}: {err}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        return 130
    return 0
