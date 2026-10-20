#!/usr/bin/env python3
"""测试用 TCP 代理：127.0.0.1:PORT -> PG_PROXY_TARGET_HOST:PORT。

PG_PROXY_CONTROL 文件内容为 `pause` 时新连接立即关闭（模拟网络不可达）；
内容为 `resume`（或文件不存在）时正常转发。仅用于第 22 轮 T2，不碰共享 PG。
"""
from __future__ import annotations

import asyncio
import os
from pathlib import Path

LISTEN_PORT = int(os.environ.get("PG_PROXY_PORT", "15432"))
TARGET_HOST = os.environ["PG_PROXY_TARGET_HOST"]
TARGET_PORT = int(os.environ["PG_PROXY_TARGET_PORT"])
CONTROL = Path(os.environ.get("PG_PROXY_CONTROL", "target/pg_proxy.control"))


def paused() -> bool:
    try:
        return CONTROL.read_text(encoding="utf-8").strip() == "pause"
    except OSError:
        return False


async def pipe(reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
    try:
        while not paused():
            data = await reader.read(65536)
            if not data:
                break
            writer.write(data)
            await writer.drain()
    except (ConnectionError, OSError):
        pass
    finally:
        writer.close()


async def handle(client_reader: asyncio.StreamReader, client_writer: asyncio.StreamWriter) -> None:
    if paused():
        client_writer.close()
        return
    try:
        remote_reader, remote_writer = await asyncio.open_connection(TARGET_HOST, TARGET_PORT)
    except OSError:
        client_writer.close()
        return
    await asyncio.gather(
        pipe(client_reader, remote_writer),
        pipe(remote_reader, client_writer),
    )
    client_writer.close()
    remote_writer.close()


async def main() -> None:
    server = await asyncio.start_server(handle, "127.0.0.1", LISTEN_PORT)
    print(f"pg_proxy listening on 127.0.0.1:{LISTEN_PORT}", flush=True)
    async with server:
        await server.serve_forever()


if __name__ == "__main__":
    asyncio.run(main())
