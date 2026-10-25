# R52 尚书省 G7 文档收口契约

主单：`JJC-20261025-004`；H 子单：`JJC-20261025-004-sub-a48c1ccb`（礼部 `libu`）。

## 终审事实与目标
- 刑部终审 `docs/testing/round52/round52-final-audit.md` verdict 为 **BLOCK**；唯一阻断是 G7 文档收口，不是产品代码或 attempt2 门禁失败。
- 功能提交基线 `46be80b`；门禁 attempt2 为 192,631 B、SHA-256 `4da8ea80…`，12/12 exit 0。此事实不替代 attempt3。
- README、roadmap、client、magic、backlog、R52 README 与 i18n 仍有过时“未实现/计划中”状态；i18n 旧 67/0 须保留为历史时点并新增实测 66/0。
- B4 实测默认 PoisonCloud L2=12000、L3=10000；plan:47 与 dispatch:61 的 L3=12000 是错误值，须明确更正并保留勘误来源。
- 需披露 `46be80b` 将 `crates/mir-server/src/magic.rs:1725`、`:1762` 两处生产 `debug_assert` 诊断消息翻译；只改字符串，逻辑未变。
- 更新时保留终审指出的 PARTIAL、历史失败史及边界，不得把它们写成已闭合。

## H 写入白名单
仅限以下文档及相关条目：
- `README.md`
- `docs/roadmap.md`
- `docs/client.md`
- `docs/world/magic.md`
- `docs/client-experience-backlog.md`
- `docs/testing/round52/README.md`
- `docs/i18n.md`
- `docs/process/round52-plan.md`：仅 L3 勘误相关行（当前 §G4 / 第47行）
- `docs/process/round52-shangshu-dispatch.md`：仅 L3 勘误相关行（当前 §B4 / 第61行）

其余均只读；逐处报告改前/改后与最终行号。验证证据写入 `target/round52/h/**`。

## 禁止事项
- 不改 `crates/**`、证据原件、门禁脚本、历史冻结大 JSON 或其他大 JSON；不删除/覆盖历史失败记录。
- 不改阈值或测试结果，不把旧扫描值伪装成当前值，不将 PARTIAL、socket、素材或历史缺口写成闭合。
- 不提交、不推送、不运行全量门禁；仅 Lead 可在 H 完成后提交文档批并启动 attempt3。

## H 验收命令（仓库根目录）
```text
py -3 tools/docs/check_links.py
py -3 tools/i18n/scan_hardcoded.py --player-visible --strict --fail-above 70
py -3 tools/i18n/scan_hardcoded.py --no-test --player-visible --strict --fail-above 0
git diff --check
```
全部须 exit 0；player-visible 结果应为 66，no-test 为 0。命令、cwd、原始结果与行号改动报告归档至 `target/round52/h/**`。

## 完成后的流程
礼部交付 diff 与证据 → Lead 审阅并提交文档批 → Lead 重跑门禁 attempt3 → 刑部 T3 复审 → 尚书验收 → 门下确认。未取得新证据前不得称 attempt3 或 T3 通过。
