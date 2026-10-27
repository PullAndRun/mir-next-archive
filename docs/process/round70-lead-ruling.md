# R70 尚书验收差异裁定（Lead）

依据：尚书 attempt-1（gpt `b7e019d3`）在 2026-10-27T07:47Z 前后超时失败、未落盘验收文档，仅留早期差异消息与 `target/round70/shangshu/attempt-1/**` 证据；其提出的两项差异经 Lead 独立复核裁定如下。

## 差异 1：`cargo clippy` 加 `--features mir-render/legacy` 出现 2 条 `unneeded return`（EXIT101）

- 实际报错位置：`crates/mir-render/src/minimap.rs:353`、`crates/mir-render/src/tiles.rs:311`。
- 归属：两文件最后一次提交为历史 `f37a402`（R49 前老格式解码器门控批次）；`git diff 6a08c65..94808d5 -- crates/mir-render/src/minimap.rs crates/mir-render/src/tiles.rs` 为空 → **既有 lint 债，非 R70 引入**。
- 官方门禁 `tools/dev/ci.ps1` 的 clippy 步骤**不加** `mir-render/legacy` 特性且 EXIT0；R68 独立终审也已把同样 2 条列为既有、不计入通过范围（R68 审计披露）。
- 裁定：属 PARTIAL 披露项（既有 lint 债），**不是 R70 必改**；R70 的验收以官方门禁 clippy（EXIT0）与 T/T2 最终审计（无额外特性）为准。尚书 attempt-2 可在限定范围下 ACCEPT-WITH-PARTIAL，不得据此判 BLOCK。

## 差异 2：看板子单状态与提交链计数

- 看板：A=`JJC-20261027-007-sub-d0d00e43` Done（06:30:57Z）、B=`sub-845fa668` Done（06:36:27Z）、C=`sub-1bdf62e4` Done（06:39:42Z）、D=`sub-67ec1a50` Done（07:36:25Z，Lead 以 main 身份对 D 交付事实回填 delegate-result，EXIT0）；主单 Doing → **done 前提已满足**。attempt-1 看到的四子单 Doing 是回填前快照/缓存读。
- 提交链：`72d9313..94808d5` 实测 6 条里程碑（非 16）：`b11deac`（过程文书）→ `6a08c65`（功能）→ `89daaaa`（C 证据）→ `5f0d781`（文档批）→ `4d3d84b`（T1 报告+勘误）→ `94808d5`（T2 复审+post-audit 清单）。Lead 提示中的“16”为笔误，验收按 6 条核对。
- `crates`/`tools/dev/ci.ps1` 在 `6a08c65..94808d5` 区间零差异。

## 结论与授权

- attempt-1 因超时未产出验收文档，不构成 BLOCK；由其遗留差异经本裁定闭合。
- 授权新增尚书 attempt-2（gpt 子代理）按裁定事实重新验收：期望 **ACCEPT-WITH-PARTIAL（限定「混合特效逐帧偏移生产入口」切片）**，必改 0；证据写 `target/round70/shangshu/attempt-2/**`，验收文档写 `docs/process/round70-shangshu-acceptance.md`，并执行一次 `done JJC-20261027-007 --as shangshu`。
