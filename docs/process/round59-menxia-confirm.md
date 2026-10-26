# R59 门下省确认（menxia，gpt-6.1-sol，只读抽核）

- 主单 `JJC-20261026-006`（R59 客户端 125 渲染最小闭环）；确认时状态 **Review**；HEAD `1b51a85` = `origin/main`。
- 只读 `crates/`/证据，仅写本文件；未 commit、未跑 `ci.ps1`/ignored 测试。
- **结论：PASS-WITH-PARTIAL 收口成立**（绿/部分边界见 §4）。尚书验收 C 项不成立属**流程 readiness**（D1–D5 看板未回填），非交付缺陷；Lead 已以 main 回填六子单 `done` 成功转 Review，未绕 readiness。

## §1 抽核 6 项（PASS / 条件 PASS / PARTIAL）

1. **门禁原件独立重算 — PASS**。`attempt-1.log` 实测 **204,916 B**、sha256 `0d96e0610a7a9ec34d42523cf38ffeaa9bb7742093d9e6e2f544966069ab9721`，与 meta 全等；meta `head=edaa5cd…4485`、`exit=0`、09:24:22–09:27:11Z、`capture=full(*>)`。重算 **12 条 `^OK` / 0 条 `^FAIL`**（fmt/clippy/duo_smoke/workspace-test/协议生成/文档链接/TextKey/两项棘轮/老资源格式/图标/PG），末行 `==> 全部通过（12 项）`。时间序：`edaa5cd` 17:24:07(+08) → 门禁 17:27:11 收尾 → 门禁后仅 docs。
2. **提交树 — PASS（附范围注）**。`edaa5cd..HEAD` = **4 文件**（`docs/client.md` 8/8、`docs/testing/round59/README.md` 3/3、`final-manifest.json` +332、`round59-final-audit.md` +117），**无 crates/scripts/target**；四提交 diffstat：`7939133` 10/+2840−2、`edaa5cd` 6/+246−8、`cde01a3` 2/+11−11、`1b51a85` 2/+449。`git show HEAD:<p>|sha256sum` 抽核 **10 个**（projectile.rs 556f48ed…、mod.rs 60139daa…、sound.rs eafbdf23…、两个 R59 测试、analyze_usage.py 8c6c8cae…、magic.md、roadmap.md、run-checks.sh、asset-pipeline.md）**逐个命中** `01-tree-hashes.txt`（blob 版，1858 B，sha `3ba2fff6…`）；对 `edaa5cd` 同 10 项亦全等（D6 已核 16/16）。`01` 首版为工作树 CRLF 版，已另存 `00-tree-hashes-worktree-crlf.txt` 并重建 `01`（Lead memo 一致）。
3. **D6 M-1 闭合 — PASS**。`cde01a3` 三个 hunk 新行号恰为 `docs/testing/round59/README.md:21/:58/:67`（`@@ -18,7 +18,7 @@`/`@@ -55 +55 @@`/`@@ -64,7 +64,7 @@`）；`docs/client.md` **8 行**（`projectile.rs:240-301→422-439`、`:5-8/208-209/250-261→369/446-462`、`Effect.cs:174-181→172-191` 两处、`Effect.cs:196→201`、`:206-212→211`、`PlayerObject.cs:3546-3547→3552`、`projectile.rs:273-284→465-497`）。自跑 `py -3 tools/docs/check_links.py` → **exit 0**（1734 链接、2 篇 README 全通）。
4. **目标边界 — PASS（G4 PARTIAL 属实）**。实跑证据：render **16 passed/0 failed**（`d4/attempt-1/02-render.log`，含 570 命中与 dead 跳过）；assets **6 passed/0**（`03-assets.log`）；D3 生成 **requested=106 loaded=106 missing=0**、`verify ok`、PNG 106/0×0（`d3/attempt-1/02-generate.log:38/43`、`03-frame-count.log`）；sound **9 passed**（`d2/04-tests.log`）；Lead `run-checks` **9/9、总 EXIT=0**（`lead-verify/10-run-checks.log`）。G4 逻辑落盘（`M34-1.wav`/`M34-2.wav`、20341/20342），**真实可听 PARTIAL**（`take_sound_cues()` 未被 `app.rs` 音频通道 drain）。偏差台账 **7 条**（D-R59-1、P-2…P-7，`README:56-62`）完整；R60 承接 **6 项**（`README §4`）。全仓新增行**无**「125 全链／渲染完整／R59 完整／G1–G7 全绿／逐字一致／M34 真实可听已证／Effect-Missile 体系／Hero-真 socket」正面断言（命中仅见于「禁称」句）。
5. **治理 — PASS**。三件 sha 与登记全等：plan `5a233b58…`（13,225 B）、review `7fce80fa…`（10,257 B）、dispatch `99965f88…`（7,857 B）；尚书 acceptance `4d926beb…`（5,330 B）其「C 项」原文即「D1–D5 仍 Doing、父单 todos 为空 → `done` 被 readiness 拒绝」，本确认如实记为**流程就绪缺口**（已由 Lead 回填闭合），**不计交付缺陷**。
6. **假绿 — PASS**。`3cb1df1..HEAD` 的 `*.rs` 新增 `#[allow]`/`#[ignore]`/`cfg(feature)` = **0**；R59 两测试文件中文断言 = **0**；`sound.rs` 仅 5 条既存中文断言（`event.index()` 于 `3cb1df1:364`、`music_for_map` ×4），本轮新增 24 条中文均为注释。`run-checks.sh:245` 有 `ZERO_TESTS_RE='running 0 tests|test result: ok\. 0 passed'` 守卫（`:391` 判 FAIL）；i18n **66 处/33 文件**（阈值 70），二次扫描 0、exit 0（`lead-verify/12-i18n.log`）。

## §2 M-1 闭合判定

`cde01a3` 只改 2 文件 11 行，行号与 D6 M-1 清单一一对应，`check_links` 复跑 exit 0；M-1 **closed**，无残留必改。

## §3 PARTIAL / 偏差台账（7 条，逐条确认）

`D-R59-1` 无落点即不生成投射物/命中/`+2`（诚实降级、**非对齐** Crystal `:4699`+`:4701`）；`P-2` 事件无 ID/序号（`EFFECT_EVENT_LIMIT=16` 丢最旧）；`P-3` duration/count spawn 冻结（Crystal 每帧重算）；`P-4` draw 不叠逐帧 `ImageInfo.offset`；`P-5` 可听性 PARTIAL（素材已导入，通道未接）；`P-6` 无 Missile/Effect 类体系；`P-7` 无 Hero/真 socket。

## §4 收口口径（三行式）

- **绿 6 项**：G1 事件消费、G2 飞行插值、G3 命中 570、G5 素材 106、G6 文档/链接、G7 回归（门禁 12 项 + run-checks 9 项）。
- **PARTIAL 1 项（G4）**：音效逻辑与 cue 已证、**真实可听未证**；另 `D-R59-1` 为已知协议边界偏离。
- **依据**：`target/round59/{d4/attempt-1,d3/attempt-1,d2/attempt-1,lead-verify,gate}/*` 与上列 sha；`game-assets/` 不入库，素材证据只存 `target/`。

## §5 R60 承接（6 项）

事件 ID/`started_ms`；每帧重算 Duration/Count；`wzl` 逐帧 `offset`；命中生命周期落 socket 级用例；`M34-1/-2.mna` 与 106 帧 PNG 生成可复跑；Hero 实例/真 socket。

## §6 禁称清单（本轮不得出现）

「125 全链」「渲染完整」「R59 完整」「G1–G7 全绿」「与 Crystal 逐字一致」「M34 真实可听已证」「Effect/Missile 体系已移植」「Hero/真 socket 已做」。只许「最小闭环 + 已知偏离 / PARTIAL」。

## §7 非阻断事项

1. `final-manifest.json` 的 `head=edaa5cd` 未随 HEAD 重签（2 个 docs 为 M-1 有意改动：`client.md` 163,013 B `8b54448c…`、`README.md` 13,460 B `f8bbba0b…` vs 清单 163,030 B/13,385 B）；建议 R60 把清单自身 sha 与门禁 HEAD 纳入冻结。
2. 尚书验收件与本人确认件截至落笔仍在工作树未跟踪（本单不 commit）；工作树另有 2 个历史大冻结 JSON（44,844,165 B/45,219,533 B）。
3. PARTIAL/P-2…P-7 与 R60 六项为既定边界，不影响本轮收口。
