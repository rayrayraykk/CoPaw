# -*- coding: utf-8 -*-
from mcp.server.fastmcp import FastMCP

# 创建一个 MCP 服务
mcp = FastMCP("test-server")


# 一个最简单的工具：加法
@mcp.tool()
def add(a: int, b: int) -> int:
    """返回两个整数的和"""
    return a + b


# 再加一个文本回显工具，方便测试
@mcp.tool()
def echo(text: str) -> str:
    """原样返回输入文本"""
    return f"echo: {text}"


if __name__ == "__main__":
    mcp.run()
