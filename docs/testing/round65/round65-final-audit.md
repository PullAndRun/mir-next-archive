# R65 刑部独立终审（限定 G1 单 UI 本地补证）

- Verdict：**BLOCK**；功能/证据核验可接受并保留 PARTIAL，但按本次“无中文 assert”字面要求，中文诊断未达标；不得称 R65 最终通过。
- 审计基准：功能 `3730e2b`；文档 `ffed7a9`；终审者 gpt-6.1-sol，只读源码/旧证据；必改 **1**（见卫生项，不是功能缺陷）。
- 独立环境：`CARGO_TARGET_DIR=target/round65/t/attempt-1/cargo-target`，`TEMP=TMP=TMPDIR=target/round65/t/attempt-1/tmp`（实际均绝对路径）；未复用 A/B/C 二进制。
- [命令/UTC/EXIT/计数/日志 SHA](../../../target/round65/t/attempt-1/runs.json)：UTC 2026-10-26T20:04:45Z–20:06:23Z，14 命令全 EXIT0；10 测试目标共60次通过，0 failed/ignored，无空跑。

|独立命令（cargo 前缀；测试均加 `-- --nocapture`）|通过/失败/忽略|C 自报|结果/EXIT|
|---|---|---|---|
|`test -p mir-client --lib r65_`|5/0/0|5|PASS/0|
|`test -p mir-client --bin mir-client r65_`|1/0/0|1|PASS/0|
|`test -p mir-client --lib r64_`|7/0/0|7|PASS/0|
|`test -p mir-client --bin mir-client r64_`|1/0/0|1|PASS/0|
|`test -p mir-client --test round65_projectile_lifecycle`|5/0/0|5|PASS/0|
|`test -p mir-client --test round64_event_reclaim`|4/0/0|4|PASS/0|
|`test -p mir-client --test round64_projectile_ack`|3/0/0|3|PASS/0|
|`test -p mir-client --lib ui::projectile::tests::`|26/0/0|26|PASS/0|
|`test -p mir-client --test round63_projectile_wire`|3/0/0|3|PASS/0|
|`test -p mir-client --test round62_projectile_wire`|5/0/0|5|PASS/0|
|`fmt -p mir-client -- --check`；`clippy -p mir-client --all-targets -- -D warnings`|无格式差异/告警|一致|PASS/0、0|

## 必做项核销与证据
- 1 范围 **PASS**：[静态核验](../../../target/round65/t/attempt-1/static-check.json)保存 git show 两提交、三文件 diff、门禁后两项 crates 空 diff、冻结抽样命令/UTC/EXIT0。+413 net/+129 app 位于 cfg(test)，B 仅649行新测试；无生产逻辑变更或白名单外新增改动。
- R61–R64 **PASS**：[补充核验](../../../target/round65/t/attempt-1/supplemental.json)四轮 `git diff 2deacee -- docs/testing/round6N` 全空；既有两份 monster-native 未跟踪冻结 JSON 未触碰。
- 2 门禁 **PASS**：唯一 gate 目录仅 attempt-1 log/meta；重算206593B/SHA256 `3c9f40fcf0b889bb51291ca9ba88125b80ca894ed0fefc65082e25300c64bfb3`；meta EXIT0、12 OK/0 FAIL 与GB18030原日志汇总一致。
- gate_head=`3730e2be7983b1cbbef704103b05a804bfb02489`，UTC19:49:35Z–19:53:18Z（2026-10-26）；当前 crates 与该功能提交全等。独立 cargo 定向复跑不是第二次门禁，未执行 CI。
- 3 独立复跑 **PASS**：上表全与 C 一致；命令、起止UTC、EXIT及完整双流日志见 runs；fmt/clippy/文档链接/空白检查均独立实跑。
- 4 真红 **PASS（静态复核）**：A trim-noop/ack-none/wrong-prefix 原始EXIT101，分别3/1/1失败；6绿+11红完整测试块的元数据、宿主SHA、字节切片、红绿SHA全一致。
- B 四份649行/28113B测试均SHA `dfef7383843f9e72c97f6999c058281c0d61ddf5f00b64d878d001c9667333e2`；两红均1 passed/4 failed/EXIT101，wrong-prefix身份left1/right3；不是编译错或空跑。
- 5 清单 **PASS（刷新后）**：原8项bytes/lines/SHA及git blob独立全匹配；README/contract/C原tracked=false是提交前快照，已更正true并以ffed7a9 blob注明；8项原哈希不变。新增本报告仅工作树、预期纳管，不冒称3730e2b已有blob。
- C SHA `8cf5d5848e829411eb6aa5ce193558acebc35ba9d05b65b06a13a2a4384d530a` 正确；README/contract明确门禁PASS仅本次、验收未发生；PARTIAL台账未扩称全局水位、容量或多消费者支持。
- 6 契约 **PASS/PARTIAL**：[net:3030–3074](../../../crates/mir-client/src/net.rs#L3030)、[A测试:9381起](../../../crates/mir-client/src/net.rs#L9381)、[B:148–310](../../../crates/mir-client/tests/round65_projectile_lifecycle.rs#L148)实证1-based=`reclaimed+i+1`、total=len+reclaimed、精确前缀/重复后退/夹尾/旧代拒绝。
- [app:210–229及3902起](../../../crates/mir-client/src/app.rs#L210)实证非零严格前进ack；[B:315–376](../../../crates/mir-client/tests/round65_projectile_lifecycle.rs#L315)逐项clone值独立不钉权威；[net:3869–3897及9700起](../../../crates/mir-client/src/net.rs#L3869)等待predicate且不自动ack；同Receiver不是并发消费者保证。
- Crystal **PASS**：实际读 Effect.cs:144/155/164/191–195活动表增删；GameScene.cs:4687–4711缺源return/出生音/存活目标570与命中音；PlayerObject.cs:750/966–967/3546–3550与MonsterObject.cs:346/528–529符合SkipFrames/FIFO/目标点口径；ServerPackets.cs:3564–3578仅Spell/Source/Destination，无ack序号。
- 7 卫生 **FAIL/PASS**：新增ignore/allow=0、历史ignored属性7无新增，wire协议diff空，links与git diff --check均EXIT0；但“无中文assert”若按字面包含诊断字符串则FAIL：仅单行即28处，另有多行，例[B:88](../../../crates/mir-client/tests/round65_projectile_lifecycle.rs#L88)与[app:3913](../../../crates/mir-client/src/app.rs#L3913)。未发现依赖中文UI文案相等的断言；不得混称中文诊断为0。
- 必改M1：由Lead/原授权方明确“无中文assert”验收口径；如确为零中文诊断，需另获源码修改授权并更新提交/红绿证据/门禁，不得由终审越权改源码；最小复现为读取B:88或运行 supplemental 脚本，证据列出28条。
- 流程披露 **PASS**：generated_by坦陈D超时、仅README/contract产出、Lead机械补齐manifest；不伪称D完成。原生成器及校验日志存在，身份披露充分；[只读表达式探针](../../../target/round65/t/attempt-1/d-generator-probe.json)实证换行计37/meta11（曾因工具渲染转义误疑，已反证撤回）。本次独立复算补强原校验未逐项检查lines/tracked的不足。
- 保留 **PARTIAL**：G1.2仅clone离线值独立；多消费者/并发/全局最低水位未证；G2整项移出；事件backlog、clone、known_positions可增长；total饱和仅构造且未证恢复/唯一序号；服务端、容量/LRU、素材/设备音频、R61 P01–P12/R62四项/R63 P1–P7/R64六项原编号全保留。
- 边界：未发现功能缺陷；仅写本报告、刷新本清单及T证据；不改crates/A/B/C/D旧证据/冻结材料，不commit/push，不跑 tools/dev/ci.ps1；尚书验收NOT_YET、门下未确认、board_done=false。
