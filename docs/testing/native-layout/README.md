# R48 A1 原生布局证据入口

本轮按队长 t24 补充要求，把证据写在独立目录
[`docs/testing/native-player-layout/`](../native-player-layout/README.md)，
避免覆盖 R47 的 `docs/testing/attack-variants/`。

这里保留入口以满足任务合同中的路径；实际 manifest、200 张 PNG、逐帧断言和
限制说明全部存放在上述目录。该目录共 202 个文件（README + `manifest.json` +
200 PNG），manifest batch `1792823484739`、`unique_png_sha256=144`，200/200
PNG 哈希已复核；本次是默认 `game-assets` 渲染路径证据，**没有窗口 live 日志**，
不声称真实 TCP 战斗已跑。为避免重复 24 MB 证据，本入口不复制文件。
