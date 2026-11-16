# Flux Code

A local-first AI coding agent for Windows. Native desktop app built with **Tauri 2** (Rust) + **React** + **TypeScript**. Chat with an agent that can plan tasks, edit files, run commands, start dev servers and ask you questions — everything stays on your machine.

![stack](https://img.shields.io/badge/Tauri-2-blue) ![platform](https://img.shields.io/badge/platform-Windows-lightgrey) ![license](https://img.shields.io/badge/license-MIT-green)

## Highlights

- **Real agent tools** — list/read/create/edit/delete/rename files, create folders, search files, run commands (with timeouts), start/stop long-running processes (dev servers), and a to-do list the agent maintains as it plans work.
- **Providers** — Groq, Google Gemini, Anthropic, OpenAI, OpenRouter, NVIDIA, plus a **Custom** provider for any OpenAI-/Anthropic-/Gemini-compatible endpoint with custom headers.
- **Streaming chat** — markdown, GFM tables, code highlighting (incl. PowerShell), KaTeX math, collapsible reasoning blocks.
- **Reasoning effort** — a dynamic selector appears only for models that support effort control (e.g. OpenAI o-series, Groq qwen reasoning models); auto-fallback if a provider rejects the parameter.
- **Permissions** — *Ask for Permissions* mode gates every mutating tool behind an approval card (with *Allow for session*), *Full Access* lets the agent work freely. Truly risky commands always ask, even in Full Access.
- **Questions** — the agent can ask structured questions (choice / multi / yes-no / text) rendered as interactive cards.
- **Workspaces** — a default workspace is created at `Documents\Flux Projects` on first run; change it or add extra folders to the access scope at any time.
- **Right panel** — Preview (live dev-server iframe), Browser (embedded web view), To-do, and Files (workspace tree with file viewer).
- **Persistence** — chats, messages, to-dos and settings in local SQLite; API keys in **Windows Credential Manager** (never in plaintext).
- **i18n** — English (US), Português (Brasil), Español.
- **Themes** — dark / light / system, five accent colors, reduced-motion option.

## Build from source

Prerequisites: **Rust** (MSVC toolchain), **Node.js 22.22+** (or **24.15+**, or 26+) with **npm 10+**, Windows 10/11.

The minimum Node.js version is set by the dev/test toolchain (Vitest 5 requires Node ≥ 22.12, jsdom 31 requires Node ≥ 22.22.2); `package.json` encodes this in its `engines` field, so `npm install` warns on older Node versions.

```bat
npm install
npm run tauri build
```

Artifacts:

| File | Location |
| --- | --- |
| Standalone executable | `src-tauri\target\release\Flux.exe` |
| Installers (NSIS + MSI) | `src-tauri\target\release\bundle\{nsis,msi}\` |

For development with hot reload: `npm run tauri dev`.

Type-check without building: `npm run check`.

## First run

1. Open **Settings → Providers**, paste an API key for your provider (stored in Credential Manager).
2. Click **Refresh models** and tick the models you want in the picker.
3. Pick a model in the composer's model menu and start building.

## Data locations

| Data | Path |
| --- | --- |
| SQLite database | `%APPDATA%\com.fluxcode.desktop\flux.sqlite` |
| Default workspace | `%USERPROFILE%\Documents\Flux Projects` |

## Implementation notes & limitations

- **Image attachments** are supported for OpenAI-compatible, Anthropic and Gemini providers; other providers receive the text portion only.
- **Preview panel** requires the dev server to bind `localhost`; sites that send `X-Frame-Options: DENY` or a restrictive `frame-ancestors` CSP cannot be embedded (Browser/Preview show a blank frame) — use *Open in browser* in that case.
- **Terminal output** is rendered inline in chat cards (with a live tail), not in a separate terminal panel.
- Drag-and-dropped paths are attached as files; directory drops are treated as file paths.
- `list_files` results are capped (~2000 entries / depth 6) for very large trees.
- Model lists come from each provider's API; for the Custom provider you add model IDs manually after testing the connection.

## Project layout

```
src/                    React frontend (components, store, i18n, design tokens)
src-tauri/              Rust backend
  src/agent.rs          Agent loop, permission/question gates, streaming
  src/llm/              Provider adapters (OpenAI-compat, Anthropic, Gemini)
  src/tools.rs          Tool implementations
  src/terminal.rs       Process spawning, output streaming, server detection
  src/db.rs             SQLite persistence
  src/creds.rs          Windows Credential Manager
  src/permissions.rs    Tool gating rules
```

## License

MIT
