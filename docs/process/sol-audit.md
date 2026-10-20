# gpt-6.1-sol 使用政策（质量判断层）

> 定位：**deepseek 干活，sol 把关。**
> 所有者指令（2026-10-21）：**不必精细核算 sol 成本**；把"重要、会直接影响项目质量、但强度不高"的事务交给 sol。
> 也就是：判断、评审、裁决、审计 → sol；实现、跑测试、翻仓库、批量重构 → deepseek。

## 1. 路由（已接入，热加载生效）

| 项 | 值 |
|---|---|
| provider id | `sol-relay` |
| model id | `gpt-6.1-sol` |
| 中转 | `https://yemoren.pull.run/v1`（与 `web_search` 同一中转，Responses API） |
| 密钥 | `GPT_LUNA_API_KEY` |
| 接入位置 | `~/.dsh/profiles/desktop/cordis.patch.yml` → `llm-pi-ai.config.providers.sol-relay` |

两种调用方式：
- **单发审计**：`workflow` 里单个 `agent(prompt, {provider:"sol-relay", model:"gpt-6.1-sol", schema})`——适合一次性判断；
- **管理岗成员**：AgentTeams `add_member(name, provider:"sol-relay", model:"gpt-6.1-sol")`——适合"每轮都审一次"的稳定职责（**每轮任务短、单任务闭环，避免上下文无限增长**）。

## 2. sol 承担的四类"管理层工作"

1. **规划审核（门下省级）**：方案是否真的在解决最高优先问题、验收是否可判定、是否在"造新 bug 修旧 bug"；
2. **风险裁决**：高危改动（协议/持久化/并发/安全/数据迁移）的对抗性评审，给 ≤3 条真实缺陷；
3. **判据设计**：阈值/判据是否测到了我们想测的东西（如 RSS 噪声地板、棘轮上限）；
4. **证据审计**：核对"报告说通过"与"产物/代码事实"是否一致（防止假绿）。

## 3. deepseek 继续负责（不要交给 sol）

- 全部功能实现、写测试、跑门禁、读日志、翻仓库、批量重构；
- 需要工具循环的探索（多轮读文件 → 上下文累积 → 每次调用重发 → 变贵）；
- 机械性核对（行数、链接、格式）。

## 4. 纪律（不精算成本，但别浪费）

| 规则 | 说明 |
|---|---|
| **只做判断** | sol 不出实现、不跑命令、不翻仓库；它拿到的永远是队长裁剪过的材料 |
| **单任务闭环** | 一次任务给一个结论（verdict + findings），不做长对话追问 |
| **输入给足但收窄** | 一次性把判断需要的证据给全（方案摘要、关键 diff、实测数字），但不塞整个仓库 |
| **输出要结构化** | `verdict` + ≤3–5 条 findings + 一行总结；不要散文 |
| **每轮 1–3 次** | 规划审核、高风险裁决、证据审计各至多一次；不刷次数 |
| **不做开放式头脑风暴** | "帮我想想架构"这种没有可判定产出的请求，不给 sol |

**判断标准**：这件事做错了会不会导致**返工 / 假绿 / 产品缺陷 / 玩家可感知的体验损失**？
会 → 交给 sol；只是费时间 → deepseek。

## 5. 标准调用模板

```js
// 单发审计：schema 强制短输出、无历史
const schema = { type: "object", properties: {
    verdict: { type: "string", enum: ["approve", "needs_revision", "reject"] },
    findings: { type: "array", items: { type: "object", properties: {
        severity: { type: "string", enum: ["low","medium","high","blocker"] },
        problem: { type: "string" }, required_fix: { type: "string" }
      }, required: ["severity","problem","required_fix"], additionalProperties: false } },
    one_line: { type: "string" }
  }, required: ["verdict","findings","one_line"], additionalProperties: false };

const r = await agent(brief, { label: "sol-review", provider: "sol-relay", model: "gpt-6.1-sol", schema });
```

## 6. 实战记录

| 日期 | 场景 | 成本 | 结果 |
|---|---|---|---|
| 2026-10-21 | 审计 `World::send_move_rejection` 节流回拉（≈600 in / 250 out） | ≈$0.0014 | 找出**真缺陷**：`move_rejections` 条目从不回收，`object_id` 随上下线单调增长 → 内存随历史会话数增长（deepseek 审查漏掉）→ 已列入第 25 轮修复 |
| 2026-10-21 | 第 25 轮方案管理层审核 | 待记 | 见看板 `JJC-20261021-004` |

## 7. 纪律

- sol 的意见**不是命令**：仍要在看板留下决策记录，并说明采纳/不采纳的理由；
- sol **看不到仓库全貌**（我们只给它片段），所以它的"没有发现问题"不等于没问题，**不代表可以跳过 deepseek 的测试与门禁**；
- 每次 sol 调用都要在回执里记**用途 + 输入规模 + 结论摘要**，便于按月核算成本。
