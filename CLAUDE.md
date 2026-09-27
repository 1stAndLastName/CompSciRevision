# A Level CS Revision Site

A revision website for A level Computer Science students: topic notes, flashcards and quizzes.
Exam board: OCR A Level Computer Science (H446). Every piece of content maps to an OCR spec point and uses OCR's terminology.

The maintainer is a CS teacher who is new to Rust and to AI agents. Explain Rust-specific choices in a sentence or two and prefer simple, readable code over clever code.

## Stack
- Backend: Rust + actix-web, pages rendered on the server with Askama templates
- Interactivity: htmx plus small amounts of vanilla JS. No SPA framework and no Node build step
- Styling: one hand-written CSS file (`static/css/site.css`), mobile first. Every colour is a CSS custom property, with one block per theme: Dark (default), OLED black and Light. Calm emerald/mint accent, not neon. Theme is picked in the header, saved in localStorage and applied by an inline script in `<head>` before paint. Fonts are self-hosted in `static/fonts`: Inter (variable, with `cv05`/`cv08` so I, l and 1 differ) for text, and JetBrains Mono for code, pseudocode, binary, hex and Boolean expressions. Tabular numbers for scores and progress. htmx is served from `static/vendor`. No CDNs or Google Fonts, because school networks block them. Quiz feedback never relies on colour alone: it shows a tick or cross plus "Correct" or "Not quite". Animations respect `prefers-reduced-motion`.
- Content: files in `content/` (Markdown + TOML), loaded at startup
- Student progress: kept in the browser (localStorage) for v1. No student accounts and no personal data stored on the server

## Commands
- Run: `cargo run`, then open http://localhost:8080
- Test: `cargo test`
- Lint: `cargo clippy -- -D warnings`
- Format: `cargo fmt`

## Content format
- `content/spec.toml`: OCR components and sections (e.g. `1.4`), used to group topics on the home page. A topic's section must be listed here.
- `content/<topic-slug>/notes.md`: revision notes, with YAML front matter holding `title`, `spec` (e.g. `"1.4.3"`) and optionally `sample: true` (shows a "Sample" badge). Start headings at `##`.
- `content/<topic-slug>/flashcards.toml`: `[[card]]` entries with `front`, `back`, `spec`
- `content/<topic-slug>/quiz.toml`: `[[question]]` entries with `prompt`, `options` (array), `answer` (0-based index), `explanation`, `spec`, `difficulty` (1-3), and optionally `shuffle = false`
- Item `spec` values may name a sub-point, e.g. `"1.4.3(b)"`.
- Quiz options and question order are shuffled on every attempt. Explanations must describe the answer itself and never refer to an option by letter or position ("option B", "the first one"); the loader rejects "option B"-style phrases. Use `shuffle = false` only where order matters (numbers in sequence, "All of the above").
- All text is Markdown. Put Boolean expressions, code, binary and hex in backticks so they use the mono font (Inter has no `∧ ∨ ⊻` glyphs). Use OCR notation: `∧ ∨ ¬ ⊻ ≡`. `<sup>` and `<sub>` are the only HTML allowed; any other HTML is shown as text.
- Create content with the `/revision-set` skill. If a content file fails to load, fix the file, not the loader.

## Source material
- `sources/` holds textbooks and specification PDFs. It is gitignored and must never be served by the site.
- Content in `content/` is written in original wording. Never copy textbook passages, figures or past-paper questions into it.

## Rules
- Pages must work well on a phone (390px wide) and a desktop.
- Accessibility: semantic HTML, labelled form controls, visible focus, WCAG AA contrast.
- After a UI change, open the page with the Playwright MCP server at phone and desktop widths and check it visually.
- A task is only done when `cargo fmt`, `cargo clippy -- -D warnings` and `cargo test` all pass.
