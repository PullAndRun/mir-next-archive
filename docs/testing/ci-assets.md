# CI 素材依赖审计（没有 `resource/` 也要全绿）

无素材的干净环境（原远端 CI）只有源码，**没有 `resource/`**：地图（`resource/map/*.map`）、
图库（`*.wzl`）、真实数据库（`resource/community/<repo>/Server.MirDB`）都不存在。
本页记录「哪些测试会碰真实素材、它们缺素材时怎么办」，以及怎么在本地复现 CI。

## 结论（2026-10-20 审计）

**整套 `cargo test --workspace` 在没有任何 `resource/` 的目录里运行：85 个测试二进制、全绿。**
（方式见文末「复现 CI」，脚本 `tools/dev/test-no-assets.ps1`。）

涉及真实素材的用例共 30 条，处理方式只有两种：

| 文件 | 用例 | 碰到什么 | 缺素材时的行为 |
|---|---|---|---|
| `tests/maps.rs` | `real_database_has_movement_points_that_resolve_to_maps` | `GameData::discover` + `resource/map` | **skip**（`eprintln!` + `return`） |
| `tests/maps.rs` | `real_map_files_load_and_change_map_moves_between_them` | 同上 | skip |
| `tests/maps.rs` | `bichon_has_a_walkable_movement_point_near_the_plaza` | 同上 | skip |
| `tests/maps.rs` | `player_can_walk_from_the_plaza_into_a_natural_teleport_point` | 同上 | skip |
| `tests/magic.rs` | `real_database_exposes_magic_infos` | `GameData::discover` | skip |
| `tests/npc.rs` | `real_database_npcs_and_scripts` / `real_database_shop_has_goods` | `GameData::discover` | skip |
| `tests/npc_input.rs` | `real_data_*`（经 `config_with_real_data()`） | `GameData::discover` + `resource/map/0.map` | 返回 `None` → skip |
| `tests/quests.rs` | `real_quest_script_parses_into_tasks_and_rewards` / `real_database_quests_and_npc_links` / `real_quest_can_be_completed_end_to_end` / `real_luke_script_gates_the_dialog_on_quest_completion` | `GameData::discover` + `Envir/NPCs` | skip |
| `tests/guild.rs` | `real_data_guild_scripts_and_creation_flow` | `GameData::discover` + 真实脚本 | skip |
| `tests/storage_mail.rs` | `real_npc_scripts_with_storage_page_parse` | `GameData::discover` + 脚本 | skip |
| `tests/conquest.rs` | `real_database_exposes_a_complete_conquest_record` | `GameData::discover` | skip |
| `tests/pk.rs` | `real_database_exposes_safe_zone_and_guard_template` | `GameData::discover` | skip |
| `tests/use_items.rs` | `real_database_exposes_potions_and_shield_spells` | `GameData::discover` | skip |
| `tests/newbie_flow.rs` | `non_gm_bookshop_purchase_and_reading_path` | `resource/map` + 脚本 | skip |
| `tests/social.rs` | `spouse_teleport_crosses_maps_when_the_spouse_is_elsewhere` | 曾用真实地形 | **合成地图**（`World::insert_map`，见下） |
| `tests/mount_wings.rs` / `tests/sound.rs` / `tests/real_files.rs` / `tests/real_assets.rs` / `tests/player_looks.rs` / `tests/client_fx.rs` / `tests/anim.rs` | 渲染/音频资源类 | `resource/` 里的 wzl/wav | skip |
| `tests/live_world.rs` / `tests/map_changed.rs` / `tests/ui_test.rs` / `tests/bigmap_panel.rs` / `tests/minimap_panel.rs` / `tests/ui_interaction.rs` | UI 行为类 | 只用**路径字符串**（不读文件） | 无影响 |
| `tests/png_export.rs` / `tests/samples.rs` | 导出 PNG / 读 `crates/mir-db/tests/data` 下的**仓库内**样本 | 临时目录 / 仓库内固定文件 | 无影响 |

## 约定（新写测试请照此办理）

1. **能合成就不读素材**（首选）：
   * 地图 → `World::insert_map("0", synthetic_map("0"))`，`synthetic_map` 造一张
     60×60 的 `MapCell::default()` 空地 + `MapVersion::V0`（`tests/maps.rs` 与 `tests/social.rs` 都有现成写法）；
   * 物品/NPC/怪物模板 → 直接构造 `MirDb { items: vec![...], ..Default::default() }` 再 `GameData::from_db(db, None)`；
   * 脚本 → `Envir::script()` 写临时脚本目录（`tests/npc.rs` 的 `Harness`）。
2. **确实要验证真实数据**时才读 `resource/`，并且必须**缺素材就跳过**：
   ```rust
   let Ok(data) = mir_server::GameData::discover() else {
       eprintln!("跳过：resource/ 里没有 Server.MirDB 样本");
       return;
   };
   ```
   错误信息里现在会带**当前工作目录的绝对路径 + 所有搜索候选**
   （`GameData::discover()` 的 `NotFound` 分支），排查时直接看 `eprintln!` 的输出即可。
3. 测试**不要**自己拼 `resource/...` 相对路径去 `expect()`：要么走 `GameData::discover()`，
   要么用 `env!("CARGO_MANIFEST_DIR")` 拼绝对路径并先判断存在性。

## 复现「没有素材」的环境（本地也能验）

```powershell
# 一步到位：构建测试二进制 → 丢进没有 resource/ 的沙箱目录 → 逐个执行
pwsh -NoProfile -File tools/dev/test-no-assets.ps1

# 只跑某个 crate
pwsh -NoProfile -File tools/dev/test-no-assets.ps1 -Jobs 8
```

脚本做的事：
1. `CARGO_TARGET_DIR=target/ci-sim cargo test --workspace --no-run`（只编译，不运行）；
2. 把 `target/ci-sim/debug/deps/*.exe` 逐个放到 `target/ci-sim/no-assets/`
   （该目录**没有** `resource/`）里执行，工作目录就是沙箱；
3. 任何一个二进制非 0 退出 → 整体 exit 1，并打印它的 `test result:` 行。

> 历史坑：`spouse_teleport_crosses_maps_when_the_spouse_is_elsewhere` 曾经直接读
> `resource/map/0108.map`，本地绿、CI 红（`NoFreeCell`）。现在改成合成地图，
> 这是本页第 1 条约定的来源。
