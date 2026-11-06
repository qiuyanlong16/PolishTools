# Polish Tools · 英语润色助手

**在任意输入框里直接改对你的英文——不用复制粘贴，不用开浏览器标签页。**

在光标所在的输入框里**连续轻按三次空格**，工具会抓取你正在写的文本，调用大模型（默认 DeepSeek）润色，再原地写回。每一次改动都会存进本地 SQLite 数据库，方便你看清自己反复犯的错。

```
你写：   This are a test sentence and i has a apple.
按键：   ␣␣␣
得到：   This is a test sentence, and I have an apple.
```

[English README](README.md)

## 功能

- **随处可用**：全局低级键盘钩子，Chrome、Word、微信、Slack、VS Code、Electron 应用都生效
- **三连空格触发**：不占用系统全局热键；中间敲了其它键就重置计数，正常打字不会误触
- **原地回写**：走 `Ctrl+C` / `Ctrl+A` / `Ctrl+V`，和你自己改一模一样
- **智能粒度**：有选区只润色选区，没选区润色整段
- **本地记忆库**：原文、润色结果、错误类型全部落库（`data/polish.db`），学习页给出错误分布和高频错误对照
- **模型可换**：兼容 OpenAI 协议，默认 DeepSeek，也可换 OpenAI、Moonshot、自建网关
- **绿色便携**：单个约 5 MB 的 exe，配置和数据都在程序目录，删文件夹即卸载；不装 WebView2，不写注册表（除非开启自启）

## 三步上手

1. 从 [Releases](../../releases) 下载 `PolishTools-vX.Y.Z-windows-x64.zip`
2. 解压到任意目录（例如 `D:\Tools\PolishTools`），运行 `PolishTools.exe`
3. 到 [platform.deepseek.com](https://platform.deepseek.com/api_keys) 创建 Key，填进「设置 → API Key」，点「测试连接」
4. 在任意输入框里输入英文，连按三下空格

程序以窗口形式运行，可以最小化到任务栏常驻。

## 工作原理

```
全局键盘钩子（WH_KEYBOARD_LL）
        │  连续 3 次空格，中间无其它按键
        ▼
  备份剪贴板 → Ctrl+C → Ctrl+A → Ctrl+C
        │
        ▼
 POST /v1/chat/completions（JSON 模式，返回结构化错误项）
        │
        ▼
  Ctrl+A → Ctrl+V 写回焦点控件
        │
        ▼
 SQLite：corrections + issue_stats
```

模型被要求返回严格 JSON：

```json
{
  "polished": "…",
  "issues": [
    { "type": "grammar", "original": "This are", "corrected": "This is", "explain": "主谓不一致" }
  ],
  "summary": "一句话总结"
}
```

`type` 会归一化为 `grammar | spelling | article | preposition | tense | word-choice | punctuation | style | other` 并长期累计。

## 配置说明

配置保存在 exe 同级的 `config.json`，界面里可改，也可以手动编辑：

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `api_base` | `https://api.deepseek.com` | 兼容 OpenAI 的地址，会自动补 `/v1/chat/completions` |
| `api_key` | 空 | 模型 Key，明文存储，注意不要外传 |
| `model` | `deepseek-chat` | 模型名（`deepseek-reasoner`、`gpt-4o-mini` 等） |
| `temperature` | `0.2` | 越低越保守 |
| `max_chars` | `6000` | 超过则跳过，防止误改整篇文档 |
| `restore_clipboard` | `true` | 润色后恢复原剪贴板 |
| `hotkey.repeat` | `3` | 连续按键次数（2–5） |
| `hotkey.interval_ms` | `400` | 相邻两次按键最大间隔 |
| `hotkey.swallow` | `true` | 吞掉触发键，避免多出空格 |
| `autostart` | `false` | 写 `HKCU\...\Run\PolishTools` 开机自启 |

## 从源码构建

需要：Windows 10/11、Rust stable（MSVC 工具链）、装有 C++ 工作负载的 Visual Studio Build Tools。

```powershell
cargo build --release
# 或直接产出便携 zip：
powershell -ExecutionPolicy Bypass -File build.ps1
```

产物：`dist\PolishTools\PolishTools.exe` 与 `dist\PolishTools-portable.zip`。

推送 `v*` 标签后，GitHub Actions 会自动构建并发布 Release（见 `.github/workflows/release.yml`）。

## 常见问题

| 现象 | 原因 / 处理 |
| --- | --- |
| 连按三次空格没反应 | 已暂停；或焦点在本工具自己的窗口（设计如此）；或目标程序以管理员运行——本工具也需以管理员启动 |
| 提示抓不到文本 | 当前焦点不是输入框，或该程序不支持 `Ctrl+A`/`Ctrl+C`；可先手动选中文本再触发 |
| 杀软报警 | 全局键盘钩子的正常现象，加入白名单即可 |
| HTTP 401 | Key 错误或已过期 |
| HTTP 429 / 超时 | 触发限流或网络问题 |
| 长文本被跳过 | 在设置里调大 `max_chars` |

日志：`data/app.log`。

## 隐私

- 文本只会发送到你自己配置的接口地址
- 所有历史记录都留在本机 `data/polish.db`，不上传任何地方
- 没有埋点、没有统计、没有自动更新回传

## 许可证

[MIT](LICENSE) © 2026 qiuyanlong16