# -*- coding: utf-8 -*-
"""CLI entry for Ralph Loop — ``qwenpaw ralph``."""
from __future__ import annotations

import click

from .http import client, print_json, resolve_base_url


@click.group("ralph")
def ralph_group():
    """Ralph Loop — iterative long-task execution."""


@ralph_group.command("start")
@click.argument("task", nargs=-1, required=True)
@click.option("--agent", default="default", help="Agent ID to use.")
@click.option(
    "--verify",
    default="",
    help="Verification command (e.g. pytest).",
)
@click.option(
    "--max-iterations",
    default=20,
    type=int,
    help="Max iterations.",
)
@click.option("--base-url", default=None, help="Server base URL.")
@click.pass_context
def ralph_start(ctx, task, agent, verify, max_iterations, base_url):
    """Start a Ralph loop with a task description.

    Example: qwenpaw ralph start "Add authentication to the API"
    """
    task_text = " ".join(task)
    parts = [f"/ralph {task_text}"]
    if verify:
        parts.append(f"--verify {verify}")
    if max_iterations != 20:
        parts.append(f"--max-iterations {max_iterations}")
    query = " ".join(parts)

    url = resolve_base_url(ctx, base_url)
    payload = {
        "text": query,
        "session_id": f"ralph:{agent}",
        "user_id": "cli",
        "agent_id": agent,
    }
    resp = client(url).post("/api/chat/send", json=payload)
    print_json(resp)


@ralph_group.command("status")
@click.option("--agent", default="default", help="Agent ID.")
@click.option("--base-url", default=None, help="Server base URL.")
@click.pass_context
def ralph_status(ctx, agent, base_url):
    """Show current Ralph loop status."""
    url = resolve_base_url(ctx, base_url)
    payload = {
        "text": "/ralph status",
        "session_id": f"ralph:{agent}",
        "user_id": "cli",
        "agent_id": agent,
    }
    resp = client(url).post("/api/chat/send", json=payload)
    print_json(resp)


@ralph_group.command("list")
@click.option("--agent", default="default", help="Agent ID.")
@click.option("--base-url", default=None, help="Server base URL.")
@click.pass_context
def ralph_list(ctx, agent, base_url):
    """List all Ralph loops."""
    url = resolve_base_url(ctx, base_url)
    payload = {
        "text": "/ralph list",
        "session_id": f"ralph:{agent}",
        "user_id": "cli",
        "agent_id": agent,
    }
    resp = client(url).post("/api/chat/send", json=payload)
    print_json(resp)
