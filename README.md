# Starter kit: Claude Code setup for the revision site

Unzip this, and the `revision-site-starter` folder becomes your project folder (rename it if you like). Files starting with a dot are hidden by default. On a Mac press Cmd+Shift+. in Finder to see them. In Windows Explorer turn on View > Hidden items.

| File | What it does |
| --- | --- |
| `CLAUDE.md` | Project memory. Claude reads it at the start of every session. Edit the exam board line first. |
| `.claude/settings.json` | Lets Claude run cargo build/test/fmt without asking, asks before `git push`, blocks reading `.env` secrets, and auto-formats Rust after every edit. |
| `.mcp.json` | Two MCP servers: Playwright (a browser Claude can drive to test pages) and Context7 (up-to-date docs for actix-web, Askama, htmx). Claude Code asks you to approve them the first time. |
| `.claude/skills/revision-set/SKILL.md` | Your own `/revision-set` command: turns a spec point plus textbook pages into notes, flashcards and a quiz. |
| `.claude/agents/content-checker.md` | A reviewer subagent that checks generated content for mistakes with fresh eyes. |
| `.gitignore` | Keeps build output, secrets and the `sources/` textbook folder out of git. |

## One-time setup

1. Install Claude Code: `curl -fsSL https://claude.ai/install.sh | bash` (Mac, Linux, WSL) or `irm https://claude.ai/install.ps1 | iex` (Windows PowerShell). On Windows, also install Git for Windows.
2. Install Rust from https://rustup.rs, then run `rustup component add rust-analyzer`.
3. Install Node.js LTS (Playwright's MCP server runs through `npx`).
4. In the project folder run `git init`, then start `claude`.
5. Inside Claude Code, install the plugins:
   ```
   /plugin install rust-analyzer-lsp@claude-plugins-official
   /plugin install frontend-design@claude-plugins-official
   /plugin install security-guidance@claude-plugins-official
   /plugin install claude-md-management@claude-plugins-official
   ```
   Then run `/reload-plugins`.
6. Optional while you learn Rust: `/output-style Explanatory` adds short explanations of each choice Claude makes.

## First prompts

Start in plan mode (press Shift+Tab until it says plan mode) and paste:

> I want to build the revision site described in CLAUDE.md. Interview me in detail using the AskUserQuestion tool about pages, navigation, how quizzes and flashcards should behave, visual style and hosting. Then write a complete spec to SPEC.md with a phased build plan, where phase 1 is the smallest version students could use.

Then run `/clear`, and:

> Build phase 1 from SPEC.md. Use the frontend-design skill for the look. Create two sample topics in content/ so there is something to see. When done, run the app and check the pages with Playwright at phone and desktop widths.

When textbooks are ready, drop the PDFs into `sources/` and run for example:

> /revision-set 1.1.1 structure and function of the processor, sources/textbook.pdf pages 2-12
