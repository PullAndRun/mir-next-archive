# R74 独立终审

裁决：**BLOCK**（证据必改 1；功能必改 0）；被审 HEAD `8df57ef`，子单 `JJC-20261028-001-sub-05088f98`。
审计 UTC 2026-10-27T18:08Z 起；命令、起止 UTC、EXIT、原始日志/摘要见 [runs](../../../target/round74/t/attempt-1/runs.json) 与 [checks](../../../target/round74/t/attempt-1/checks.json)。

|核验项|结论|独立结果|
|---|---|---|
|1 提交链/范围/C2|PASS|提交链吻合；7095808 后仅八项文档；c222fe6 后 crates/tools 空差异；c1d0c76 仅 spellfx:309 注释。|
|1 测试/冻结保全|PASS|测试 1375 行/49,849B，SHA a2b329ce88a3640ad3609c804a2e589a4177a7de8c0e1e6df19c6d5c40013cc1；两 freeze 44,844,165B/45,219,533B 与历史 SHA 一致。|
|2 唯一门禁|PASS|原 log 213,918B/e2b1c25a…；meta 573B/4e622efe…；HEAD c222fe6、EXIT0、12/12、UTC/operator 与清单一致；字段 mismatch=0；未重跑全量。|
|3 清单原字节|PASS|25 artifacts 按 tracked/git blob、工作树与目录 inventory_convention 独算，mismatch=0；delivery_head=4567ba8、stage=pre-audit；无自引用；外置摘要吻合。|
|4 R74 隔离复跑|PASS|5/0/0，EXIT0；独立 target/TEMP，DEV_DEBUG=0。|
|4 历史/单元复跑|PASS|legacy 回归 R63/R70/R71/R72/R73=3+9+8+9+9=38/0/0；spellfx=4/0/0；均 EXIT0，与 A/B/C 对齐。|
|4 格式/静态检查|PASS|fmt --all -- --check、双包 all-targets clippy -D warnings 均 EXIT0。|
|5 真红原始日志|PASS|三方 15 份 M1–M5 红日志均编译完成、实际运行 5 测试并 panic 到目标断言，EXIT101；非编译错/零测试/skip；恢复日志全绿。|
|5 M2/M4 抽核|PASS|从历史 entities blob 重建 Swing→Magic、additive→false，三方 SHA 全匹配（A CRLF、B/C LF；M2 分支顺序差异无语义变化）；A/B 94 项摘要核验、C runs 日志摘要 mismatch=0。|
|5 精确 M3/C1 合同|FAIL|计划要求 offset_x 加法改减法且禁止语义替换；C 实际 x/y 各叠两次，虽真红却不是指定变异；详见下方必改。|
|6 G1–G5/Crystal|PASS|Magic/Magic2 起点 0/0/20；offset 一次、G3 每步整帧/缓存、G4 独算 [61,47,34]、G5 缺帧后独立 oracle 均核验。|
|6 Crystal 锚点|PASS|实读 Effect:8–61/81–96、MLibrary:658–711/748–801、MirGraphics/DXManager:353–389、Shared/Enums:70–80/805–815；Oracle.cs 不存在；四项题面纠偏属实。|
|7 文档诚实性|PARTIAL|均 pre-audit 候选、无正面禁称；NORMAL SourceAlpha/One、INVLIGHT BlendFactor/InverseSourceColor、rate/offset 表述正确；C 报告“五变异全部复现”须随 M3 补证纠正。|
|8 卫生/链接|PASS|起始仅两个 freeze 未跟踪；新增 allow/ignore=0；git diff --check EXIT0；bundled Python UTF8 links EXIT0（2040 相对链接；765 target 链接按工具规则跳过，清单另验）。|

最小必改（证据 E1，不要求功能/测试修复）：C 在独立副本按原计划精确执行 `x=add(offset_x) → sub(offset_x)`，保留前绿5/0/0→build0→目标断言RED101（位置/left/right）→原 SHA 恢复→后绿5/0/0，并补附录准确区分既有 double-offset 额外变异与计划 M3。
依据：[计划:73/77](../../process/round74-plan.md#L73)、[门下:15](../../process/round74-menxia-review.md#L15)、[C patch](../../../target/round74/c/attempt-1/mutation-m3/patch.json)、[C 报告:29](round74-c-evidence.md#L29)。现有 C-M3 不能冒充该必验项；终审未越权改 C 证据。
复跑过程披露：最初历史命令漏 legacy，R70–R73 各真实 EXIT101（无相应解码支持）；原日志保留，补齐与 A/B/C 相同 feature 后 38/0/0，不归因生产回归。
非阻塞精度项：G5“同场景/空帧”措辞宽于实测缺失条目，但独立 oracle 排除自证；contract/C“四侧裁剪”缺显式逐侧部分裁剪矩阵；README/roadmap 的“HEAD（D未提交）”为过时快照，交付头以 manifest 为准。
计数澄清：A 声明 37 项含外置冻结/脚本等，目录清单 32 项为不同口径；contract 文件名与门下预期名不同但链接可达，均不阻塞。
PARTIAL 原号承接：GAP-1/2、R70-C2、GAP-5/R70-C1、R70-C3、R71 时钟/对象、R72 B plan-M4、R73-T1、GAP-3/4/6；R71-C7 永久、E1 仅补救 CLOSED，R69 条件5、R68/R49–R70/R61–R67 照承。
严格时钟/寿命、完整 DrawBlend/BlendMode、rate0.8、其余技能、真实素材/GPU/设备视觉、全 UI/跨后端/Live 音频均未闭合；本审不升级历史结论。
未 commit/push，未改 manifest，未运行全量门禁；本报告及 T 证据之外无审计写入。
