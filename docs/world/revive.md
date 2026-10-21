# 死亡与复活（第 26 轮 DR-1）

## 正常路径（服务端）
| 步骤 | 实现 | Crystal 依据 |
|---|---|---|
| 客户端死亡动画结束 | 客户端弹窗；玩家点「是」发 `C.TownRevive`（空包） | `Client/MirScenes/GameScene.cs:1269-1281` |
| 服务端分派 | `ClientPacket::TownRevive` → `Session::on_town_revive` | `Server/MirNetwork/MirConnection.cs:1643-1648` |
| 回城/出生点 | `World::town_revive_player(player_id, start_map, start_point)`：`place_player` 同步 AOI → `revive_player` | `PlayerObject.TownRevive`，`PlayerObject.cs:1392-1455` |
| 满血满蓝 | `set_player_hp(stats.hp)` / `set_player_mp(stats.mp)` | `PlayerObject.cs:1414-1415` |
| 本人包 | `S.ObjectRevived`（effect=true）+ `S.Revived` | `HumanObject.Revive`，`HumanObject.cs:1690-1721`；`PlayerObject.cs:1448-1449` |
| 旁观者 | `S.ObjectRemove` + `S.ObjectRevived` + 外观包 | `HumanObject.cs:1697-1721` |

## 兜底
- 正常路径**没有 10s 自动复活**；`PLAYER_REVIVE_DELAY_MS` 已改为 **60_000ms** 安全兜底；
- 到点触发：`WARN` 日志 + `mir2_revive_fallback_total`，再 `revive_player`；
- 12s 内不得有 `S.Revived`；测试：`world_lifecycle::fallback_revives_only_after_60s_and_counts_metric`。

## 兼容/已知差异
- `S.MapChanged` 未在 TownRevive 时额外下发：世界层只有地图文件/坐标，客户端现有 `S.ObjectRevived` 足以恢复动作；地图标题/灯光等元数据在进图时已下发。
- 旧 5 个用例已改为主动复活入口（见回执），不删除测试。

## 旧用例改造（5 个）
| 用例 | 改法 | 结果 |
|---|---|---|
| world_lifecycle::player_dies_then_revives_in_place_and_can_walk_again | 删除 10s 自动复活断言；改为 12s 仍 dead、无 S.Revived；再 `town_revive_player` 到 (30,30)，断言 HP/MP 满、可走；observer 仍收 ObjectRevived | passed 14/14 |
| pk::revive_keeps_pk_points | 死亡后显式 `revive_player`，不再等 10s；PK 值仍 150 | passed 27/27 |
| gm::heal_command_restores_hp_and_revives | 不改入口（`@heal` 本来调用 revive_player），验证仍过 | passed 18/18 |
| gm::revive_command_revives_dead_player | 不改入口（`@revive` 显式复活），验证仍过 | passed 18/18 |
| use_items::dead_player_cannot_use_a_potion_but_a_resurrection_scroll_revives | 不改（显式 `revive_time=i64::MAX`，复活卷轴路径），验证仍过 | passed 13/13 |

新增：`world_lifecycle::fallback_revives_only_after_60s_and_counts_metric`（59.999s dead、60s 复活、指标递增）。


## 验证状态（2026-10-21 12:xx）
- 已编译：`cargo test -p mir-server` 全绿（含 world_lifecycle 30、pk 27、gm 18、use_items 13、login_flow 7）。
- world 层已验证：12s 内不发 `S.Revived`、`town_revive_player` 到回城点、HP/MP 满、旁观者收 `ObjectRevived`、60s 兜底的 WARN/指标。
- **未完成**：真实 TCP 的死亡 fixture。GM `@kill` 在 login_flow 测试里未产生 `Death`（后续包只有欢迎/集合包），为避免留红已移除该临时用例；`C.TownRevive` 分派目前只有 world 层覆盖。下一轮用 `serve_forever_with_world` + 公开 `player_mut` 注入死亡后补。
