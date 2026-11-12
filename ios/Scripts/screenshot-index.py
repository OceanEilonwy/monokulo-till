#!/usr/bin/env python3
"""Writes index.html into the screenshots' folder (ios.yml's
till-ios-screenshots artifact): every PNG the tests wrote, one section per
simulator or set, in file order, so the unzipped artifact opens as one page
in a browser.

    ios/Scripts/screenshot-index.py "$RUNNER_TEMP/screenshots"
"""

import html
import os
import sys
from pathlib import Path
from urllib.parse import quote

STYLE = """
  :root { color-scheme: light dark; --ground: #f4f1ea; --ink: #1a1917; --muted: #5a564c; --card: #fff; }
  @media (prefers-color-scheme: dark) { :root { --ground: #111; --ink: #ece8df; --muted: #a8a29a; --card: #1c1c1c; } }
  body { margin: 0; padding: 16px; background: var(--ground); color: var(--ink); font: 15px/1.4 -apple-system, system-ui, sans-serif; }
  h1 { font-size: 22px; margin: 0 0 4px; }
  h2 { font-size: 18px; margin: 32px 0 12px; position: sticky; top: 0; background: var(--ground); padding: 8px 0; }
  p { color: var(--muted); margin: 0 0 8px; }
  nav a { margin-right: 12px; color: inherit; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 16px; }
  figure { margin: 0; background: var(--card); border-radius: 12px; padding: 8px; }
  figure img { width: 100%; height: auto; display: block; border-radius: 6px; }
  figcaption { font-size: 13px; color: var(--muted); margin-top: 6px; word-break: break-word; }
"""


def main() -> None:
    root = Path(sys.argv[1])
    root.mkdir(parents=True, exist_ok=True)
    pngs = sorted(p.relative_to(root) for p in root.rglob("*.png"))
    groups: dict[str, list[Path]] = {}
    for png in pngs:
        groups.setdefault(png.parent.as_posix(), []).append(png)

    out = [
        "<!doctype html>",
        '<html lang="en">',
        "<head>",
        '<meta charset="utf-8">',
        '<meta name="viewport" content="width=device-width, initial-scale=1">',
        "<title>Till screenshots</title>",
        f"<style>{STYLE}</style>",
        "</head>",
        "<body>",
        "<h1>Till screenshots</h1>",
    ]
    ref, sha = os.environ.get("GITHUB_REF_NAME"), os.environ.get("GITHUB_SHA")
    if sha:
        out.append(f"<p>{html.escape(ref or '')} at {html.escape(sha[:12])}</p>")
    if not pngs:
        out.append("<p>The tests wrote no screenshots: see the test results.</p>")
    else:
        out.append("<p>Tap a picture to see it full size.</p>")
        out.append("<nav>")
        for group in groups:
            out.append(f'<a href="#{quote(group)}">{html.escape(group)}</a>')
        out.append("</nav>")
    for group, files in groups.items():
        out.append(f'<h2 id="{quote(group)}">{html.escape(group)}</h2>')
        out.append('<div class="grid">')
        for png in files:
            href = quote(png.as_posix())
            label = html.escape(png.stem)
            out.append(
                f'<figure><a href="{href}"><img loading="lazy" src="{href}" alt="{label}"></a>'
                f"<figcaption>{label}</figcaption></figure>"
            )
        out.append("</div>")
    out += ["</body>", "</html>", ""]
    (root / "index.html").write_text("\n".join(out), encoding="utf-8")
    print(f"index.html: {len(pngs)} screenshots in {len(groups)} sets")


if __name__ == "__main__":
    main()
