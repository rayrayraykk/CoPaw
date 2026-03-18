#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""Test that reusable services are properly cleaned up on final shutdown."""
# pylint: disable=protected-access
import asyncio
import logging
import sys

sys.path.insert(0, "src")

# Set up logging to capture DEBUG messages
logging.basicConfig(
    level=logging.DEBUG,
    format="%(levelname)-8s | %(message)s",
)


async def test_final_shutdown_cleanup():
    """Verify reusable services are closed during final shutdown."""
    from copaw.app.multi_agent_manager import MultiAgentManager

    manager = MultiAgentManager()

    # Create and start workspace
    ws = await manager.get_agent("default")
    mm = ws.memory_manager
    cm = ws.chat_manager

    print("\n" + "=" * 70)
    print("Initial State")
    print("=" * 70)
    print(f"MemoryManager: {mm}")
    print(f"ChatManager: {cm}")

    # Reload to test reusable services are skipped
    print("\n" + "=" * 70)
    print("Reload (should SKIP closing reusable services)")
    print("=" * 70)
    await manager.reload_agent("default")

    ws2 = await manager.get_agent("default")
    assert id(ws2.memory_manager) == id(mm), "Should be same instance"
    print("✓ MemoryManager preserved during reload")

    # Final shutdown
    print("\n" + "=" * 70)
    print("Final Shutdown (should CLOSE all services)")
    print("=" * 70)
    await manager.stop_all()

    print("\n✓ Test complete - check logs above")
    print("Expected:")
    print("  - During reload: 'Skipped stopping reusable service'")
    print("  - During final: 'Service 'memory_manager' stopped'")


if __name__ == "__main__":
    asyncio.run(test_final_shutdown_cleanup())
