# Agent logos — sources and terms

QuantPilot shows these marks only to identify which agent a thread, picker
entry or setting belongs to (round 2, card 01). Every mark is a trademark or
logo of its owner; no endorsement is implied. The files are bundled with the
app — nothing is fetched from the network.

All files below were taken from **Orca** (`stablyai/orca`, commit
`1d2e7f6f52c963d02c5d4e1c776334b3179374dd`, 2026-11-02), MIT License,
Copyright (c) 2026 Lovecast Inc. The SVGs were converted unchanged in
geometry and color from Orca's React components to plain SVG files; the PNGs
are Orca's bundled favicons, copied as they are. Orca's own notes on each
upstream source are repeated here.

| File | Agent | Orca source | Upstream origin (per Orca) |
| --- | --- | --- | --- |
| `claude.svg` | Claude Code | `src/renderer/src/components/status-bar/icons.tsx` `ClaudeIcon` | Anthropic Claude mark |
| `openai.svg` | Codex | same file, `OpenAIIcon` | OpenAI mark |
| `gemini.svg` | Gemini CLI | same file, `GeminiIcon` | Google Gemini mark |
| `droid.svg` | Droid (Factory) | same file, `DroidIcon` | Factory mark on a black tile |
| `pi.svg` | pi | `src/renderer/src/lib/agent-icon-glyphs.tsx` `PiIcon` | pi.dev/favicon.svg |
| `omp.svg` | omp | same file, `OmpIcon` | omp.sh homepage mark |
| `kilo.svg` | Kilo Code | same file, `KiloIcon` | Kilo-Org/kilocode `kilo-light.svg` |
| `copilot.svg` | GitHub Copilot CLI | same file, `CopilotIcon` | Primer Octicons `copilot-16` (MIT, GitHub) |
| `opencode.svg` | OpenCode | same file, `OpenCodeIcon` | opencode.ai/favicon.svg |
| `cursor.png` | Cursor | `src/shared/agent-icons/cursor.png` | the project's favicon |
| `grok.png` | Grok | `src/shared/agent-icons/grok.png` | the project's favicon |
| `hermes.png` | Hermes | `src/shared/agent-icons/hermes.png` | the project's favicon |
| `freebuff.png` | Freebuff | `src/shared/agent-icons/freebuff.png` | the project's favicon |
| `openclaw.png` | OpenClaw | `src/shared/agent-icons/openclaw.png` | the project's favicon |
| `kiro.png` | Kiro | `src/shared/agent-icons/kiro.png` | the project's favicon |
| `cline.png` | Cline | `src/shared/agent-icons/cline.png` | the project's favicon |
| `antigravity.png` | Antigravity | `src/shared/agent-icons/antigravity.png` | the project's favicon |
| `devin.png` | Devin | `src/shared/agent-icons/devin.png` | the project's favicon |

`mono` marks (`openai`, `opencode`, `copilot`, `pi`) are drawn as a mask in
the current text color so they read in both themes; the others keep their
colors and, where they have one, their own tile.

Adding a mark: take it from the agent's own published assets or press kit,
keep it local, add a row here with the source and terms, and register it in
`utils/agentIcons.ts`. Do not ship a file whose rights are unclear — the
agent then shows its letter.

## MIT License (Orca)

Copyright (c) 2026 Lovecast Inc.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
