# 协议级 playtest（`tools/playtest/playtest.py`，第 8 轮新增）

用途：不启动图形客户端，用 Python 直接连真实服务端，按 Crystal 协议把「登录 → 建角 →
进世界 → 走一步 → 看包」跑一遍，用于冒烟与排障。

## 1. 用法

```powershell
python tools/playtest/playtest.py
python tools/playtest/playtest.py --server 127.0.0.1:7000 --timeout 8
```

* `--server host:port` 直连已有服务端（不自己起服务）；不传时按脚本默认本地端口；
* `--timeout` 单步等待秒数（默认 8）；
* 输出 `[OK]` / `[FAIL]` / `[SKIP]`；失败时打印期望包号与实际收到的包；
* 退出码：`0` = 全 OK/合理 SKIP，`1` = 有 FAIL。

## 2. 覆盖

| 步骤 | 期望 |
|---|---|
| 连接 + `C.ClientVersion` | `S.Connected` |
| 注册/登录 | `S.LoginSuccess` |
| 建角 + `C.StartGame` | `S.UserInformation` → 进世界 |
| 走一步 `C.Walk` | `S.ObjectWalk`/位置更新 |
| NPC/打怪/技能/商店 | 依赖角色状态，见下 |

## 3. 当前状态（t76 回执）

- **新角色满血已修复**：`World::enter` 对 `hp/mp <= 0` 兜底满血满蓝（`docs/world/combat.md` §10.4），
  `C.Walk` 不再被 `MoveError::Dead` 拒绝；`[OK] 走到 NPC / 对话 —— Premium_Elijah`；
- **TownArcher 红名过滤**：AI 57 只锁 `PKPoints >= 200`（Crystal `TownArcher.cs:96-112`），
  新角色不会再被出生点旁的 Royal_Archer 秒杀；
- `[SKIP] 买药（NPC 商店页需客户端 UI）`；
- `[FAIL] 接近怪物 —— 未走到 Hen`：服务端返回 `target cell … is a wall/occupied`，
  属**脚本自身 BFS 走墙**（玩家 hp=15 一直活着），不是服务端 bug；
- NPC 商店购买（`NPCGoods` 解析）与使用技能（新建角色未学技能）尚不能自动验证；
- 真实数据里没有 `ObjectMagic` AI，魔法链路靠合成用例（`tests/skills.rs`）或预留的
  AI 215/216/220。

## 4. 与其它测试的关系

playtest 是「真服务端 + 真 socket + Python 客户端」的冒烟，不替代 Rust 集成测试；
它补的是「换个语言实现的客户端也能按协议玩」这一层。
