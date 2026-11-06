# Polish Tools

**Fix your English right inside any text box — no copy-paste dance, no browser tab.**

Press the **spacebar three times** in any input field. Polish Tools grabs the text you are writing, sends it to an LLM (DeepSeek by default), and writes the polished version back in place. Every correction is stored in a local SQLite database so you can see which mistakes you keep repeating.

```
You type:        This are a test sentence and i has a apple.
Press:           ␣␣␣
You get:         This is a test sentence, and I have an apple.
```

[中文说明](README.zh-CN.md)

---

## Features

- **Works everywhere** — global low-level keyboard hook, so it triggers in Chrome, Word, WeChat, Slack, VS Code, Electron apps, anywhere you can type.
- **Triple-space trigger** — no conflict with system hotkeys. Detection resets if you press any other key in between, so normal typing never fires it.
- **Writes back in place** — uses `Ctrl+C` / `Ctrl+A` / `Ctrl+V`, so it behaves like you edited the text yourself.
- **Selection-aware** — if you have text selected, only that selection is polished; otherwise the whole field is.
- **Local learning database** — every original/polished pair and every issue type is stored in `data/polish.db`. The Insights tab shows your error distribution and a "frequent mistakes" list.
- **Any OpenAI-compatible model** — DeepSeek by default, works with OpenAI, Moonshot, Qwen, or your own gateway.
- **Green / portable** — a single 5 MB exe. Config and data live next to the executable. Delete the folder to uninstall. No installer, no WebView2, no registry writes unless you enable autostart.
- **Lives in the system tray** — closing the window hides it instead of quitting, and the hotkey keeps working. Double-click the tray icon to bring it back; right-click for show / pause / quit.

## Quick start

1. Download `PolishTools-vX.Y.Z-windows-x64.zip` from [Releases](../../releases).
2. Extract it anywhere, e.g. `D:\Tools\PolishTools`.
3. Run `PolishTools.exe`.
4. Get an API key from [platform.deepseek.com](https://platform.deepseek.com/api_keys), paste it into **Settings → API Key**, then click **Test connection**.
5. Type English anywhere and press <kbd>Space</kbd> three times.

> The app runs windowed. Closing it hides the window to the system tray — the triple-space trigger stays active.
> Double-click the tray icon to reopen it, or right-click it for **Show / Pause / Quit**.

## How it works

```
 global keyboard hook (WH_KEYBOARD_LL)
              │  3 × space, no other key in between
              ▼
     save clipboard → Ctrl+C → Ctrl+A → Ctrl+C
              │
              ▼
   POST /v1/chat/completions  (JSON mode, structured issues)
              │
              ▼
   Ctrl+A → Ctrl+V back into the focused control
              │
              ▼
   SQLite: corrections + issue_stats
```

The model is asked to return strict JSON:

```json
{
  "polished": "…",
  "issues": [
    { "type": "grammar", "original": "This are", "corrected": "This is", "explain": "subject-verb agreement" }
  ],
  "summary": "one-line summary"
}
```

`type` is normalised into `grammar | spelling | article | preposition | tense | word-choice | punctuation | style | other` and aggregated over time.

## Configuration

Settings are stored in `config.json` next to the executable (editable from the UI or by hand):

| Key | Default | Description |
| --- | --- | --- |
| `api_base` | `https://api.deepseek.com` | OpenAI-compatible base URL. `/v1/chat/completions` is appended automatically. |
| `api_key` | empty | Your model provider key. Stored in plain text — keep this file private. |
| `model` | `deepseek-chat` | Model name (`deepseek-reasoner`, `gpt-4o-mini`, …). |
| `temperature` | `0.2` | Lower is more conservative. |
| `max_chars` | `6000` | Texts longer than this are skipped to avoid accidental whole-document rewrites. |
| `restore_clipboard` | `true` | Restore your previous clipboard content after polishing. |
| `hotkey.repeat` | `3` | Number of consecutive presses (2–5). |
| `hotkey.interval_ms` | `400` | Maximum gap between presses. |
| `hotkey.swallow` | `true` | Swallow the triggering keypress so no extra space is inserted. |
| `autostart` | `false` | Write `HKCU\...\Run\PolishTools` for launch at login. |

## Build from source

Requirements: Windows 10/11, Rust stable (MSVC toolchain), Visual Studio Build Tools with the C++ workload.

```powershell
cargo build --release
# or produce the portable zip:
powershell -ExecutionPolicy Bypass -File build.ps1
```

Output: `dist\PolishTools\PolishTools.exe` and `dist\PolishTools-portable.zip`.

Release builds are produced automatically by GitHub Actions on every `v*` tag (see `.github/workflows/release.yml`).

## Troubleshooting

| Symptom | Cause / fix |
| --- | --- |
| Nothing happens when pressing space three times | The app is paused, or focus is inside Polish Tools itself (by design), or the target app runs elevated — run Polish Tools as administrator too. |
| "Could not capture any editable text" | The focused element is not a text field, or the app does not support `Ctrl+A`/`Ctrl+C`. Try selecting the text manually first. |
| Antivirus warns about a keyboard hook | Expected for any global-hotkey utility. Whitelist the exe. |
| HTTP 401 | Wrong or expired API key. |
| HTTP 429 / timeout | Provider rate limit or network issue. |
| Text longer than the limit is skipped | Raise `max_chars` in settings. |

Logs: `data/app.log`.

## Privacy

- Your text is sent **only** to the API endpoint you configure.
- All history stays in `data/polish.db` on your machine. Nothing is uploaded anywhere else.
- No telemetry, no analytics, no auto-update phone-home.

## License

[MIT](LICENSE) © 2026 qiuyanlong16