# 参考实现仓库清单

> 本文件记录 `github/` 下用于**参考**的《传奇2》开源实现。
> `github/` 已在 `.gitignore` 中（体积大、非本项目产物），这里只登记**来源与用途**，
> 便于任何人重建同一份参考环境。
>
> **主参考是 `Suprcode/Crystal`（C#）**：协议编号表、技能公式、AI 行为、任务/掉落/地图
> 解析等，一切行为有歧义时以 Crystal 源码为准。其余仓库仅在 Crystal 缺失或可疑时用于交叉验证。

## 清单

| 目录 | 仓库 | HEAD（clone 时） | 体积 | 定位 |
|---|---|---|---:|---|
| `github/Crystal` | <https://github.com/Suprcode/Crystal> | `0e315fe3` | 49 MB | **主参考**：C# 服务端/客户端/共享库，协议与玩法行为的权威来源 |
| `github/MIR2` | <https://github.com/lzxsz/MIR2>（`GameOfMir` 目录） | `98711da` | 49 MB | 传统 Mir2 服务端/客户端实现，用于对照地图/NPC/任务脚本格式 |
| `github/Mir2-GeeM2` | <https://github.com/cjlaaa/Mir2-GeeM2> | `9981a8d` | 266 MB | GeeM2 引擎实现（体量最大），用于对照技能/怪物/爆率等扩展行为 |
| `github/OpenMir2` | <https://github.com/mirbeta/OpenMir2> | `1847483` | 70 MB | 另一支开源重实现，用于对照网络层与数据表组织方式 |
| `github/mir2x` | <https://github.com/etorth/mir2x> | `4ceb865` | 59 MB | 备用参考（数据/脚本结构） |
| `github/cjlaaa-mir2` | <https://github.com/cjlaaa/mir2> | `7ec7fe3` | 13 MB | 备用参考（轻量实现） |
| `github/mirgo` | <https://github.com/pyq0109/mirgo> | `8443cfb` | 2.8 MB | 备用参考（Go 实现，适合看协议/服务端结构的最小化表达） |

## 重建命令

```powershell
# 代理按 .env 的 CLASH 设置
$env:HTTPS_PROXY = "http://127.0.0.1:7890"; $env:HTTP_PROXY = $env:HTTPS_PROXY
git clone --depth 1 https://github.com/Suprcode/Crystal.git        github/Crystal
git clone --depth 1 https://github.com/lzxsz/MIR2.git              github/MIR2
git clone --depth 1 https://github.com/cjlaaa/Mir2-GeeM2.git       github/Mir2-GeeM2
git clone --depth 1 https://github.com/mirbeta/OpenMir2.git        github/OpenMir2
git clone --depth 1 https://github.com/etorth/mir2x.git            github/mir2x
git clone --depth 1 https://github.com/cjlaaa/mir2.git             github/cjlaaa-mir2
git clone --depth 1 https://github.com/pyq0109/mirgo.git           github/mirgo
```

> `OpenMir2` 在 Windows 上可能因超长路径导致 checkout 失败（`exit 128`），
> 需要加 `-c core.longpaths=true` 并去掉 `--filter=blob:none`。

## 使用约定

1. **行为有歧义 → 只信 Crystal**，其余仓库用于"是否还有别的实现方式"的旁证；
2. 每个玩法实现都要在代码/文档里写清 **Crystal 文件:行号**（AI 表、技能公式、包字段等已经这样做）；
3. 不要把参考仓库的任何代码直接拷贝进 `crates/`——**重写，不是搬运**；需要移植算法时，注释里给出来源行号与差异说明。
