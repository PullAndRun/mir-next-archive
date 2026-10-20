# 内部集合清单与慢泄漏观测（第 24 轮 t-1c707906）

> 背景：4h soak 的 RSS 判据对「每天 +4.6MB」这类**慢增长**不敏感（4h 只涨 ~800KB，淹没在噪声里）。
> 第 23 轮审查发现 `DB_SAVE_RECORDS` 曾是无界 `Vec`，因此本轮把关键集合的**长度**做成 gauge 单独观测，
> 并在这里登记每一项的「有界/无界 + 上限 + 代码位置 + 证据」。

## 1) 已暴露为 gauge 的 5 项（`kind` 是**编译期固定集合**）

指标：`mir2_internal_records{kind="..."}`（`crates/mir-server/src/metrics.rs`，`INTERNAL_COLLECTION_KINDS`）。

| `kind` | 集合 | 有界？ | 上限 | 代码位置 | 证据 |
|---|---|---|---|---|---|
| `db_save` | `DB_SAVE_RECORDS: Mutex<VecDeque<(u64,i64,i64)>>` | ✅ **有界**（环形） | **1024** 条（≈16KB） | `pg_store.rs:17`（`DB_SAVE_RECORD_CAP`）、`:20`（static）、`:24-30`（push 后 `while len > cap { pop_front }`） | 单测 `db_save_records_are_bounded_and_drop_oldest`（`pg_store.rs:1194`）：塞 `CAP + 76` 条后 `len == 1024`；本轮新增 `internal_records_gauge_tracks_growth_and_stops_at_cap`（`tests/metrics.rs`）从 gauge 侧再验一次 |
| `mail_writes` | `World::mail_writes: Vec<MailWrite>` | ✅ 有界（**每包 drain**） | 正常 0；仅在写库失败时短暂保留（下一轮重试） | `world.rs:955`（字段）、`mail.rs:448`（`take_mail_writes`）、`market.rs:2690`（失败保留） | 访问器 `World::pending_mail_writes()`（`market.rs:982`）直接读 `len()`；soak 观测它应长期为 0 |
| `pending_hits` | `World::pending_hits: Vec<PendingHit>` | ✅ 有界（**按时间结算**） | 与「300ms 内命中数」同阶（≈ 攻击者数 × 每次命中），不会跨 tick 累积 | `world.rs:778`（字段）、`world.rs:2704`（`resolve_pending_hits` 到点结算并移除） | 访问器 `World::pending_hits()`（`world.rs:2109`）；soak 观测它应随负载波动、不单调上涨 |
| `pending_map_moves` | `World::pending_map_moves: Vec<PendingMapMove>` | ✅ 有界（**每 tick 清空**） | 每 tick 的跨图请求数（`std::mem::take` 后处理） | `world.rs:899`（字段）、`maps.rs:755-760`（`resolve_pending_map_moves` 里 `mem::take`） | 访问器 `World::pending_map_moves()`（`maps.rs:393`）；处理后必为 0 |
| `market_searches` | `World::market_searches: HashMap<u32, MarketSearchState>` | ✅ 有界（**每个在线玩家至多 1 条**，下线即删） | `≤ players_online` | `world.rs:982`（字段）、`world.rs:1872`（离开世界时 `remove`） | 访问器 `World::market_search_count()`（本轮新增，`market.rs`）；soak 观测它应 ≈ 在线人数 |

## 2) 其它候选（本轮**未**接入 gauge，登记原因）

| 集合 | 位置 | 有界？ | 为什么没接 |
|---|---|---|---|
| `World::monster_slaves: HashMap<u32, Vec<u32>>` | `world.rs:990` 附近 | ✅ 随召唤者死亡清理 | 需要 `world.rs` 访问器（归兵部），本轮未跨线改 |
| `World::monster_summon_time`、`trap_rock_expire`、`trap_rock_shown` | `world.rs` | ✅ 随怪物死亡/TTL 清理 | 同上（需要 `world.rs` 访问器） |
| `World::broadcast_batch: Option<HashMap<u32, Vec<u8>>>` | `world.rs` | ✅ 每 tick `flush` 后 `take` | 同上；且它是 tick 内瞬态，采样点到 0 的概率高 |
| `Player::friends` / `guild.allies` / `mail` | 玩家/行会对象内 | ✅ 有硬上限（好友 100、盟友 5、邮件上限） | 属于玩家数据而非服务端内部集合，另有既有校验与测试 |

> 结论：**本轮接入的 5 项全部有界**；真正可能「无界增长」的是**未加访问器的 world 侧集合**，
> 它们要么有 TTL，要么随对象销毁 —— 但**没有 gauge 就没有证据**，下轮若兵部愿意加只读访问器，
> 可以按同一套 `kind` 机制继续扩（`INTERNAL_COLLECTION_KINDS` 是编译期数组，扩项要同步这里与测试）。

## 3) 判据：gauge 怎么用（**不改既有阈值**）

| 观测 | 含义 | 动作 |
|---|---|---|
| gauge **斜率 ≈ 0** 且 RSS 平 | 正常 | — |
| gauge **线性上涨** 且 RSS 平 | **有界缓冲在涨但被回收**（例如环形未满 → 涨到 cap 为止） | 看是否收敛到上限；收敛即正常 |
| gauge **线性上涨** 且 RSS **同步涨** | **真泄漏**（缓存没上限，或清理路径漏了） | 提 P0：定位该集合的清理路径 |
| gauge 停在**上限**（如 `db_save == 1024`） | 环形工作正常 | — |

判定用**斜率**（对 `t` 做最小二乘，单位「条/小时」）而不是单点差值——与 RSS 斜率同口径：

```
gauge 斜率 > 0 且 RSS 斜率 > 2MB/h  → 判为真泄漏（P0）
gauge 斜率 > 0 但 RSS 斜率 ≈ 0      → 有界缓冲，记录上限即可
```

**本轮不改任何既有阈值**：RSS ≤10%、p50 ≤30ms、p99 ≤80ms、时长 ≥14400s、chunk 判据全部保持原样
（`git diff` 里 `CRITERIA` 的这些项一行未动）。

## 4) 复算命令（PowerShell 7，无 heredoc）

```powershell
# 从 samples.json 看 5 条 gauge 的斜率（条/小时）与 RSS 斜率对照
py -c "import json,statistics;d=json.load(open(r'docs/testing/soak-data/<ts>/samples.json',encoding='utf-8'));s=d['samples'];keys=['internal_db_save','internal_mail_writes','internal_pending_hits','internal_pending_map_moves','internal_market_searches'];
def slope(k):
 v=[(x['t']/3600.0,x[k]) for x in s if isinstance(x.get(k),(int,float))];
 if len(v)<2: return None
 mx=statistics.fmean([a for a,_ in v]); my=statistics.fmean([b for _,b in v]); den=sum((a-mx)**2 for a,_ in v)
 return round(sum((a-mx)*(b-my) for a,b in v)/den,4) if den else None
print({k: slope(k) for k in keys}); print('rss_slope_MB_per_h', slope('rss_bytes') and round(slope('rss_bytes')/1e6,3))"
```

> 注意：第 24 轮之前的 soak 数据没有这 5 个字段（采样是后来加的），复算时若全为 `None` 属正常。
