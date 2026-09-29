# A Level CS Revision Site

A revision website for A level Computer Science students: topic notes, flashcards and quizzes.
Exam board: OCR A Level Computer Science (H446). Every piece of content maps to an OCR spec point and uses OCR's terminology.

The maintainer is a CS teacher who is new to Rust and to AI agents. Explain Rust-specific choices in a sentence or two and prefer simple, readable code over clever code.

## Stack
- Backend: Rust + actix-web, pages rendered with Askama templates. The live site is a static export of the same pages, published to GitHub Pages (https://1standlastname.github.io/CompSciRevision/) by `.github/workflows/pages.yml` on every push to main. Every page must therefore work as a plain file: no server-side logic beyond rendering, and every link in a template starts with `{{ base }}` ("" locally, "/CompSciRevision" on Pages); files in `static/` refer to each other with relative paths.
- Interactivity: small amounts of vanilla JS (`static/js/`): theme and progress (`site.js`), flashcards (`flashcards.js`) and the quiz (`quiz.js`, which marks answers in the browser from the questions on the page). No SPA framework and no Node build step
- Styling: one hand-written CSS file (`static/css/site.css`), mobile first. Every colour is a CSS custom property, with one block per theme: Dark (default), OLED black and Light. Calm emerald/mint accent, not neon. Theme is picked in the header, saved in localStorage and applied by an inline script in `<head>` before paint. Fonts are self-hosted in `static/fonts`: Inter (variable, with `cv05`/`cv08` so I, l and 1 differ) for text, and JetBrains Mono for code, pseudocode, binary, hex and Boolean expressions. Tabular numbers for scores and progress. No CDNs or Google Fonts, because school networks block them. Quiz feedback never relies on colour alone: it shows a tick or cross plus "Correct" or "Not quite". Animations respect `prefers-reduced-motion`.
- Content: files in `content/` (Markdown + TOML), loaded at startup
- Student progress: kept in the browser (localStorage) for v1. No student accounts and no personal data stored on the server

## Commands
- Export the static site: `cargo run --bin export -- --base /CompSciRevision --out dist` (the Pages workflow does this; `dist/` is gitignored)
- Run: `cargo run`, then open http://localhost:8080. Debug builds listen on 0.0.0.0 so a phone on the same Wi-Fi can open the address printed at startup; release builds listen only on 127.0.0.1
- Test: `cargo test`
- Lint: `cargo clippy -- -D warnings`
- Format: `cargo fmt`

## Content format
- `content/spec.toml`: every spec point from section 2c of the OCR specification (components 01 and 02): component, section, number (e.g. `1.4.3`), title, and each lettered sub-point with its text. The home page lists every point and shows which have a topic yet. Every `spec` value in content must be a point or lettered sub-point listed here; the loader rejects anything else.
- `content/<topic-slug>/notes.md`: revision notes, with YAML front matter holding `title`, `spec` (e.g. `"1.4.3"`) and optionally `sample: true` (shows a "Sample" badge). Start headings at `##`.
- `content/<topic-slug>/flashcards.toml`: `[[card]]` entries with `front`, `back`, `spec`
- `content/<topic-slug>/quiz.toml`: `[[question]]` entries with `prompt`, `options` (array), `answer` (0-based index), `explanation`, `spec`, `difficulty` (1-3), and optionally `shuffle = false`
- Item `spec` values may name a lettered sub-point, e.g. `"1.4.3(b)"`. Tag each item with the most specific sub-point it covers.
- Quiz options and question order are shuffled on every attempt. Explanations must describe the answer itself and never refer to an option by letter or position ("option B", "the first one"); the loader rejects "option B"-style phrases. Use `shuffle = false` only where order matters (numbers in sequence, "All of the above").
- All text is Markdown. Put Boolean expressions, code, binary and hex in backticks so they use the mono font (Inter has no `∧ ∨ ⊻` glyphs). Use OCR notation: `∧ ∨ ¬ ⊻ ≡`. `<sup>` and `<sub>` are the only HTML allowed; any other HTML is shown as text.
- Create content with the `/revision-set` skill. If a content file fails to load, fix the file, not the loader.

## Sources
- `sources/` holds the OCR specification, a scanned textbook (school copy), web notes that cover required content, OCR exam questions with mark schemes, and a student's own revision notes (shared with their permission). It is gitignored: never commit it or serve it from the site.
- Work from the converted text, not the originals. `tools/convert-sources` turns every file into Markdown sections in `sources/text/<type>/<source>/`, each starting with its source, type, trust level and pages, and lists them in `sources/index.md` with the spec points they cover. `sources/manifest.toml` catalogues each original file. Re-run `tools/convert-sources` after adding files to `sources/` (it only converts new or changed files). Never open a PDF in `sources/` when a text version exists.
- To gather material for a spec point, use the `source-reader` subagent rather than reading the sources yourself.
- The OCR specification decides what is in scope. The textbook and the web notes are the main sources of facts and are trusted equally.
- The web notes cover required content, so use them as fully as the textbook. If they disagree with the textbook or the spec, flag it for the teacher rather than picking one.
- Student notes are never the only source for a fact. Use them to spot common misconceptions (good wrong options for quizzes) and how students phrase things.
- Exam questions and mark schemes show command words, mark allocation and what examiners reward. Never copy them into `content/`; write new questions in the same style.
- Nothing from `sources/` is copied into `content/` word for word. The one exception is `content/spec.toml`, which quotes the specification's own point titles and sub-point text so they can be checked against.
- Don't reference URLs found in the sources, and keep personal details out of everything (names to remove are listed in `sources/redact.toml`). Text marked "unverified transcription" was read from handwriting by an AI model, and text marked "AI description of a diagram" was written by a local AI model from a diagram in a scanned page (tables as Markdown, circuits, trees and graphs as Mermaid). Both must be checked against the original page before they are relied on.
- Pseudocode, Little Man Computer, SQL, HTML/CSS/JavaScript and Boolean notation in content follow OCR's conventions: see `docs/ocr-pseudocode.md` (a summary of spec appendix 5d).

## Rules
- Pages must work well on a phone (390px wide) and a desktop.
- Phones: every tap target is at least 44x44px (links in running text excepted), full-height layouts use `dvh` with a `vh` fallback, and swipe gestures sit alongside buttons, never replacing them. The site has a web app manifest and icons in `static/` for Add to Home Screen.
- Accessibility: semantic HTML, labelled form controls, visible focus, WCAG AA contrast.
- After a UI change, open the page with the Playwright MCP server at phone and desktop widths and check it visually.
- A task is only done when `cargo fmt`, `cargo clippy -- -D warnings` and `cargo test` all pass.
