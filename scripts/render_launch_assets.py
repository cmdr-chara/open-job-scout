"""Render OpenJobScout's social preview and README terminal demo."""

from __future__ import annotations

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "docs" / "assets"

PAPER = "#EEEDE8"
INK = "#121719"
TERMINAL = "#0C1114"
TERMINAL_BAR = "#171E22"
WHITE = "#F4F3EE"
MUTED = "#97A1A5"
TEAL = "#1B9C8B"
GREEN = "#80C995"
YELLOW = "#D6AE55"


def font(size: int, *, bold: bool = False, mono: bool = False) -> ImageFont.FreeTypeFont:
    """Load a system font with Linux-friendly fallbacks."""
    if mono:
        candidates = [
            Path("C:/Windows/Fonts/consolab.ttf" if bold else "C:/Windows/Fonts/consola.ttf"),
            Path(
                "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf"
                if bold
                else "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"
            ),
        ]
    else:
        candidates = [
            Path("C:/Windows/Fonts/segoeuib.ttf" if bold else "C:/Windows/Fonts/segoeui.ttf"),
            Path(
                "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"
                if bold
                else "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"
            ),
        ]
    for candidate in candidates:
        if candidate.exists():
            return ImageFont.truetype(str(candidate), size=size)
    raise FileNotFoundError("No suitable UI font was found")


def draw_terminal(
    image: Image.Image,
    box: tuple[int, int, int, int],
    lines: list[tuple[str, str]],
    *,
    line_height: int = 27,
    font_size: int = 18,
) -> None:
    """Draw a plain terminal panel with exact output."""
    draw = ImageDraw.Draw(image)
    left, top, right, bottom = box
    draw.rectangle(box, fill=TERMINAL, outline=INK, width=2)
    draw.rectangle((left + 1, top + 1, right - 1, top + 44), fill=TERMINAL_BAR)
    draw.text((left + 18, top + 12), "openjobscout / local", font=font(15, mono=True), fill=MUTED)

    mono_font = font(font_size, mono=True)
    y = top + 67
    for text, color in lines:
        if y + line_height > bottom - 12:
            break
        draw.text((left + 22, y), text, font=mono_font, fill=color)
        y += line_height


def render_social_preview() -> Path:
    """Render the 1280x640 GitHub social preview."""
    image = Image.new("RGB", (1280, 640), PAPER)
    draw = ImageDraw.Draw(image)

    draw.rectangle((0, 0, 18, 640), fill=TEAL)
    draw.text((64, 48), "OpenJobScout", font=font(38, bold=True), fill=INK)
    draw.text((350, 63), "v0.1.0", font=font(15, mono=True), fill=TEAL)
    draw.text((66, 94), "LOCAL JOB DISCOVERY AND TRACKING", font=font(14, mono=True), fill=TEAL)
    draw.line((64, 128, 653, 128), fill="#B8BCB9", width=2)

    draw.text((64, 166), "Search, verify,", font=font(54, bold=True), fill=INK)
    draw.text((64, 230), "rank, and track.", font=font(54, bold=True), fill=INK)

    labels = [
        ("discover", "JobSpy or CSV"),
        ("review", "live-link checks + readable scores"),
        ("track", "SQLite + Markdown reports"),
        ("submit", "never"),
    ]
    y = 326
    for label, value in labels:
        draw.text((67, y), label.ljust(9), font=font(19, bold=True, mono=True), fill=TEAL)
        draw.text((190, y), value, font=font(19, mono=True), fill=INK)
        y += 39

    draw.text(
        (66, 560),
        "python 3.11+  /  MIT  /  github.com/cmdr-chara/open-job-scout",
        font=font(15, mono=True),
        fill="#4F5859",
    )

    terminal_lines = [
        ("$ jobscout import-csv jobs.csv", TEAL),
        ("Received: 2", WHITE),
        ("Accepted: 1  Filtered out: 1", MUTED),
        ("", WHITE),
        ("$ jobscout list --status new", TEAL),
        ("ID          SCORE  STATUS  ROLE", MUTED),
        ("425a56c785   69.0  new     Junior", WHITE),
        ("", WHITE),
        ("$ jobscout show 425a56c785", TEAL),
        ("[ok] backend · python · junior", GREEN),
        ("[ok] fully remote · salary found", GREEN),
    ]
    draw_terminal(image, (700, 56, 1224, 574), terminal_lines, line_height=31, font_size=17)

    output = ASSETS / "openjobscout-social-preview.png"
    output.parent.mkdir(parents=True, exist_ok=True)
    image.save(output, format="PNG", optimize=True)
    return output


def demo_lines(stage: int) -> list[tuple[str, str]]:
    """Return exact terminal lines for an animation stage."""
    install = [
        ("$ uv tool install git+https://github.com/cmdr-chara/", TEAL),
        ("  open-job-scout.git@v0.1.0", TEAL),
    ]
    imported = [
        ("$ jobscout import-csv jobs.csv --no-verify", TEAL),
        ("Received: 2", WHITE),
        ("Unique valid jobs: 2", WHITE),
        ("Accepted: 1   Filtered out: 1", GREEN),
    ]
    listed = [
        ("$ jobscout list --status new", TEAL),
        ("ID          SCORE  STATUS  ROLE", MUTED),
        ("425a56c785   69.0  new     Junior Python Backend", WHITE),
    ]
    shown = [
        ("$ jobscout show 425a56c785", TEAL),
        ("", WHITE),
        ("Junior Python Backend Engineer", WHITE),
        ("Example Labs · remote · 40k–50k EUR", MUTED),
        ("", WHITE),
        ("WHY 69.0", YELLOW),
        ("[ok] title: backend, python", GREEN),
        ("[ok] skills: FastAPI, PostgreSQL, Docker", GREEN),
        ("[ok] early-career signal: junior", GREEN),
        ("[ok] fully remote · published salary", GREEN),
    ]
    scenes = [
        install,
        install + [("Installed 1 executable: jobscout", GREEN)],
        [("$ jobscout import-csv jobs.csv --no-verify", TEAL)],
        imported,
        imported + [("", WHITE), ("$ jobscout list --status new", TEAL)],
        listed,
        listed + [("", WHITE), ("$ jobscout show 425a56c785", TEAL)],
        shown,
    ]
    return scenes[stage]


def render_demo() -> Path:
    """Render an animated terminal walkthrough for the README."""
    frames: list[Image.Image] = []
    durations: list[int] = []
    for stage in range(8):
        image = Image.new("RGB", (1000, 563), PAPER)
        draw = ImageDraw.Draw(image)
        draw.rectangle((0, 0, 12, 563), fill=TEAL)
        draw.text((42, 26), "OpenJobScout", font=font(27, bold=True), fill=INK)
        draw.text((765, 37), "bundled sample data", font=font(13, mono=True), fill=TEAL)
        draw_terminal(image, (42, 86, 958, 525), demo_lines(stage), line_height=25, font_size=16)
        frames.append(image)
        durations.append(850 if stage not in {1, 3, 5, 7} else 1500)

    output = ASSETS / "openjobscout-demo.gif"
    output.parent.mkdir(parents=True, exist_ok=True)
    frames[0].save(
        output,
        format="GIF",
        save_all=True,
        append_images=frames[1:],
        duration=durations,
        loop=0,
        optimize=True,
        disposal=2,
    )
    return output


def main() -> None:
    """Render all launch assets."""
    for path in (render_social_preview(), render_demo()):
        print(path)


if __name__ == "__main__":
    main()
