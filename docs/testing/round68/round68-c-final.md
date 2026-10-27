# R68-C 最终收口核验
verdict：**PASS-WITH-PARTIAL；剩余必改 0，Lead 可在承接下列 PARTIAL 的前提下推进**；不作为真实素材或整轮全部范围通过声明。
基线 HEAD `2c69c70fff34913e59fabb779b5bd602a6c32ff5`；仅本次最终 SHA 有效。

| 必改 | 状态 | 最终证据 |
|---|---|---|
| 1 飞行非空 tick 重复 draw / center | **CLOSED** | [live:330–345](../../../crates/mir-client/tests/round68_effect_offset_live.rs#L330) 的 `assert_eq!(left.pixels, right.pixels, ...)` 直接比较 RGBA 字节；[live:546](../../../crates/mir-client/tests/round68_effect_offset_live.rs#L546) 两次 draw 后调用 helper，:547–559 换 center 整帧 oracle 与屏移检查仍在。 |
| 2 新入口边界直测 | **CLOSED** | 维持 [C2:8](round68-c-addendum.md#L8)；wire SHA 完全未变，边界/裁剪/缺帧直测保持。 |
| 3 目标断言布尔字段 | **CLOSED** | 维持 [C2:9](round68-c-addendum.md#L9)；历史布尔补录与目标首败匹配；共享旧原日志的归档限制继续承接。 |

最终 live SHA `39a032c74d71b52f23f06be653e31cbb81c96967691cea384064bd5965116038`（751行/30,666B）；wire SHA `c330a4b628749184461b595fe0681fbf161f042b578586ebd02483e390908c73`（569行/23,355B）。
摘要实现说明：FNV 仍先行比较并提供短失败消息；接受相等必须再通过实际 pixels 等式，摘要碰撞不能绕过字节比较，不再以 digest 充当唯一 oracle。

| 独立复跑 | UTC 起止（2026-10-27） | EXIT / 计数 |
|---|---|---|
| `cargo test -p mir-client --features mir-render/legacy --test round68_image_offset_wire --test round68_effect_offset_live --test round62_projectile_wire --test round63_projectile_wire` | 02:23:06.827–02:23:49.434Z | 0；live2 + wire2 + R62=5 + R63=3 = **12 passed / 0 failed / 0 ignored** |
| `cargo clippy -p mir-client -p mir-render --all-targets -- -D warnings` | 02:23:49.443–02:24:24.234Z | 0 |
| `cargo fmt --check` | 02:24:24.242–02:24:26.760Z | 0 |

独立全新 target=`target/round68/c/final/cargo-target`，TEMP=TMP=TMPDIR=`target/round68/c/final/tmp`，实际均绝对路径；命令/环境/UTC/EXIT 见 [runs.json](../../../target/round68/c/final/runs.json)，原始 [测试](../../../target/round68/c/final/logs/green.log)、[clippy](../../../target/round68/c/final/logs/clippy.log)、[fmt](../../../target/round68/c/final/logs/fmt.log)。
静态核对见 [static-audit.json](../../../target/round68/c/final/static-audit.json)：3实现 SHA 仍为 entities `96519746…`、scene `8a44a5a9…`、projectile `3528a4ee…`；无新增生产变动，protocol/server diff 均空；新增 ignore/allow=0，既有属性对 HEAD 无变化。
两 freeze SHA 仍 `70845aef…dc9de` / `c7d896e7…de32`；A attempt-1/2/3、B attempt-1/2/3/4 的7份 manifest 均存在且 SHA 各异；静态审计期间104份证据前后 SHA 一致。本次未写 A/B 历史目录。
独立读取 [A最终红JSON](../../../target/round68/a/attempt-3/red-failures.json)、[manifest](../../../target/round68/a/attempt-3/manifest.json)、[原始红](../../../target/round68/a/attempt-3/red-live-v3.log)：live test_sha=最终 `39a032c7…`，EXIT101，0 passed/2 failed；G4/G3 均首败 live:283:9（目标 offset 像素）；wire 红交叉引用未变 SHA 的 attempt-2，本次不重跑变异。
PARTIAL承接：A旧 manifest:394“真实资源字节”仍需正式措辞勘误；本终审明确两测试使用合成夹具，不构成真实素材视觉签核；B共享 isolated 旧原日志已替换，历史首败应引用C留存摘录与旧日志SHA。
PARTIAL承接：真实素材/设备视觉签核、全UI offset、spellfx居中迁移、DrawBlend/shadow/mask、历史R49–R67遗留维持原边界；仅核125/570切片。
范围：本C仅写本报告及 `target/round68/c/final/**`；未修改实现/测试/其他文档，未 commit/push，未执行 ci.ps1。证据脚本静态阶段曾遇系统GBK解码错误，改用UTF-8后仅补跑静态审计成功；三项独立门禁首次执行均EXIT0。
