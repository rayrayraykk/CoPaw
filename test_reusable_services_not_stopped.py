#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""Test that reusable services are not stopped during reload."""
# pylint: disable=protected-access
import asyncio
import logging
import sys

sys.path.insert(0, "src")

# Set up logging to capture DEBUG messages
logging.basicConfig(
    level=logging.DEBUG,
    format="%(levelname)-8s | %(name)-40s | %(message)s",
)


async def test_reusable_services_not_stopped():
    """Verify that reusable services skip stop during old instance cleanup."""
    from copaw.app.multi_agent_manager import MultiAgentManager

    manager = MultiAgentManager()

    # Create initial workspace
    ws1 = await manager.get_agent("default")
    mm1_id = id(ws1.memory_manager)
    cm1_id = id(ws1.chat_manager)

    print("\n" + "=" * 70)
    print("Initial Workspace Created")
    print("=" * 70)
    print(f"MemoryManager ID: {mm1_id}")
    print(f"ChatManager ID: {cm1_id}")

    # Reload agent
    print("\n" + "=" * 70)
    print("Reloading Agent (watch for 'Skipped stopping' messages)...")
    print("=" * 70)

    await manager.reload_agent("default")

    ws2 = await manager.get_agent("default")
    mm2_id = id(ws2.memory_manager)
    cm2_id = id(ws2.chat_manager)

    print("\n" + "=" * 70)
    print("Reload Complete - Verification")
    print("=" * 70)
    print(f"MemoryManager reused: {mm1_id == mm2_id}")
    print(f"ChatManager reused: {cm1_id == cm2_id}")

    # Check if services are still functioning
    assert mm1_id == mm2_id, "MemoryManager should be reused"
    assert cm1_id == cm2_id, "ChatManager should be reused"

    print("\n✓ All reusable services were preserved")

    await manager.stop_all()


if __name__ == "__main__":
    asyncio.run(test_reusable_services_not_stopped())
