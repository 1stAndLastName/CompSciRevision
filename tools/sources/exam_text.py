"""Copy the text of the OCR exam questions and mark schemes into exam-questions/.

The question papers and mark schemes in sources/Exam Questions/ are PDFs with
real text in them, so this reads the text straight from the PDF with pdfplumber
(no OCR and no images): every character with its font and position. That lets
it keep what plain text loses:

- pseudocode (Courier font) becomes a code block with its indentation
- bold words stay bold, and small raised or lowered characters become <sup>/<sub>
- tables (drawn with ruled cells) become Markdown tables
- the mark scheme table is read cell by cell, so the answer, marks and guidance
  columns are no longer mixed together line by line
- each question is split into its parts, 1(a), 1(b)(i) and so on, and each
  mark scheme row is matched to its part

Diagrams are images in the PDFs. They are not copied: the part that had one
gets a note saying a diagram is not shown, and the question is marked
`diagram = true`.

Output: one TOML file per spec point, exam-questions/<spec point>.toml, which
the site loads at startup (see src/exam.rs). Existing files are left alone
unless --force is given, so hand corrections are not overwritten by accident.

Run it with tools/extract-exam-questions.
"""

from __future__ import annotations

import argparse
import re
import sys
import tomllib
from collections import Counter, defaultdict
from dataclasses import dataclass, field
from pathlib import Path

import pdfplumber

ROOT = Path(__file__).resolve().parents[2]
SOURCES = ROOT / "sources"
MANIFEST = SOURCES / "manifest.toml"
REDACT = SOURCES / "redact.toml"
SPEC_TOML = ROOT / "content" / "spec.toml"
OUT = ROOT / "exam-questions"

DIAGRAM_NOTE = "*[Diagram in the original paper, not shown here.]*"
IMAGE_NOTE = "*[Image in the original mark scheme, not shown here.]*"
# Images smaller than this (in points) are icons or ticks, not diagrams.
MIN_IMAGE = 24


# ---------------------------------------------------------------------------
# Characters and lines
# ---------------------------------------------------------------------------


@dataclass
class Char:
    text: str
    x0: float
    x1: float
    base: float  # baseline, measured down from the top of the page
    size: float
    mono: bool
    bold: bool


@dataclass
class Line:
    chars: list[Char]
    base: float
    size: float

    @property
    def x0(self) -> float:
        return min(c.x0 for c in self.chars if c.text.strip())

    @property
    def x1(self) -> float:
        return max(c.x1 for c in self.chars if c.text.strip())

    @property
    def mono(self) -> bool:
        visible = [c for c in self.chars if c.text.strip()]
        return sum(c.mono for c in visible) >= 0.8 * len(visible)

    def plain(self) -> str:
        return render_chars(self.chars, markup=False)


def page_chars(page) -> list[Char]:
    """Every character on the page, except the page header (Helvetica)."""
    chars = []
    for c in page.chars:
        font = c["fontname"].split("+")[-1]
        if font.startswith("Helvetica") or not c.get("upright", True):
            continue
        # The baseline comes from the text matrix. pdfplumber's "top" uses the
        # font's bounding box, which is wrong for some maths fonts (∧ in
        # Cambria Math lands a line lower than the text around it).
        base = page.height - c["matrix"][5]
        chars.append(
            Char(
                text=c["text"],
                x0=c["x0"],
                x1=c["x1"],
                base=base,
                size=c["size"],
                mono="Courier" in font,
                bold="Bold" in font,
            )
        )
    return chars


def inside(char: Char, box) -> bool:
    x0, top, x1, bottom = box
    middle = (char.x0 + char.x1) / 2
    y = char.base - char.size * 0.3
    return x0 - 1 <= middle <= x1 + 1 and top - 1 <= y <= bottom + 1


def group_lines(chars: list[Char]) -> list[Line]:
    """Group characters into lines by baseline. Small characters a little above
    or below a line (powers, subscripts) join that line."""
    visible = [c for c in chars if c.text.strip()]
    if not visible:
        return []
    main_size = Counter(round(c.size) for c in visible).most_common(1)[0][0]
    normal = sorted((c for c in chars if c.size >= main_size * 0.85), key=lambda c: c.base)
    small = [c for c in chars if c.size < main_size * 0.85]
    lines: list[Line] = []
    for c in normal:
        if lines and abs(c.base - lines[-1].base) <= 2.5:
            lines[-1].chars.append(c)
        else:
            lines.append(Line([c], c.base, c.size))
    for c in small:
        near = [line for line in lines if abs(line.base - c.base) <= line.size * 0.6]
        if near:
            min(near, key=lambda line: abs(line.base - c.base)).chars.append(c)
        else:
            lines.append(Line([c], c.base, c.size))
    lines.sort(key=lambda line: line.base)
    for line in lines:
        line.chars.sort(key=lambda c: c.x0)
        line.size = Counter(round(c.size, 1) for c in line.chars if c.text.strip()).most_common(1)[0][0] if any(
            c.text.strip() for c in line.chars
        ) else line.size
    return [line for line in lines if any(c.text.strip() for c in line.chars)]


MARKDOWN_SPECIAL = re.compile(r"([\\`*_\[\]<>|])")


def escape(text: str) -> str:
    return MARKDOWN_SPECIAL.sub(r"\\\1", text)


# A Boolean expression written with OCR's symbols, such as ¬(A ∧ B) ∨ C or
# Q ≡ A ⊻ B. These go in `code` so they use the mono font, which has the symbols.
BOOL_VAR = r"[A-Z01]"
BOOL_OP = r"(?:[∧∨⊻≡.+]|v(?=\s?[¬(A-Z01]))"
BOOLEAN = re.compile(
    rf"(?<![\w.])(?:[A-Z]\s?[=≡]\s?)?[¬(\s]*{BOOL_VAR}(?:[\s)]*{BOOL_OP}[\s¬(]*{BOOL_VAR})*[)\s]*(?![\w])"
)


def boolean_spans(text: str) -> list[tuple[int, int]]:
    spans = []
    for match in BOOLEAN.finditer(text):
        start, end = match.span()
        while start < end and text[start].isspace():
            start += 1
        while end > start and text[end - 1].isspace():
            end -= 1
        found = text[start:end]
        if not re.search(r"[∧∨⊻≡¬]", found):
            continue
        # Balance the brackets by trimming unmatched ones at the ends.
        while found.count(")") > found.count("(") and found.endswith(")"):
            found, end = found[:-1].rstrip(), end - 1 - (len(found[:-1]) - len(found[:-1].rstrip()))
        while found.count("(") > found.count(")") and found.startswith("("):
            trimmed = found[1:].lstrip()
            start += len(found) - len(trimmed)
            found = trimmed
        spans.append((start, end))
    return spans


def render_chars(chars: list[Char], markup: bool = True, line_base: float | None = None) -> str:
    """Turn one line of characters into text. With markup, bold runs become
    **bold**, Courier runs and Boolean expressions become `code`, and small
    raised or lowered characters become <sup>/<sub>."""
    if not chars:
        return ""
    visible = [c for c in chars if c.text.strip()]
    size = Counter(round(c.size, 1) for c in visible).most_common(1)[0][0] if visible else chars[0].size
    if line_base is None and visible:
        normal = [round(c.base, 1) for c in visible if c.size >= size * 0.85] or [round(visible[0].base, 1)]
        line_base = Counter(normal).most_common(1)[0][0]
    # The line as plain text, one item per character, with a space added
    # wherever there is a gap between characters.
    items: list[tuple[str, Char | None]] = []
    previous: Char | None = None
    for c in chars:
        if previous is not None and c.text.strip() and c.x0 - previous.x1 > size * 0.18:
            if not (items and not items[-1][0].strip()):
                items.append((" ", None))
        items.append((c.text, c))
        if c.text.strip():
            previous = c
    plain = "".join(text for text, _ in items)
    if not markup:
        return re.sub(r"[ \t]+", " ", plain).strip()
    in_boolean = [False] * len(plain)
    for start, end in boolean_spans(plain):
        for index in range(start, end):
            in_boolean[index] = True
    # Give every item a style, then join items of the same style into runs.
    runs: list[list] = []  # [style, text]
    offset = 0
    for text, c in items:
        here = offset
        offset += len(text)
        if not text.strip():
            style = "code" if in_boolean[here] else (runs[-1][0] if runs else "plain")
        elif in_boolean[here]:
            style = "code"
        elif c.size < size * 0.85 and line_base - c.base > 1.2:
            style = "sup"
        elif c.size < size * 0.85 and c.base - line_base > 1.2:
            style = "sub"
        else:
            style = "code" if c.mono else "bold" if c.bold else "plain"
        if runs and runs[-1][0] == style:
            runs[-1][1] += text
        else:
            runs.append([style, text])
    out = []
    for style, text in runs:
        if style == "plain":
            out.append(escape(text))
            continue
        core = text.strip()
        if not core:
            out.append(text)
            continue
        lead = text[: len(text) - len(text.lstrip())]
        trail = text[len(text.rstrip()) :]
        if style == "code":
            core = re.sub(r"\s+", " ", core)
            core = f"`` {core} ``" if "`" in core else f"`{core}`"
        elif style == "bold":
            core = f"**{escape(core)}**"
        else:
            core = f"<{style}>{escape(core)}</{style}>"
        out.append(lead + core + trail)
    return re.sub(r"[ \t]+", " ", "".join(out)).strip()


def code_text(lines: list[Line]) -> str:
    """Lines of Courier text as a code block body. Indentation is the number of
    space characters at the start of each line (the papers indent with spaces,
    not all of them in Courier, so positions would squash it). Wider gaps inside
    a line become runs of spaces, so columns roughly line up."""
    width = 0.6 * lines[0].size  # every Courier character is 0.6 em wide
    out = []
    previous = None
    for line in lines:
        if previous is not None and line.base - previous.base > line.size * 1.9:
            out.append("")
        chars = line.chars
        lead = 0
        while lead < len(chars) and not chars[lead].text.strip():
            lead += 1
        text = " " * lead
        last = None
        for c in chars[lead:]:
            if last is not None and c.text.strip():
                gap = c.x0 - last.x1
                if gap > width * 1.5:
                    text += " " * max(1, round(gap / width))
                elif gap > width * 0.4 and not text.endswith(" "):
                    text += " "
            text += c.text
            last = c
        out.append(text.rstrip())
        previous = line
    # Remove indentation shared by every line.
    shared = min((len(t) - len(t.lstrip()) for t in out if t.strip()), default=0)
    return "\n".join(t[shared:] for t in out)


def fence(body: str) -> str:
    marker = "```"
    while marker in body:
        marker += "`"
    return f"{marker}\n{body}\n{marker}"


BULLET = re.compile(r"^[•●▪◦·\-–]\s*")
ANSWER_LINE = re.compile(r"^[\s.…_]{6,}$")


def lines_to_markdown(lines: list[Line], right: float) -> list[tuple[float, str]]:
    """Turn lines into Markdown blocks: code blocks for Courier lines, bullet
    lists, and paragraphs (lines are joined unless a line stops short, which
    means the line break was meant). Returns (top, markdown) pairs."""
    blocks: list[tuple[float, str]] = []
    i = 0
    while i < len(lines):
        line = lines[i]
        if ANSWER_LINE.match(line.plain()):
            i += 1
            continue
        if line.mono:
            group = [line]
            while i + 1 < len(lines) and lines[i + 1].mono and lines[i + 1].base - group[-1].base < line.size * 3.2:
                i += 1
                group.append(lines[i])
            blocks.append((line.base, fence(code_text(group))))
            i += 1
            continue
        # A paragraph or a bullet list.
        start = line.base
        items: list[list[str]] = []
        is_list = bool(BULLET.match(line.plain()))
        current: list[str] = []
        previous = None
        while i < len(lines):
            line = lines[i]
            if line.mono or ANSWER_LINE.match(line.plain()):
                break
            if previous is not None and line.base - previous.base > previous.size * 1.75:
                break
            text = render_chars(line.chars)
            bullet = BULLET.match(line.plain())
            if bullet and previous is not None and not is_list:
                break
            if bullet:
                if current:
                    items.append(current)
                current = [BULLET.sub("", text.replace("\\-", "-", 1), count=1).strip()]
            elif previous is not None and not is_list and previous.x1 < right - max(60, (right - previous.x0) * 0.2):
                # The previous line stopped well short of the edge: keep the break.
                current.append("\\\n" + text)
            else:
                current.append(text)
            previous = line
            i += 1
        if current:
            items.append(current)
        if is_list:
            blocks.append((start, "\n".join("- " + " ".join(item).replace(" \\\n", "\\\n") for item in items)))
        else:
            for item in items:
                blocks.append((start, " ".join(item).replace(" \\\n", "\\\n")))
    return blocks


def table_markdown(table, chars: list[Char]) -> str | None:
    """A ruled table as Markdown, or None if it has no text (an answer box).
    Tables with code or several lines in a cell are written out cell by cell
    instead, because a Markdown table cell holds one line."""
    rows = []
    multi = False
    for row in table.rows:
        cells = []
        for cell in row.cells:
            if cell is None:
                cells.append(None)
                continue
            lines = group_lines([c for c in chars if inside(c, cell)])
            # Code over several lines, or a long piece of text, will not fit in
            # a Markdown table cell.
            if (len(lines) > 1 and any(line.mono for line in lines)) or len(lines) > 4:
                multi = True
            cells.append((cell, lines))
        rows.append(cells)
    if not any(lines for row in rows for cell in row if cell for lines in [cell[1]]):
        return None
    if multi:
        # Side-by-side boxes (a heading row over two pieces of code, say) read
        # best one column at a time; bigger tables read row by row.
        order = rows
        if len(rows) <= 2:
            order = [list(column) for column in zip(*rows)] if len({len(r) for r in rows}) == 1 else rows
        blocks = []
        for row in order:
            for cell in row:
                if not cell or not cell[1]:
                    continue
                box, lines = cell
                blocks.extend(md for _, md in lines_to_markdown(lines, box[2]))
        return "\n\n".join(blocks)
    table_rows = []
    for row in rows:
        texts = []
        for cell in row:
            if cell is None:
                continue
            texts.append(" ".join(render_chars(line.chars) for line in cell[1]).replace("|", "\\|") or " ")
        table_rows.append(texts)
    width = max(len(r) for r in table_rows)
    table_rows = [r + [" "] * (width - len(r)) for r in table_rows]
    out = ["| " + " | ".join(table_rows[0]) + " |", "|" + "---|" * width]
    out += ["| " + " | ".join(r) + " |" for r in table_rows[1:]]
    return "\n".join(out)


def region_blocks(page, chars: list[Char], box, tables, image_note: str, skip_tables=()) -> tuple[list[tuple[float, str]], bool, list[tuple[int, tuple]]]:
    """Markdown blocks for everything inside `box`: tables, images and text.
    Returns the blocks in reading order, whether an image was found, and for
    each image note (in the same order) its page number and box, so that
    tools/redraw-exam-diagrams can crop the diagram behind it."""
    x0, top, x1, bottom = box
    blocks: list[tuple[float, str]] = []
    taken: set[int] = set()
    region = [c for c in chars if inside(c, box)]
    for table in tables:
        if table in skip_tables:
            continue
        tx0, ttop, tx1, tbottom = table.bbox
        if tx0 >= x0 - 2 and tx1 <= x1 + 2 and ttop >= top - 2 and tbottom <= bottom + 2:
            in_table = [c for c in region if inside(c, table.bbox)]
            taken.update(id(c) for c in in_table)
            markdown = table_markdown(table, in_table)
            if markdown:
                blocks.append((ttop, markdown))
    has_image = False
    image_boxes: dict[int, tuple] = {}  # position in `blocks` -> image box
    for image in page.images:
        w, h = image["x1"] - image["x0"], image["bottom"] - image["top"]
        cx, cy = (image["x0"] + image["x1"]) / 2, (image["top"] + image["bottom"]) / 2
        if w >= MIN_IMAGE and h >= MIN_IMAGE and x0 <= cx <= x1 and top <= cy <= bottom:
            image_boxes[len(blocks)] = (image["x0"], image["top"], image["x1"], image["bottom"])
            blocks.append((image["top"], image_note))
            has_image = True
    rest = [c for c in region if id(c) not in taken]
    tagged = [(top_, text, image_boxes.get(i)) for i, (top_, text) in enumerate(blocks)]
    tagged += [(top_, text, None) for top_, text in lines_to_markdown(group_lines(rest), x1)]
    tagged.sort(key=lambda b: b[0])
    # Drop repeated notes (one diagram can be several images): the note that
    # stays covers all of their boxes.
    out: list[tuple[float, str]] = []
    boxes: list[tuple] = []
    for top_, text, image_box in tagged:
        if out and text == out[-1][1] and text in (DIAGRAM_NOTE, IMAGE_NOTE) and image_box and boxes:
            a = boxes[-1]
            boxes[-1] = (min(a[0], image_box[0]), min(a[1], image_box[1]), max(a[2], image_box[2]), max(a[3], image_box[3]))
            continue
        out.append((top_, text))
        if image_box:
            boxes.append(image_box)
    return out, has_image, [(page.page_number, b) for b in boxes]


# ---------------------------------------------------------------------------
# Question papers
# ---------------------------------------------------------------------------

QUESTION_LABEL = re.compile(r"^(\d{1,2})(?:\(([a-z])\))?(?:\(([ivx]+)\))?\.$")
LETTER_LABEL = re.compile(r"^\(([a-z])\)\.?$")
ROMAN_LABEL = re.compile(r"^\(?([ivx]{1,4})\)?\.$")


@dataclass
class Part:
    label: str
    blocks: list[str] = field(default_factory=list)
    marks: int | None = None
    diagram: bool = False
    # (page number, box) for each diagram note in `blocks`, in order.
    images: list = field(default_factory=list)


@dataclass
class Question:
    number: int
    parts: list[Part] = field(default_factory=list)
    pages: set[int] = field(default_factory=set)


def first_word(line: Line) -> tuple[str, list[Char], list[Char]]:
    """The first word of a line, its characters and the characters after it."""
    word: list[Char] = []
    rest_start = 0
    for index, c in enumerate(line.chars):
        if not c.text.strip():
            if word:
                rest_start = index
                break
            continue
        if word and c.x0 - word[-1].x1 > c.size * 0.18:
            rest_start = index
            break
        word.append(c)
        rest_start = index + 1
    return "".join(c.text for c in word), word, line.chars[rest_start:]


def read_questions(path: Path) -> list[Question]:
    questions: list[Question] = []
    all_parts: list[Part] = []
    question_of: dict[int, Question] = {}
    letter = roman = None
    with pdfplumber.open(path) as pdf:
        for number, page in enumerate(pdf.pages, start=1):
            chars = page_chars(page)
            tables = page.find_tables()
            box = (0, 20, page.width, page.height)
            # Split the page at each label line, so each piece of the page goes
            # to the right part. Labels are found on lines outside tables.
            table_chars = {id(c) for t in tables for c in chars if inside(c, t.bbox)}
            lines = group_lines([c for c in chars if id(c) not in table_chars])
            cuts: list[tuple[float, str]] = []  # (top of the label line, label)
            in_progress = all_parts[-1] if all_parts else None
            before = len(all_parts)
            for line in lines:
                word, word_chars, _ = first_word(line)
                if not word_chars or word_chars[0].x0 > 55:
                    continue
                label = None
                q = QUESTION_LABEL.match(word)
                last = questions[-1].number if questions else 0
                if word == "." and word_chars[0].bold and word_chars[0].x0 < 30:
                    # A few questions lost their number in the PDF: just a bold "." is left.
                    q = QUESTION_LABEL.match(f"{last + 1}.")
                if q and all(c.bold for c in word_chars):
                    n = int(q.group(1))
                    if last < n <= last + 3:
                        letter, roman = q.group(2), q.group(3)
                        label = f"{n}" + (f"({letter})" if letter else "") + (f"({roman})" if roman else "")
                        questions.append(Question(n))
                elif questions and LETTER_LABEL.match(word) and all(c.bold for c in word_chars):
                    letter, roman = LETTER_LABEL.match(word).group(1), None
                    label = f"{questions[-1].number}({letter})"
                elif questions and ROMAN_LABEL.match(word) and not any(c.mono for c in word_chars):
                    roman = ROMAN_LABEL.match(word).group(1)
                    label = f"{questions[-1].number}" + (f"({letter})" if letter else "") + f"({roman})"
                if label:
                    part = Part(label)
                    questions[-1].parts.append(part)
                    all_parts.append(part)
                    question_of[id(part)] = questions[-1]
                    cuts.append((line.base - line.size * 1.1, label))
                    # Blank out the label itself so it is not copied into the text.
                    for c in word_chars:
                        c.text = ""
            new_parts = all_parts[before:]
            # The page is cut at each label line. The piece above the first
            # label belongs to the part that was in progress at the top of the page.
            owners = [in_progress] + new_parts
            edges = [box[1]] + [cut[0] for cut in cuts] + [page.height]
            for piece, owner in enumerate(owners):
                if owner is None:
                    continue
                piece_box = (0, edges[piece], page.width, edges[piece + 1])
                blocks, has_image, images = region_blocks(page, chars, piece_box, tables, DIAGRAM_NOTE)
                owner.blocks.extend(markdown for _, markdown in blocks)
                owner.diagram |= has_image
                owner.images.extend(images)
                question_of[id(owner)].pages.add(number)
    for question in questions:
        for part in question.parts:
            take_marks(part)
        question.parts = [p for p in question.parts if p.blocks or p.marks is not None]
    return questions


def take_marks(part: Part) -> None:
    """Move the "[4]" at the end of a part's text into part.marks."""
    total = 0
    found = False
    kept = []
    for block in part.blocks:
        # Marks are printed in bold: "**\[4\]**" once escaped.
        match = re.search(r"\s*\*\*\\\[(\d{1,2})\\\]\*\*\s*$", block)
        if match:
            total += int(match.group(1))
            found = True
            block = block[: match.start()].rstrip().rstrip("\\").rstrip()
        if block.strip():
            kept.append(block)
    part.blocks = kept
    if found:
        part.marks = total


# ---------------------------------------------------------------------------
# Mark schemes
# ---------------------------------------------------------------------------


@dataclass
class MarkRow:
    label: str
    marks: int | None = None
    answer: list[str] = field(default_factory=list)
    guidance: list[str] = field(default_factory=list)
    # (page number, box) for each image note in `answer` / `guidance`, in order.
    answer_images: list = field(default_factory=list)
    guidance_images: list = field(default_factory=list)


LABEL_TOKEN = re.compile(r"^\(?(\d{1,2}|[a-h]|[ivx]{1,4})\)?\.?$")
MARKS_CELL = re.compile(r"^(?:AO\s?\d(?:\.\d)?[\s,]*(?:x\s?\d)?\s*)*\(?(\d{1,2})\)?(?:\s*AO.*)?$|^\d{1,2}\s*(?:marks?)?$", re.I)


def cell_text(chars: list[Char], box) -> str:
    return " ".join(line.plain() for line in group_lines([c for c in chars if inside(c, box)])).strip()


def split_label(text: str) -> list[str] | None:
    """"1 a i", "1(a)(i)", "b", "ii" -> tokens; None if this is not a label."""
    # A two-digit number can wrap in the narrow cell: "1 0" is 10.
    text = re.sub(r"(\d)\s+(\d)", r"\1\2", text.strip())
    if not text:
        return []
    tokens = re.findall(r"\d{1,2}|[a-z]{1,4}", text)
    if not tokens or re.sub(r"[\s().\d a-z]", "", text):
        return None
    if all(LABEL_TOKEN.match(t) for t in tokens) and len(tokens) <= 3:
        return tokens
    return None


def read_mark_scheme(path: Path) -> tuple[list[MarkRow], dict[int, int]]:
    rows: list[MarkRow] = []
    totals: dict[int, int] = {}
    number = letter = roman = None
    columns: dict[str, tuple[float, float]] | None = None
    after_total = False
    with pdfplumber.open(path) as pdf:
        for page in pdf.pages:
            chars = page_chars(page)
            tables = page.find_tables()
            outer = [t for t in tables if t.bbox[2] - t.bbox[0] > page.width * 0.5]
            inner = [t for t in tables if t not in outer]
            for table in outer:
                for row in table.rows:
                    cells = [c for c in row.cells if c is not None]
                    texts = [cell_text(chars, c) for c in cells]
                    if not any(texts):
                        continue
                    joined = " ".join(texts)
                    if "Guidance" in joined and ("Question" in joined or "Mark" in joined):
                        continue
                    # "Total 8" rows.
                    total_at = next((i for i, t in enumerate(texts) if t == "Total"), None)
                    if total_at is not None:
                        after = next((t for t in texts[total_at + 1 :] if t), "")
                        if number and re.fullmatch(r"\d{1,3}", after):
                            totals[number] = int(after)
                            after_total = True
                        continue
                    # Label cells: the narrow cells at the left.
                    label_tokens: list[str] = []
                    first_content = 0
                    for index, (cell, text) in enumerate(zip(cells, texts)):
                        if cell[2] - cell[0] > 60:
                            break
                        tokens = split_label(text)
                        if tokens is None:
                            break
                        label_tokens += tokens
                        first_content = index + 1
                    content = list(zip(cells[first_content:], texts[first_content:]))
                    # The marks cell: a short cell that is just a number (maybe with AO codes).
                    marks_at = None
                    for index, (cell, text) in enumerate(content):
                        if index > 0 and cell[2] - cell[0] < 90 and MARKS_CELL.match(text or "-"):
                            marks_at = index
                            break
                    if label_tokens:
                        for token in label_tokens:
                            if token.isdigit():
                                number, letter, roman = int(token), None, None
                            elif re.fullmatch(r"[ivx]{1,4}", token) and (letter is not None or token not in "i"):
                                roman = token
                            elif re.fullmatch(r"[a-h]", token):
                                letter, roman = token, None
                            else:
                                roman = token
                        # After a "Total" row a new question starts, even when
                        # its number is missing from the cell.
                        if after_total and not any(t.isdigit() for t in label_tokens) and number is not None:
                            number += 1
                        after_total = False
                        if number is None:
                            continue
                        label = f"{number}" + (f"({letter})" if letter else "") + (f"({roman})" if roman else "")
                        current = MarkRow(label)
                        rows.append(current)
                    elif not rows:
                        continue
                    else:
                        current = rows[-1]
                    if marks_at is not None:
                        answer_cells = content[:marks_at]
                        guidance_cells = content[marks_at + 1 :]
                        m = MARKS_CELL.match(content[marks_at][1])
                        value = next((g for g in m.groups() if g), None) if m else None
                        if value is None:
                            value = re.search(r"\d{1,2}", content[marks_at][1]).group(0)
                        if label_tokens or current.marks is None:
                            current.marks = int(value) if current.marks is None else current.marks + int(value)
                        columns = {
                            "answer": (answer_cells[0][0][0], answer_cells[-1][0][2]) if answer_cells else None,
                            "guidance": (guidance_cells[0][0][0], guidance_cells[-1][0][2]) if guidance_cells else None,
                        }
                    else:
                        # No marks cell (a row carried over from the last page):
                        # use the column positions of the last full row.
                        answer_cells, guidance_cells = [], []
                        for cell, text in content:
                            middle = (cell[0] + cell[2]) / 2
                            g = columns and columns.get("guidance")
                            if g and middle >= g[0] - 2:
                                guidance_cells.append((cell, text))
                            else:
                                answer_cells.append((cell, text))
                    for target, image_list, cells_in in (
                        (current.answer, current.answer_images, answer_cells),
                        (current.guidance, current.guidance_images, guidance_cells),
                    ):
                        for cell, text in cells_in:
                            blocks, _, images = region_blocks(page, chars, cell, inner, IMAGE_NOTE)
                            target.extend(md for _, md in blocks)
                            image_list.extend(images)
    for row in rows:
        if row.marks is None:
            for block in row.answer:
                found = re.fullmatch(r"(\d{1,2})\s*(?:\(AO[\d.]+\)\s*\(\d{1,2}\)\s*)+", block.strip())
                if found:
                    row.marks = int(found.group(1))
                    row.answer.remove(block)
                    break
    return rows, totals


# ---------------------------------------------------------------------------
# Putting it together
# ---------------------------------------------------------------------------


def redact(text: str, redactions: list[str]) -> str:
    for item in redactions:
        text = re.sub(re.escape(item), "[removed]", text, flags=re.I)

    # No web addresses: they would point readers at other sites. One in a
    # question ("a user enters ... such as http://...") becomes "[web address]"
    # so the sentence still reads; full stops after it are kept.
    def web_address(match: re.Match) -> str:
        trailing = re.search(r"[.,;:)]*$", match.group(0)).group(0)
        return "[web address]" + trailing

    text = re.sub(r"\bhttps?:/+\S*|\bwww\.\S+|\b[\w-]+\.(?:com|co\.uk|org\.uk|org)\b\S*", web_address, text)
    return re.sub(r"(\[web address\][.,;:)]*\s*){2,}", "[web address] ", text)


def drop_support_links(blocks: list[str]) -> list[str]:
    """Mark schemes end some rows with "OCR support: resources can be found in
    this document" and a web address. Drop that heading and what follows it,
    up to the next bold heading."""
    kept = []
    skipping = False
    for block in blocks:
        if block.startswith("**OCR support"):
            skipping = True
            continue
        if skipping and block.startswith("**"):
            skipping = False
        if not skipping:
            kept.append(block)
    return kept


def toml_string(text: str) -> str:
    if "'''" not in text and "\r" not in text:
        return "'''\n" + text + "'''" if "\n" in text or "'" in text else "'" + text + "'"
    return '"""\n' + text.replace("\\", "\\\\").replace('"""', '\\"\\"\\"') + '"""'


def tidy(blocks: list[str]) -> str:
    text = "\n\n".join(b.strip() for b in blocks if b.strip())
    # Dotted lines to write on ("Final path: ........") become a short blank.
    text = re.sub(r"(?:\.|…){5,}", "……", text)
    return re.sub(r"\n{3,}", "\n\n", text).strip()


def point_title(spec_number: str) -> str:
    spec = tomllib.loads(SPEC_TOML.read_text())
    for point in spec.get("point", []):
        if point["number"] == spec_number:
            return point["title"]
    raise SystemExit(f"{spec_number} is not in content/spec.toml")


def write_topic(entry: dict, mark_scheme: dict, redactions: list[str], force: bool) -> Path | None:
    spec_number = entry["spec"][0]
    out = OUT / f"{spec_number}.toml"
    if out.exists() and not force:
        print(f"  {out.relative_to(ROOT)} exists, left alone (use --force to replace it)")
        return None
    questions = read_questions(SOURCES / entry["path"])
    rows, totals = read_mark_scheme(SOURCES / mark_scheme["path"])
    by_question: dict[int, list[MarkRow]] = defaultdict(list)
    for row in rows:
        by_question[int(re.match(r"\d+", row.label).group(0))].append(row)

    lines = [
        f"# OCR exam questions for {spec_number} {point_title(spec_number)}.",
        "# Copied as text from the question paper and mark scheme PDFs in sources/",
        "# by tools/extract-exam-questions. Diagrams are not copied as images; run",
        "# tools/redraw-exam-diagrams to redraw them as text. Corrections by hand are",
        "# fine: the tool will not overwrite this file without --force.",
        f'spec = "{spec_number}"',
        "",
    ]
    for question in questions:
        marks = sum(p.marks or 0 for p in question.parts)
        total = totals.get(question.number)
        lines.append("[[question]]")
        lines.append(f"number = {question.number}")
        lines.append(f"marks = {total or marks}")
        if any(p.diagram for p in question.parts):
            lines.append("diagram = true")
        lines.append("")
        for part in question.parts:
            lines.append("[[question.part]]")
            lines.append(f'label = "{part.label}"')
            if part.marks is not None:
                lines.append(f"marks = {part.marks}")
            lines.append("text = " + toml_string(redact(tidy(part.blocks), redactions) + "\n"))
            lines.append("")
        for row in by_question.get(question.number, []):
            lines.append("[[question.mark_scheme]]")
            lines.append(f'label = "{row.label}"')
            if row.marks is not None:
                lines.append(f"marks = {row.marks}")
            lines.append("answer = " + toml_string(redact(tidy(drop_support_links(row.answer)), redactions) + "\n"))
            guidance = redact(tidy(drop_support_links(row.guidance)), redactions)
            if guidance:
                lines.append("guidance = " + toml_string(guidance + "\n"))
            lines.append("")
        if question.number not in by_question:
            print(f"  question {question.number}: no mark scheme found")
        if total is not None and marks and total != marks:
            print(f"  question {question.number}: parts add up to {marks}, mark scheme total is {total}")
    OUT.mkdir(exist_ok=True)
    out.write_text("\n".join(lines).rstrip() + "\n")
    return out


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--force", action="store_true", help="replace files that already exist")
    parser.add_argument("--only", help="only spec points starting with this, e.g. 1.4.3")
    args = parser.parse_args()
    manifest = tomllib.loads(MANIFEST.read_text())["file"]
    by_path = {entry["path"]: entry for entry in manifest}
    redactions = [item["text"] for item in tomllib.loads(REDACT.read_text()).get("remove", [])] if REDACT.exists() else []
    papers = [e for e in manifest if e.get("type") == "exam-questions" and e.get("part") == "questions"]
    if args.only:
        papers = [e for e in papers if e["spec"][0].startswith(args.only)]
    for entry in papers:
        mark_scheme = by_path.get(entry.get("mark_scheme", ""))
        if not mark_scheme:
            print(f"{entry['path']}: no mark scheme in the manifest, skipped", file=sys.stderr)
            continue
        print(entry["spec"][0], entry["title"])
        written = write_topic(entry, mark_scheme, redactions, args.force)
        if written:
            print(f"  wrote {written.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
