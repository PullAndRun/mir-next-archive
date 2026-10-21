# 新手链等级门槛与 canary 记录（第 25 轮）

## Server.MirDB 任务等级（只读 dump）
命令（临时 example，未入库前可删除）：`cargo run --release -p mir-server --example quest_levels`

```text
1 | Assistant's Request          | min=1 max=255 required_quest=0 class=31 type=0
2 | The CraftLady's Request     | min=1 max=255 required_quest=1 class=31 type=0
3 | Talk with the Butcher       | min=1 max=255 required_quest=2 class=31 type=0
4 | Hunt for the Butcher        | min=1 max=255 required_quest=3 class=31 type=0
7 | Meet the Warrior Instructor | min=4 max=255 required_quest=0 class=1 type=0
8 | Test for the Fencing Skill  | min=4 max=255 required_quest=7 class=1 type=0
```

结论：
- quest 1/3 是 1 级可接的对话/前置任务；quest 7/8 要求 **MinLevel=4**。
- 这解释了 canary5 的 `level 1 does not meet the quest requirement (4..=255)`，**不是脚本/NPC/quest_ids 问题**；
- Jane 的 `file=BichonProvince/BorderVillage/Jane` 正确，服务端确实把 `C.AcceptQuest{7}` 按 Jane 处理。

## canary5（未升级，直接接 quest7）
```text
[OK] 清洁初态 角色 pt986205/PT986205；金币0，经验0，completed_quests=[]（不含7=True）
[诊断] Jane NPC: object_id=3 quest_ids=None（不在 S.ObjectNPC 可见集）
        file=BichonProvince/BorderVillage/Jane
[聊天] 接取任务失败。
Server WARN: 接取任务被拒绝 session=1 error=level 1 does not meet the quest requirement (4..=255)
[FAIL] 接取 —— 发 C.AcceptQuest{7} 后未收到 S.ChangeQuest{7}；失败聊天=['接取任务失败。']
```
walker 已到达 Jane 邻格（约 4:11），失败点明确在等级门槛；失败聊天端到端可见。

## canary6（B 方案：自然升级到 4）
- 新流程：clean → `newbie_level_to(4)` → Jane → Master_Wa → 购买。
- 实测：13:15 仅到 exp=81（Hen exp=9），击杀 9 只 Hen；到 4 级需要 100+200+300=600 exp。
- 按当前击杀/寻路吞吐，gate 的 900s 预算内不可能到 4 级；已停止，未硬凑通过。
- 结论：B 方案在当前 gate 时间预算内不可行；需要产品/验收决定：
  1. 给 gate 更长的 leveling 预算（实测约 >1h，不现实）；
  2. 用测试夹具在 clean 之外提供 4 级起始（但不再是“清洁 1 级”）；
  3. 或把 quest 7 的 MinLevel 作为产品数据问题另行裁决（本轮按 captain 裁决不改数据）。

## canary7（quest 3 设计）
- clean 初态通过；walker 到 Jane；`[诊断] Jane ... file=.../Jane`。
- `C.AcceptQuest{3}` 未收到 `S.ChangeQuest{3}`；失败聊天「接取任务失败。」。
- Server WARN：`接取任务被拒绝 session=1 error=prerequisite quest 2 is not complete`。
- 数据 dump：quest 3 `required_quest=2`，**不是 1 级无前置的纯对话任务**；`min=1 req=0` 的只有 quest 1（carry CannibalLeaves 5）、quest 5（kill Deer/Scarecrow）、quest 154（无脚本文件）等。
- 结论：新设计“Jane 接 quest 3”也不满足前置；需要先完成 quest 1/2 或选其他任务。已停 server/gate，本轮①仍 PARTIAL。
