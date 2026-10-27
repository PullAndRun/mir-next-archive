# R70 尚书省派发契约

- 主单 `JJC-20261027-007`：Assigned→Doing；基线 `72d9313e225dbb825cece84ffd5753e0a1e25612`；本页登记派发，不表示实现或验收通过。
- 依据：[计划](round70-plan.md)与[门下附条件准奏](round70-menxia-review.md)；C1–C6均为不得绕过的收口条件；执行者仅由Lead spawn。

|子单|真实ID|角色/模型|depends_on（阶段）|
|---|---|---|---|
|A|`JJC-20261027-007-sub-d0d00e43`|兵部 bingbu / deepseek|B:fixture-ready|
|B|`JJC-20261027-007-sub-845fa668`|工部 gongbu / deepseek|夹具=[]；终绿=[A:implementation]|
|C|`JJC-20261027-007-sub-1bdf62e4`|刑部 xingbu / gpt|A:implementation+B:final-green-and-four-mutations|
|D|`JJC-20261027-007-sub-67ec1a50`|礼部 libu / gpt|C:verified+Lead:commit-and-unique-full-gate-EXIT0|

- DAG/启动：B夹具与独立oracle先落盘→A实现→B终绿/四变异→C独立核验→Lead提交+唯一全量门禁→D只据实写文档；Lead保存子单↔agent ID映射，无未满足依赖或重叠写入才可启动。
- CLI差异：真实`--help`及源码`cmdDelegate`无结构化depends_on/model选项；阶段依赖已写入delegation.return_spec、父memo与四todo详情，未伪造结构化字段；delegate自动创建子单Doing不代表执行者已启动。
- 当前明确指令覆盖计划A/B官署及D/门禁顺序；父todo已修正为A兵部/B工部。门下C6原为C/D后Lead门禁，本次D在门禁后仅文档实录，其他门禁条件不变，终审须知此顺序差异。

|子单|写白名单|逐子单接受标准|
|---|---|---|
|A|`crates/mir-render/src/entities.rs`、`crates/mir-render/src/scene.rs`、`target/round70/a/**`|私有共同实现+生产draw_effect_blend+scene薄转发，红绿自测、两套T68；B之后终绿；满足C1/G1–G5。|
|B|`crates/mir-client/tests/round70_effect_blend_offset.rs`、`crates/mir-render/src/frame.rs`仅原单测区、`target/round70/b/**`|先夹具/oracle；≥7命名测试、0 ignored/0 filtered；C2全部覆盖，最终同SHA四变异红绿；满足C2–C4/G1–G6。|
|C|`docs/testing/round70/round70-c-evidence.md`、`target/round70/c/**`|独立T70/两套T68/四变异全部复跑、核工具链/计数/SHA/范围卫生，写C报告；不代改A/B；满足C3–C4/G6，可BLOCK。|
|D|`docs/testing/round70/{README.md,round70-contract.md,render-contract.json,final-manifest.json,manifest.sha256}`、`docs/client.md`仅§5.35、`docs/roadmap.md`仅本轮承接、`target/round70/d/**`|逐G关联真实命令/断言/UTC/EXIT/SHA；UTF-8无BOM/LF、本地链接有效；C5原号PARTIAL承接、记录Lead G7实跑。|

- **C1→A，C复核**：`fn draw_effect_impl(&mut self, frame: &mut Frame, kind: FxKind, index: u32, x: i32, y: i32, apply_offset: bool, additive: bool) -> bool`；公开`draw_effect_blend(..., apply_offset: bool) -> bool`，scene同名薄转发；仅entities/scene生产修改。
- 旧draw_effect=(false,false)，旧with_offset=(true,false)，新blend=(参数,true)；ensure_image→当帧metadata→可选saturating_add一次→裁剪；Add调用Frame::blit_add，直接借缓存，不克隆Image/不新增逐帧分配；旧Over整帧与舍入、存在性不变，有图屏外true、缺图false。
- 不动mapping/time/wire/server/magic、图库帧号、播放时长、UI/事件消费者、frame算法、freeze/历史证据；不加依赖，不把CPU Add子集称D3D RGBA或完整DrawBlend等价。
- **C2→B，A配合，C复核**：期望先由原始夹具/预定像素独立计算，禁调用生产blit/blit_add/add_channel或复制实现生成oracle；覆盖±8正负跨帧/单次偏移、屏外移入/部分裁剪/负坐标/i32极值、缺图/屏外返回、false、scene转发、旧Over/交替缓存无泄漏；alpha=0/128/255、非黑背景/饱和/alpha行为；两套T68全绿。
- T70：`cargo test -p mir-client --features mir-render/legacy --test round70_effect_blend_offset -- --test-threads=1`；T68分别`--test round68_image_offset_wire`和`--test round68_effect_offset_live`（同包/feature）；半透明补测`cargo test -p mir-render --lib`，不能filter成0tests。
- **C3→B执行/C独立复现**：最终API已编译的有效生产语句M1关闭Add offset→G1位置；M2偏移前裁剪早退→G2移入像素；M3 Add改Over→G3非黑底通道；M4忽略false→G4绝对落点；四者同最终测试/工具SHA的green-before/red/green-after，逐次恢复原字节。编译错/0tests/import失败/skip/ignore/未知基线失败均不充红。
- **C4→B/C各自负责**：独立物理副本，逐文件源/副本bytes/SHA与文件ID/链接数证明非symlink/junction/hardlink；独立cwd/CARGO_TARGET_DIR/TEMP/TMP/TMPDIR/out，仅可复用只读下载缓存。C独立重跑T70/T68与四变异；保全首败原件、命令/UTC/EXIT/计数/断言/diff/恢复字节；测试或工具SHA改变后四红和C全部重跑。
- **C5→D，C/Lead核收**：逐G仅真实证据；GAP-1–6、R70-C1–C3、R68、R49–R67按原号承接；R69条件5永久PARTIAL。本轮完整C2/公共API全面收敛、GAP-1/完整DrawBlend/rate/INVLIGHT、spellfx/GameScene条件、shadow/mask、全UI/真实素材/GPU设备均不升级；规划不冒充执行。
- **C6→Lead，D记录/终审核收**：只有Lead执行`pwsh -NoProfile -File tools/dev/ci.ps1`，最终生产树唯一全量EXIT0且逐项成功；无SkipTests/SkipPostgres等降级参数，不借R69门禁；门禁后再改生产必须重跑，失败/超时保全新attempt，不降级。
- 风险：冷构建/数据库预算不足、oracle自证、假红、可变路径共享、SHA漂移、Add污染旧Over、把子集消账；分别以追加attempt、独立oracle/物理副本/四变异、C复跑、G5/T68与逐项PARTIAL表拦截；回滚只交Lead最小逆向diff，不重置工作区或改历史freeze。
- 锚点纠正：Crystal真实路径是`github/Crystal/Client/MirGraphics/DXManager.cs:353–389`，计划链接正确，不使用误写的MirScenes路径。
- 证据根：本次[CLI回执](../../target/round70/shangshu/attempt-1/cli-receipts.json)、[子单/DAG](../../target/round70/shangshu/attempt-1/subtasks.json)；后续`target/round70/{a,b,c,d}/attempt-N/`只新建；尚书本轮只写本页与`target/round70/shangshu/**`。
- 真实CLI：2026-10-27T06:19:27.583Z–06:19:28.810Z，18次均EXIT0；state Doing于06:19:28.379Z；validate于06:19:28.454Z回执226条/schema v3.1；show确认Doing与4项not-started，四子单ID/官署正确；未启动执行者、未跑cargo/ci、未commit/push。
