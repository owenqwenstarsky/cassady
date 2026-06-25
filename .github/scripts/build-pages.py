#!/usr/bin/env python3
"""Build the docs Markdown files into a static GitHub Pages site.

This intentionally avoids themed site generators and template files. Each Markdown
file is converted to a minimal standalone HTML page, and relative .md links are
rewritten to the generated .html filenames.
"""

from __future__ import annotations

import html
import re
import shutil
from pathlib import Path, PurePosixPath
from urllib.parse import urlsplit, urlunsplit

import markdown

ROOT = Path(__file__).resolve().parents[2]
DOCS_DIR = ROOT / "docs"
SITE_DIR = ROOT / "site"

MARKDOWN_EXTENSIONS = ["fenced_code", "tables", "toc"]
HREF_RE = re.compile(r'href="([^"]+)"')


def output_path(source: Path) -> Path:
    if source.name == "README.md":
        return SITE_DIR / "index.html"
    return SITE_DIR / f"{source.stem}.html"


def page_title(text: str, fallback: str) -> str:
    for line in text.splitlines():
        if line.startswith("# "):
            return line[2:].strip()
    return fallback


def rewrite_markdown_links(rendered: str) -> str:
    def replace(match: re.Match[str]) -> str:
        href = html.unescape(match.group(1))
        parts = urlsplit(href)
        if parts.scheme or parts.netloc or not parts.path.endswith(".md"):
            return match.group(0)

        url_path = PurePosixPath(parts.path)
        if url_path.name == "README.md":
            new_path = str(url_path.with_name("index.html"))
        else:
            new_path = parts.path[:-3] + ".html"

        new_href = urlunsplit(("", "", new_path, parts.query, parts.fragment))
        return f'href="{html.escape(new_href, quote=True)}"'

    return HREF_RE.sub(replace, rendered)


def render_page(source: Path) -> str:
    text = source.read_text(encoding="utf-8")
    title = page_title(text, "Cassady docs")
    body = markdown.markdown(
        text,
        extensions=MARKDOWN_EXTENSIONS,
        output_format="html5",
    )
    body = rewrite_markdown_links(body)

    return "\n".join(
        [
            "<!doctype html>",
            '<html lang="en">',
            "<head>",
            '  <meta charset="utf-8">',
            '  <meta name="viewport" content="width=device-width, initial-scale=1">',
            f"  <title>{html.escape(title)}</title>",
            "</head>",
            "<body>",
            body,
            "</body>",
            "</html>",
            "",
        ]
    )


def copy_static_assets() -> None:
    for item in DOCS_DIR.iterdir():
        if item.suffix == ".md":
            continue
        destination = SITE_DIR / item.name
        if item.is_dir():
            shutil.copytree(item, destination)
        elif item.is_file():
            shutil.copy2(item, destination)


def main() -> None:
    if SITE_DIR.exists():
        shutil.rmtree(SITE_DIR)
    SITE_DIR.mkdir(parents=True)

    for source in sorted(DOCS_DIR.glob("*.md")):
        output_path(source).write_text(render_page(source), encoding="utf-8")

    copy_static_assets()
    (SITE_DIR / ".nojekyll").write_text("", encoding="utf-8")


if __name__ == "__main__":
    main()
