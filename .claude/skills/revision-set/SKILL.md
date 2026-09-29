---
name: revision-set
description: Create or extend a revision set (notes, flashcards and a quiz) for one A level Computer Science topic, using the converted sources via the source-reader subagent. Use when asked to make revision content, flashcards or quiz questions.
argument-hint: <topic or spec point>
---

Create a revision set for: $ARGUMENTS

1. Look up the spec point and its lettered sub-points in `content/spec.toml`. Cover every sub-point, and tag each card and question with the sub-point it tests (e.g. `"1.4.3(b)"`); the loader rejects tags that aren't in `spec.toml`. If you can't tell which spec point is meant, ask.
2. Ask the `source-reader` subagent for the spec point (one call per spec point, or per sub-point if the point is large). Work from its tagged report; do not read `sources/` yourself.
   - If you need to check a detail, open only the file in `sources/text/` that the report cites. Never open a PDF in `sources/` when a text version exists.
   - If files have been added to `sources/` since `sources/index.md` was built, run `tools/convert-sources` first.
   - Follow the Sources rules in CLAUDE.md:
     - Facts come from the spec, the textbook and the web notes, used equally.
     - Student notes only supply misconceptions (use them for wrong quiz options) and student phrasing.
     - Exam questions and mark schemes show the style, command words and mark allocation, but are never copied.
     - If the sources disagree, or the report lists a disagreement, flag it for the teacher in your summary instead of choosing one.
3. Pick a topic slug (lowercase, hyphens) and create or update `content/<slug>/`:
   - `notes.md`: concise revision notes in original wording. Short sections, key terms in bold, one worked example where it helps. Front matter: `title`, `spec`.
   - `flashcards.toml`: 10 to 20 cards. One fact or definition per card. The back is a model answer a student could write in the exam.
   - `quiz.toml`: 8 to 12 multiple-choice questions across difficulty 1 to 3. Wrong options are common student misconceptions, not obviously silly answers. Every question has an explanation of why the answer is right.
   - Options are shuffled for every attempt, so an explanation must describe the answer itself and never refer to an option by letter or position ("option B", "the first one"). Add `shuffle = false` only when the options must stay in order (numbers in sequence, "All of the above").
   - Put Boolean expressions, code, binary and hex in backticks. Use `<sup>`/`<sub>` for powers and number bases; no other HTML.
4. Use the specification's own terminology and conventions (its names for registers, and the pseudocode, LMC, SQL and Boolean notation in `docs/ocr-pseudocode.md`).
5. Do not copy sentences, figures or past-paper questions from the sources, and never include URLs found in them. Paraphrase, and write new questions in the style of the exam questions (their command words and mark allocations).
6. Ask the content-checker subagent to review the new files against the spec and the sources, then fix anything it reports as wrong.
7. Run `cargo test` so the content loader confirms the files parse (once content tests exist).
8. Finish with a short summary: spec points covered, number of cards and questions, any disagreements between sources, and anything the teacher should double-check.
