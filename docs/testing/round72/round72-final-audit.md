# R72 御史台独立终审（T）

- verdict：**BLOCK；必改 1（文档一致性），功能必改 0；当前不可推进**。仅审 HEAD `90a7516`，不推翻已披露 PARTIAL 裁量。
- 交付链：`69b2e0c → 2a31554 → d8b6157 → 90a7516`；复核 UTC 2026-10-27T13:12–13:17Z。

| 核验项 | 结论 | 独立证据与边界 |
| --- | --- | --- |
| 1 提交链/范围 | PASS | crates/tools 相对69b2e0c仅spellfx.rs +38、新测试+1660；tools空；crates相对2a31554空；d8b6157后仅manifest/sha256。 |
| 2 唯一门禁 | PASS | 原字节212729B、12 OK/0 FAIL；meta与manifest的head/exit/items/UTC/operator一致；ci.ps1:108 clippy不带legacy；T未跑CI。 |
| 3 清单机械复算 | PASS | 16/16 bytes/SHA；12 tracked blobs对delivery_head=d8b6157一致；无自引用；feature_head/gate.head=2a31554；审计前tracked工作树等于HEAD。 |
| 3 文档对齐 | **BLOCK** | README:32仍指tracked blob锚定2a31554，与清单d8b6157冲突；README:6与contract:3仍称D文档未提交，与提交链不符。 |
| 4 独立回归 | PASS | 全新cargo-target/tmp、DEV_DEBUG=0；四套29/0/0，fmt EXIT0，未把0 tests或编译失败计绿。 |
| 5 真红/oracle | PASS | A原红1/8、首败:1262；B七红/补证M2与M7均运行断言失败且恢复9/0；T另做旧语义红1/8→恢复绿9/0；G1独立二进制重算整帧三源一致。 |
| 6 诚实/PARTIAL | PASS-WITH-PARTIAL | C首轮BLOCK与C2通过均披露；C2旧裁定SHA快照差异已说明；未见肯定式全量/等价/视觉或legacy-clippy扩称。文档时态问题列必改1。 |
| 7 卫生 | PASS | 新增ignore/allow=0；无wire/protocol/server改动；审计前status仅两freeze；check_links/diff --check均EXIT0；审计新增本报告属授权。 |

- **必改1（Lead/D属主）**：统一README:32的tracked锚点为实际delivery_head口径，并修正README:6、round72-poison-contract.md:3“未提交”时态；修订后机械刷新清单及外置SHA，再追加独立复核。T未越权改交付物。
- 独立复跑：`cargo test -p mir-client --features mir-render/legacy --test <name>`，依次round72_poison_cloud_mapping **9/0/0**、round71_firewall_mapping **8/0/0**、round70_effect_blend_offset **9/0/0**、round63_projectile_wire **3/0/0**，均EXIT0；`cargo fmt --all -- --check` EXIT0。
- 上述四套UTC：13:12:00.060812–13:12:41.159907Z；fmt至13:12:43.507957Z。环境：CARGO_TARGET_DIR=`target/round72/t/attempt-1/cargo-target`；TEMP=TMP=TMPDIR=`target/round72/t/attempt-1/tmp`（实际传绝对路径）；CARGO_PROFILE_DEV_DEBUG=0。
- 清单：7295B，SHA256 `499158bb74c1ffbb0f4296d5e250e01710774ade1c2ec5cf3ffc0de325032411`，外置manifest.sha256同值；16/16、12/12 blob通过是本次90a7516快照结论。
- 门禁：212729B，SHA256 `6fe99c93ee0bfaed2ede492071d5c2822415e453df37a3ec6025436410b43a3c`；UTC开始12:36:59Z，mtime终点12:41:04.569739Z，meta按秒12:41:04Z；operator=lead、EXIT0、12/0。
- freeze-before：44844165B，SHA256 `70845aefd4dd689a55ab0618b36bbf692a4b33ebb5ee2f3a662dedc17a3dc9de`；freeze：45219533B，SHA256 `c7d896e7f5eee4706b943912c0c564d10d61c98d163143f051c2d632daecde32`；均未跟踪且与C留存值一致。
- C原报告：42行/6291B/`5f6addd04b331f5341f0b6032332e768d76fb0e7585571e6f7d1fea630f7ed6a`；C2：20行/2654B/`f7315ce0ffa8045ebdcf1b8903842c75af4b15cce7268b2ca2c6ebe1aaa716f8`；当前Lead裁定：3766B/`a5758e574ebeb718d0c025c43dcca0d9f9492f2167c6379e24e3c51a17873727`。
- T真红：物理archive副本，生产取69b2e0c、交付测试SHA始终`58f64ab2d30b69f3aea5c07a8975e988ca842bce45972499a6f93e120958baea`；13:15:03–13:15:11Z串行EXIT101、1/8/0、首败:939（调度不同于A）；恢复交付生产13:15:51–13:15:57Z EXIT0、9/0/0。
- T oracle：直接struct/zlib解留存G1的WZX/WZL索引1650，3×3、offset(-3,5)，重建800×600两格整帧；像素(381,261)=[248,4,0,255]；expected/actual/T SHA均`2d55e7ad6bf34f6c607e7eb0787884f3c77d6f70a6c043a9392ee32324f9908d`；不调用A/B/C计算helper。
- PARTIAL原号承接：R71严格NextMotion/初始100ms/每Process一步/149持久场与26移除/对象寿命；4000ms仅产品窗；GAP-1/2、R70-C2的rate0.8与src*0.8*alpha未实施，DrawColour/INVLIGHT/完整DrawBlend仍未闭合。
- PARTIAL原号承接：GAP-3/4/6 Light3、mask/tint/shadow、场景offset/AnimationOffset；GAP-5/R70-C1其他26技能及1620/其他调用点；R70-C3居中、B plan-M4近似残余、legacy lint债、真实素材/GPU视觉未签核、R71-C7及更早原号均不清零。
- 证据：[runs.json](../../../target/round72/t/attempt-1/runs.json)、[static-audit.json](../../../target/round72/t/attempt-1/static-audit.json)、[logs](../../../target/round72/t/attempt-1/logs/)；包括命令/UTC/EXIT/计数、diff原文、清单逐项值、门禁、红绿与oracle。
- 范围声明：只写本报告与target/round72/t/attempt-1/**；未改实现/测试/manifest/既有证据/freeze；未commit/push、未运行tools/dev/ci.ps1。manifest post-audit刷新由Lead机械完成；本BLOCK不等于功能失败。
