# R54 门下省评审

结论：**修改后放行（approve with changes）**。D1–D4准奏，以下五项作为派发与验收的强制补充契约；尚书须原样带入子单，C同步其文档白名单内的措辞，T逐项核验。均为现有可执行路径的验收收紧，无需新增生产接口或依赖，未发现必须封驳的致命阻碍。

主单 `JJC-20261026-002`；2026-10-26；独立门下；基线 HEAD/origin/main 实查均为 `08572ae8ca2420c95a8ecf02977b52c64241f235`。
计划原字节 SHA-256 `fd44643cfb52772b84efc7324a87f4c2d129dd4b74d256a61ce5b88ceaf23387`，124行；已读上游尚书验收、门下确认、AOI v2、刑部终审及团队证据纪律。
本次仅静态只读核查与本文落盘；未执行 cargo/全量门禁、未修改 crates、未 commit。本文先于首条 edict CLI 写入。

## 锚点抽查（计划值 / 实测值 / 判定）

表中路径相对仓库根；行号由本次直接读取原文件取得，不将上游报告当成源码抽查。

| 项 | 计划值 | 实测值与语义 | 判定 |
|---|---|---|---|
| 1 首帧 | Crystal `github/Crystal/Server/MirObjects/PlayerObject.cs:1741-1768`；Rust `crates/mir-server/src/world.rs:1881-1893` | C# :1743 y、:1748 x、:1758 cell插入遍历；Rust :1882/:1885/:1888/:1891 玩家/怪物/NPC/物品分组 | 匹配；只证差异契约，不证逐包等价 |
| 2 地图登记 | Crystal `github/Crystal/Server/MirEnvir/Map.cs:2361-2382` | :2365 Players.Add，:2372 cell.Add；:2377 Players.Remove，:2382 cell.Remove | 匹配；Rust seen为独立维护，不能视为Crystal同结构 |
| 3 离图 | Crystal `github/Crystal/Server/MirObjects/PlayerObject.cs:400-407`；Rust `crates/mir-server/src/world.rs:1961-1997` | C# remove/broadcast/Despawn/LeaveGroup；Rust :1975 seen.remove，:1979 aoi_remove，:1980 清自身seen，:1997 players.remove | 匹配；重复leave由:1962的?短路，无新增remove |
| 4 跑步 | Crystal `github/Crystal/Server/MirObjects/HumanObject.cs:2596-2602`；Rust `crates/mir-server/src/world.rs:2237-2244` | C# 摘旧格/RemoveObjects/换位/加格/AddObjects；Rust :2241 sync_player_players，随后怪物/法术场/物品 | 匹配；不回退R53修复 |
| 5 切图 | Crystal `github/Crystal/Server/MirObjects/PlayerObject.cs:4506-4509,4558-4583`；Rust `crates/mir-server/src/maps.rs:487-520,528-574` | C# :4506旧remove，:4562新Add，:4566 MapChanged，:4583 GetObjects；Rust :499旧观察者seen删除，:513旧NPC清空，:519新网格，:555本人MapChanged | 匹配；包序必须限定本人channel |
| 6 普通物品到期 | Crystal `github/Crystal/Server/MirObjects/MapObject.cs:358-375`；Rust `crates/mir-server/src/items.rs:1593-1638` | C# MapObject仅通用Despawn；实际到期触发在 `github/Crystal/Server/MirObjects/ItemObject.cs:129-135`，严格Time>ExpireTime后RemoveObject+Despawn；Rust :1629同为>，:1638 remove_ground_item | **部分匹配：计划缺到期触发锚点，须补；不可只引Despawn证明到期** |
| 7 尸体撤销 | Crystal `github/Crystal/Server/MirObjects/MapObject.cs:339-375`；Rust `crates/mir-server/src/world.rs:5121-5137` | C# :363广播remove、:375 Node=null；Rust :5133 seen删除成功才发remove，:5137撤网格 | 匹配为撤销端；真实死亡→tick须由新增测试证明，不能直接调用forget_monster充数 |
| 8 广播 | Crystal `github/Crystal/Server/MirEnvir/Map.cs:2437-2448`；Rust `crates/mir-server/src/aoi.rs:267,273-279`、`crates/mir-server/src/world.rs:5557-5590` | C# :2441 Players逆序；Rust :267 sort(y,x,id)，world :5569过滤exclude和actor；:5577另有batch分支 | 匹配；批处理覆盖不能被直接send例外推 |
| 9 玩家差分 | Crystal `github/Crystal/Server/MirObjects/HumanObject.cs:2596-2602`；Rust `crates/mir-server/src/world.rs:5281-5333` | Rust BTreeSet，:5288/:5289 difference；:5306先remove、:5309后add；:5333写新集合 | 匹配，批内ID序；与广播空间序区分 |
| 10 Frost | Crystal `github/Crystal/Server/MirObjects/HumanObject.cs:5967-5994`；Rust `crates/mir-server/src/magic.rs:4098-4166` | C# 同图/有效目标/落点2/MAC>0；Rust :4098 hittable、:4101 damage>0、:4126等级+2、:4131/:4150分母100，4/2跳与1000ms | 匹配；概率<=技能3意味着4%，不是3% |
| 11 同队/同会 | Crystal `github/Crystal/Server/MirObjects/PlayerObject.cs:4650-4674`；Rust `crates/mir-server/src/world.rs:2503-2528` | C# :4661 Group、:4663 Guild；Rust :2521 same_group、:2522 same_guild，:2524 EnemyGuild=false；实际谓词 :5713-5725 | 匹配；两个None不视为同会/同队 |
| 12 R53盲区 | Rust `crates/mir-server/tests/round53_frost_pvp.rs:555-588` | :559是60→63，:588是60→60；:582阈值24..72；没有精确60→62正侧 | 匹配，R54新增有必要 |

附：输入候选中的 `github/Crystal/Server/MirMap/Map.cs` 不存在；glob实查仅 `github/Crystal/Server/MirEnvir/Map.cs`。计划采用后者，计划本身无此路径错误。

## 单轮范围与可证伪性

A/B只增测试，C单写文档，T独立审计；A/B编辑互不重叠、cargo串行，DAG可在单轮交付补证矩阵。范围是补证交付，不承诺五路由全PASS或补齐所有历史缺口。
A的真实enter/leave/change_map/死亡/tick、双向精确集合、指定ID恰好一次、后续不可达可证伪；私有断言只作观测，不替代动作。普通物品必须Some(UserItem)，金币只能对照。
B的60→62、1200真实cast与延迟tick、HP因果、双侧计数及固定24..72阈值可拒明显错误分母；Group/Guild初始拒绝与落地重检分别验证两个时点。
已有可行组队/行会路径：`crates/mir-server/tests/group.rs:219-227` invite/accept；`crates/mir-server/tests/common/mod.rs:378-393` create_guild/guild_invite/guild_join。
落地重检实在 `crates/mir-server/src/magic.rs:2679-2711`，:2710重新调用PK谓词。静态存在不等同本轮已运行。

## D1–D4 裁决

- **D1 批准**：逐路由/子项PASS、PARTIAL、FAIL分别报告；NPC无真实回收入口与maps四例无样本保留PARTIAL，AOI全量保持开放。实际断言失败须FAIL且保留原件，不准降成“缺证PARTIAL”掩盖。
- **D2 修改后批准**：Group同队与Guild同会分别真实建关系/施法；EnemyGuild排除。采用M1的因果护栏，禁止仅is_err/无毒判绿。
- **D3 修改后批准**：只准 `crates/mir-server/src/world.rs:5906-5908` 既有cfg(test)模块追加私有观测；无新增生产getter/feature。采用M2，库/集成双目标发现，所有发现目标均执行。
- **D4 批准**：门禁绑定受测HEAD、diff、候选文件原字节哈希；提交一后核实树内容一致。第二次仅审计/治理文档，代码、测试、CI、配置、生成输入或其他受测行为变更均重跑。文档例外仍须链接/diff检查与T复核；按本次实际脚本项数/总exit判定。

## 必须修改（尚书派发时即生效，共5项）

1. **M1 high，B/T**：拒绝例明确断言PK拒绝原因，施法前确认存活/同图/落点/非安全区/法力与冷却合法、关系真实形成；落地重检例先证明cast成功且pending存在，再经invite/join改变关系，再跨延迟tick断言HP/毒/伤害包不变。清空建关系杂包，设置同条件异队/异会正对照；禁止直接写group_id/guild或注毒造因果。保持概率样本间HP/毒/冷却重置，不能借上次状态命中。
2. **M2 med，A/T**：计划:44“对发现用例的目标”改为“对所有发现用例的目标逐个运行”；各前缀列两次--list、完整例名/目标/实际执行数，合计>=1且所有发现例均执行，0 tests不得计通过。world.rs:5906确认确有既有cfg(test)模块。
3. **M3 high，A/T**：明确NPC首帧使用现有生产 `spawn_npcs`（`crates/mir-server/src/npc.rs:687-748`）并核实返回ID/网格/seen；:759-762的spawn_npc标为测试/GM入口，不能冒充生产加载。测试fixture可提供最小GameData，但动作必须走World；任何NPC回收不得手删容器；无路径则该子项PARTIAL。
4. **M4 med，A/C/T**：补上表第6项ItemObject到期触发锚点与严格>边界；到期前保持、到期后World tick撤销、再tick零重复remove。将广播默认直接send与 `crates/mir-server/src/world.rs:5577-5582` batching路径区分：覆盖真实tick批处理后逐接收者完整解码序列，或明确batching未测子项PARTIAL；不得以跨channel取包时刻证明全局发送序。
5. **M5 med，尚书/C/T**：计划:103的“A1/A2/A4/A5/A6”改为明确的路由名/前缀与矩阵子项，避免与本轮A/B/C/T或历史A2混淆；冻结19项承接映射，每个结果写资源/动作/断言/有效数/原exit/HEAD/候选哈希。首红保留独立attempt；不把资源提前return、未执行和断言FAIL混为PASS。

## 风险、PARTIAL与放行意见

NPC公开区段只有查询/生成；本次grep未见npcs.remove/retain/clear回收，不能以搜索未见宣称绝对不存在。无需为本轮新增despawn接口，继续保留源码依据与缺口。
真实maps样本本审未查找或运行；四例是否可用由A核实，缺样本不下载/不改DB，reported20与有效16须分记。
广播batching、到期严格>、护栏无伤害的错误原因、两目标漏跑是新增高风险点；按M1–M4收紧，无法证明的子项明确开放。
R52十二项、R49 P05/P08、R51遗留、125生产者、真socket/窗口、原生dash、完整PK矩阵/G1/G6/EnemyGuild、门远端/精确5000ms/跑步撞门/初始已开门/同名实例仍开放；本轮不得借全量绿灯清零。
全量前检查soak.lock，有锁等待合法释放，不删锁、不KillStale；原始exit/起止时区/字节B/sha与受测树绑定不可缺。
**放行**：允许转Assigned并交尚书登记A/B/C/T；五项强制契约随派单落地后执行，T验收逐项核对，真实失败返工/另轮裁决，不改生产逻辑修绿。
