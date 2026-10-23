# 客户端体验后续清单（第 45 轮登记，未实施）

本清单只**记录**第 45 轮体验优化（自机移动插值/相机跟随）之后仍值得做的客户端体验项，
不在本轮实施；每项都给出 Crystal 出处、现状/影响、可验收标准与排序。
Crystal 参考版本：`github/Crystal` HEAD `0e315fe327192afe52c3d7357ddd1f5b7e26c5b8`。

排序口径：**P1 = 日常游玩高频可见**（走跑/战斗动作）；**P2 = 长时间沉浸感**（怪物外观/光照）；
**P3 = 表现力与设置**（天气/音效/UI）；**P4 = 便利性**（按键）。先做 P1，再按 P2→P3→P4。

| 排序 | 项目 | Crystal 出处 | 现状 / 影响 | 验收标准（建议） |
|---|---|---|---|---|
| **P1** | 他人/怪物移动插值 | `PlayerObject.cs:847-854`（他者 `DrawLocation += User.OffSetMove` 再减自身 `OffSetMove`）、`MonsterObject.cs:375-437`（怪物 `Movement/OffSetMove` 同公式）、`GameScene.cs:10328`（`Location` 叠加 `User.OffSetMove`） | 本轮只做自机；其他玩家/怪物/NPC 仍按服务端落点逐格瞬移，与自机平滑滑动并排时观感割裂 | 另开一路 live 用例：另一玩家/怪物走 1 格、跑 2 格，按 6×100ms 采样，位置逐帧单调推进、600ms 无残留；像素差与 Crystal 对照；不影响自机居中 |
| **P1** | 攻击变招与职业动作 | `PlayerObject.cs:1055-1075`（战士/刺客按随机/Shift 选 Attack1/2/3/4）、`:1277-1288`（弓手 `WalkingBow/RunningBow`、刺客 `Sneek`）、`Frames.cs:157-170` 帧表 | 目前统一播 `Attack1`；弓手/刺客/双手武器的动作辨识度缺失，攻击节奏与 Crystal 不完全一致 | 按职业 × 武器 × 动作矩阵断言 `Frame.Start/Count/Interval` 与随机分支；live 截图/帧序列逐项对照；服务端 300ms 伤害延迟仍对齐 |
| **P2** | 怪物逐图像帧表 | `MonsterObject.cs:274-276`（`Frames = BodyLibrary.Frames ?? FrameSet.DefaultMonster`）、`MLibrary.cs:522-545`（`.Lib` 的 `FrameSet` 读取）、`Frames.cs:25-31`（默认怪物帧表） | `!Mon.dat` / Crystal `.Lib` 的逐怪物帧表未解析，只有 §6.3 保守默认；部分怪物的受击/死亡段会回退到站立帧 | 至少对演示地图的怪物集合解析逐怪物帧表；扩展 `animation_assets` 逐怪物断言攻击/死亡段；与 Crystal 同帧号截图对比 |
| **P2** | 光照 / 昼夜 / 失明 | `GameScene.cs:10354-10356`（`Lights/Lightning/MapDarkLight`）、`:10589-10593`（`DrawLights(setting)`）、`:11010-11024`（`GetBlindLight` / `DrawLights`）；`Shared/Enums.cs:809`（`LightSetting`） | 客户端已接 `S.TimeOfDay.lights` 快照字段，但渲染全亮；夜间地图、洞穴、失明状态缺少明暗层次 | Day/Dawn/Evening/Night × `MapDarkLight` 截图与 Crystal 对照；失明下画面变暗；缺光效素材时优雅降级；帧耗时纳入性能预算 |
| **P3** | 天气粒子 | `GameScene.cs:10358`（`WeatherSetting` 字段）、`:12278-12440`（`UpdateWeather` 各天气）、`Shared/Enums.cs:13`（`WeatherSetting`） | 天气值已随 `S.MapChanged` 进入 HUD 文本，但没有雨/雪/雾/落叶等粒子表现 | Rain/Snow/Fog/Leaves/RedEmber/WhiteEmber 等逐类型截图或短录屏；切图/关天气能清理粒子；性能预算与关闭选项 |
| **P3** | 音效细分 | `MonsterObject.cs:217,601,1395`（`BaseSound = BaseImage*10`）与 `:117,141,872,1529`（按动作加偏移播放）、`PlayerObject.cs:3601-4208`（按地块选脚步音）、`SoundManager.cs:75-100`（查表/编号） | 目前走路统一地面音、受击不按武器、怪物无各自 `BaseSound`；长时间游玩声音重复感明显 | 怪物攻击/受击/死亡事件映射 `BaseSound+n`；八类地块脚步音；武器类别受击音；缺文件静默降级；声音事件单测扩展 |
| **P3** | UI 缩放 / 分辨率适配 | `Settings.cs:9`（`ScreenWidth=1024, ScreenHeight=768`）、`:78`/`:217`（`Resolution` 设置，官方只允许更宽）、`GameScene.cs:10398-10399`（视口偏移由屏幕尺寸计算） | 本作已有 4 档分辨率（800×600/1024×768/1280×720/1600×900）与 1/2/3 整数渲染缩放，但没有把「全部面板 × 全部组合」纳入验收；窄窗口/高缩放可能出现裁切 | 4 分辨率 × 3 缩放 × 关键面板（背包/装备/商店/任务/大地图/设置）截图与命中测试；无裁切/越界；窗口缩放后相机与面板锚点正确 |
| **P4** | 按键改键补全 | `KeyBindSettings.cs:126-176`（配置文件格式与 `RequireAlt/Shift/Ctrl/Tilde` 0/1/2 语义）、`:183-223`（默认键表） | 本作设置面板目前只有 6 个动作可改键（`KEY_ACTIONS`），其余固定；Crystal 支持完整键表 + 修饰键语义 | 可改键动作与 Crystal 键表逐项对照；冲突拒绝/恢复默认/持久化；修饰键语义测试；面板显示与实际输入一致 |

## 备注

- 本清单**只登记、不实施**；任一项开始实现前，先补 `docs/` 的 Crystal 行为对照（文件:行号）
  与可执行验收，再改代码。
- 与移动插值直接相关的残余风险（四类窗口未实测、≥600ms/跳变无录屏）见
  [`docs/testing/self-motion/README.md`](testing/self-motion/README.md)；
  自机插值行为与限制见 [`docs/client.md`](client.md) §4/§8。
- 优先级只是本轮登记建议，最终由尚书/门下按轮次预算调整；调整时保留出处与验收两列。
