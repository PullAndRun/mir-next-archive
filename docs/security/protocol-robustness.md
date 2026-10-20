# 协议健壮性与限流（第 20 轮）

## 帧语义（`mir-protocol/src/codec.rs`）

| 输入 | 期望 | 测试 |
|---|---|---|
| 头不足 4 字节 | `Ok(None)` | `frame_length_classes_match_crystal_semantics` |
| 头完整但 body 不足声明长度 | `Ok(None)` | 同上 |
| 声明长度 0..3 | `Err(InvalidFrameLength)` | 同上 |
| 声明长度 `MAX_PACKET_LEN + 1` | 长度校验函数拒绝（u16 帧头无法表达超限） | 同上 |
| 完整帧 + 随机 body | `Err`/`Ok` 均可，**panic == 0** | `fuzz_random_bodies_never_panic` |

## `ALL_*` 真实入口

`tools/protocol-gen/generate.py` 生成 `ALL_CLIENT_INDICES` / `ALL_SERVER_INDICES`，
并复用同一 dispatch 导出 `decode_client_packet` / `decode_server_packet`：

- `crates/mir-protocol/src/packets/client.rs`：`ALL_CLIENT_INDICES` 实测 **153**（149 个生成包 + 4 个 manual 扩展包）
- `crates/mir-protocol/src/packets/server.rs`：`ALL_SERVER_INDICES` 实测 **283**（259 个生成包 + 24 个 manual 扩展包）
- 断言口径：`ALL_* 长度 == 生成文件 pub const INDEX 计数 + manual 扩展包数`；本仓库当前为 153/283，而不是仅生成包的 149/259。
- 遍历所有 ID：不得 `UnknownPacket`；ID `32000` 必须 `UnknownPacket`；
- 生成器 `--check` 与提交文件同步。

## 资源上限

| 上限 | 值 | 代码位置 | 测试 |
|---|---:|---|---|
| 单帧长度 | 65535（u16） | `mir-protocol/src/codec.rs` `validate_frame_length` | `frame_length_classes_match_crystal_semantics` |
| 集合长度 | `1 << 20` | `mir-protocol/src/lib.rs` `MAX_COLLECTION_LEN` | `PacketReader::read_count` 边界（协议现有测试） |
| 单连接包速率 | `max_packets_per_second`（默认 200） | `mir-server/src/session.rs` `rate_allows_packet` | `packet_rate_limit_at_200_per_second_drops_excess_and_keepalive_responds` |
| 连接并发/秒 | `max_connections_per_ip` / `max_connections_per_second` | `mir-server/src/server.rs` `ConnectionLimiter` | `anti_flood` 既有用例 |
| TCP 垃圾/超大帧 | 不 panic、连接不踢、新连接可握 | `FrameBuffer::next_frame` | `garbage_and_oversized_frames_keep_server_alive` |

## 本轮发现与修复

1. `codec.rs` 新增公开 `validate_frame_length`，`next_frame` 复用；超限长度可被单元测试直接喂入（u16 帧头不可表达）。
2. 生成器新增 `ALL_*` 与 `decode_*_packet`，避免 fuzz 自己维护一份“假入口”。
3. **负向自测（实测）**：
   - 注释 `validate_frame_length(length)?` → `frame_length_classes_match_crystal_semantics` FAILED；恢复后 1 passed。
   - 将 `ClientVersion::INDEX` dispatch 改为 `9_000` → `all_generated_ids_hit_real_dispatch_and_unknown_id_is_rejected` FAILED（`client 0 ... UnknownPacket`）；恢复后 3 passed。
4. **字段级边界（生成器覆盖）**：`BoundarySample` 现覆盖 **170** 条：client `StringTooLong=32 / CollectionCountHuge=2 / EnumUnknown=40 / FloatNotFinite=0`，server `52 / 0 / 40 / 4`；string/count 断言语义，enum/float 断言当前解码行为不 panic。`packets_without_boundary_fields()` 为纯定宽整数包（client 99 / server 207），`packets_with_uncovered_boundary_fields()` 为前置字段无法默认编码的包（client 0 / server 1 `ChangeQuest`）。
5. **ChangeQuest 白名单豁免（真实缺口）**：`boundary_exempt_packets()` 登记 `ChangeQuest(index 201)`，原因“nested ClientQuestProgress cannot be Default-encoded; hand-written constructor needed; its task_list string/count boundaries remain uncovered”。其 `ClientQuestProgress` 确实含 `task_list: Vec<String>`（字符串/集合字段），目前该包的 string/count 边界**未被覆盖**，列入下轮小项；未知 `QuestState` 走 `from_value(...).unwrap_or_default()`，即**回落 default**，不是 Err。
6. **负向自测**：把 string 样本从 `max+1` 改为 `max` 后 `generated_string_boundary_samples_must_error` FAILED（`CollectionTooLong` 变 `UnexpectedEof`），恢复后绿。
