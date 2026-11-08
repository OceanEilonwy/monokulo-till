"""Builds the receipt QR scan test page from out/report.json and the SVGs.
Usage: python3 -I page.py out out/receipt-qr-test.html"""
import html
import json
import re
import sys
from pathlib import Path

out = Path(sys.argv[1])
dest = Path(sys.argv[2])
report = json.loads((out / "report.json").read_text())
CATS = ["hand", "restaurant", "grocery", "hardware", "clothing", "pharmacy"]
CAT_NAMES = {"hand": "Hand-written (5)", "restaurant": "Restaurant", "grocery": "Grocery",
             "hardware": "Hardware", "clothing": "Clothing", "pharmacy": "Pharmacy"}
LICENCE = {
    "food only": "ODbL",
    "open": "Public domain, CC0, ours",
    "open + food": "+ ODbL",
    "all": "+ Amazon (terms to confirm)",
}


def svg(name: str, label: str) -> str:
    s = (out / name).read_text()
    s = re.sub(r"^<\?xml[^>]*\?>", "", s)
    s = re.sub(r'width="\d+" height="\d+" ', "", s, count=1)
    return s.replace("<svg ", f'<svg role="img" aria-label="{html.escape(label)}" class="qr" ', 1)


def money(cents: int) -> str:
    return f"${cents / 100:,.2f}"


def receipt_html(r) -> str:
    rows = "".join(
        f"<tr><td>{html.escape(n)}</td><td class=n>{q}</td><td class=n>{money(q * c)}</td></tr>"
        for n, q, c in r["items"]
    )
    tax = f"<tr><td>Tax</td><td></td><td class=n>{money(r['tax'])}</td></tr>" if r["tax"] else ""
    tip = f"<tr><td>Tip</td><td></td><td class=n>{money(r['tip'])}</td></tr>" if r["tip"] else ""
    return f"""<div class="paper">
  <div class="paper-head"><strong>{html.escape(r['store'])}</strong><span>{html.escape(r['ref'])}</span></div>
  <div class="paper-scroll"><table>
    <tbody>{rows}</tbody>
    <tfoot>
      <tr><td>Subtotal</td><td></td><td class=n>{money(r['subtotal'])}</td></tr>{tax}{tip}
      <tr class="tot"><td>Total</td><td></td><td class=n>{money(r['total'])}</td></tr>
      <tr><td>Paid</td><td></td><td class=n>{r['xmr']} XMR</td></tr>
    </tfoot>
  </table></div>
</div>"""


def band(version: int) -> tuple[str, str]:
    if version <= 13:
        return "easy", "Easy to scan"
    if version <= 18:
        return "ok", "Fine in good light"
    return "hard", "Dense: test carefully"


def qr_card(d, title: str, label: str, extra: str = "") -> str:
    cls, words = band(d["qr_version_m"])
    return f"""<figure class="qr-card">
  <figcaption><span class="kind">{title}</span><span class="pill {cls}">{words}</span></figcaption>
  <div class="qr-wrap">{svg(d['svg'], label)}</div>
  <dl>
    <div><dt>QR version</dt><dd>{d['qr_version_m']} <small>(M)</small> · {d['qr_version_l']} <small>(L)</small></dd></div>
    <div><dt>Grid</dt><dd>{d['modules']}×{d['modules']}</dd></div>
    {extra}
  </dl>
</figure>"""


results = report["results"]
best = next(r for r in results if r["label"] == report["chosen"])
before = min((r for r in results if r["variant"] == "food only"), key=lambda r: r["mean_version_m"])

# Per category: the best dictionary against the previous (food-only) one.
cat_rows = "".join(
    f"<tr><td>{CAT_NAMES[c]}</td>"
    f"<td class=n>{before['categories'][c]['median_m']}</td>"
    f"<td class=n><strong>{best['categories'][c]['median_m']}</strong></td>"
    f"<td class=n>{best['categories'][c]['p90_m']}</td><td class=n>{best['categories'][c]['max_m']}</td>"
    f"<td class=n>{best['categories'][c]['median_l']}</td>"
    f"<td class=n>{best['categories'][c]['median_bytes']} B</td></tr>"
    for c in CATS
)

# Every variant and size: median version per category.
var_rows = "".join(
    f"<tr{' class=best' if r['label'] == best['label'] else ''}><td>{html.escape(r['variant'])}</td>"
    f"<td class=n>{r['dict_bytes'] // 1024} KB</td><td>{LICENCE[r['variant']]}</td>"
    + "".join(f"<td class=n>{r['categories'][c]['median_m']}</td>" for c in CATS)
    + f"<td class=n>{r['mean_version_m']:.2f}</td></tr>"
    for r in results
)
var_head = "".join(f"<th class=n>{CAT_NAMES[c].split(' ')[0]}</th>" for c in CATS)

sections = []
for s in report["shown"]:
    a = s["after"]
    extra = (
        f"<div><dt>Receipt data</dt><dd>{a['payload_bytes']} B</dd></div>"
        f"<div><dt>Whole URI</dt><dd>{a['uri_chars']} chars</dd></div>"
    )
    r = s["receipt"]
    cards = ""
    if s["before"]:
        b = s["before"]
        cards += qr_card(b, "Before", f"{r['store']} receipt, first encoding, version {b['qr_version_m']}")
    cards += qr_card(a, "After" if s["before"] else "Typical receipt",
                     f"{r['store']} receipt, compact encoding, version {a['qr_version_m']}", extra)
    title = r["store"] if s["category"] == "hand" else f"{CAT_NAMES[s['category']]}: median test receipt"
    sub = (f"{len(r['items'])} items · version {s['before']['qr_version_m']} before, {a['qr_version_m']} after"
           if s["before"] else f"{len(r['items'])} items, names from the held-out test set · version {a['qr_version_m']}")
    sections.append(f"""<section class="case">
  <header class="case-head"><h2>{html.escape(title)}</h2><p>{sub}</p></header>
  <div class="case-body">{receipt_html(r)}{cards}
    <details class="uri"><summary>URI in the code</summary><code>{html.escape(a['uri'])}</code></details>
  </div>
</section>""")

train_counts = report["training"]
test_counts = report["test"]
base = report["baseline"]

page = f"""<title>Receipt QR Scan Test</title>
<meta name="claude:viewport-fit-sides" content="cover">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Manrope:wght@500;700;800&family=JetBrains+Mono:wght@400;600&display=swap">
<style>
/* A test sheet: results tables first, then receipts with their QR codes. */
:root {{
  --bg: #f4f1ea; --surface: #ffffff; --ink: #1a1917; --muted: #5a564c; --line: #dcd5c7;
  --accent-text: #b34700; --best: #e2f0e5;
  --ok: #1a7f37; --ok-bg: #e2f0e5; --warn: #7d5e00; --warn-bg: #f3ead2; --bad: #b00020; --bad-bg: #f6dfe2;
  --paper: #fffdf8; --qr-bg: #ffffff;
  --font-body: Manrope, system-ui, sans-serif; --font-mono: "JetBrains Mono", ui-monospace, Menlo, monospace;
  --qr: 300px;
}}
@media (prefers-color-scheme: dark) {{ :root:not([data-theme="light"]) {{
  --bg: #1e1e1e; --surface: #2a2a2a; --ink: #d4d4d4; --muted: #9d9d9d; --line: #3d3d3d; --accent-text: #ff8a3d; --best: #1f3326;
  --ok: #4cc27a; --ok-bg: #1f3326; --warn: #e2b93b; --warn-bg: #3a3220; --bad: #ff6b7d; --bad-bg: #3d2226;
  --paper: #262626; color-scheme: dark; }} }}
:root[data-theme="dark"] {{
  --bg: #1e1e1e; --surface: #2a2a2a; --ink: #d4d4d4; --muted: #9d9d9d; --line: #3d3d3d; --accent-text: #ff8a3d; --best: #1f3326;
  --ok: #4cc27a; --ok-bg: #1f3326; --warn: #e2b93b; --warn-bg: #3a3220; --bad: #ff6b7d; --bad-bg: #3d2226;
  --paper: #262626; color-scheme: dark; }}
body {{ background: var(--bg); color: var(--ink); font-family: var(--font-body); font-size: 16px; line-height: 1.5;
  padding-left: max(16px, env(safe-area-inset-left, 0px)); padding-right: max(16px, env(safe-area-inset-right, 0px)); padding-block: 32px 64px; }}
.wrap {{ max-width: 1180px; margin: 0 auto; display: flex; flex-direction: column; gap: 36px; }}
h1, h2 {{ text-wrap: balance; margin: 0; letter-spacing: -0.02em; font-weight: 800; }}
h1 {{ font-size: clamp(28px, 4vw, 40px); }}
h2 {{ font-size: 22px; }}
p {{ margin: 0; max-width: 72ch; }}
.muted {{ color: var(--muted); }}
.intro {{ display: flex; flex-direction: column; gap: 12px; }}
.warn {{ border: 1px solid var(--line); background: var(--surface); border-radius: 10px; padding: 12px 16px; font-size: 15px; }}
.warn strong {{ color: var(--accent-text); }}
.controls {{ display: flex; flex-wrap: wrap; align-items: center; gap: 12px; font-size: 15px; }}
.controls input {{ accent-color: var(--accent-text); width: min(320px, 60vw); }}
.controls output {{ font-family: var(--font-mono); font-variant-numeric: tabular-nums; }}
.block {{ display: flex; flex-direction: column; gap: 10px; }}
.table-box {{ overflow-x: auto; background: var(--surface); border: 1px solid var(--line); border-radius: 10px; }}
table {{ border-collapse: collapse; width: 100%; font-size: 15px; }}
th, td {{ text-align: left; padding: 8px 12px; border-bottom: 1px solid var(--line); vertical-align: top; }}
th {{ font-size: 12px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); font-weight: 700; }}
td.n, th.n {{ text-align: right; font-variant-numeric: tabular-nums; font-family: var(--font-mono); font-size: 14px; white-space: nowrap; }}
tr:last-child td {{ border-bottom: 0; }}
tr.best td {{ background: var(--best); font-weight: 700; }}
.case {{ display: flex; flex-direction: column; gap: 16px; padding-top: 24px; border-top: 1px solid var(--line); }}
.case-head p {{ color: var(--muted); font-size: 15px; }}
.case-body {{ display: grid; grid-template-columns: minmax(240px, 1fr) auto auto; gap: 20px; align-items: start; }}
@media (max-width: 1100px) {{ .case-body {{ grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); }} .paper {{ grid-column: 1 / -1; }} }}
@media (max-width: 700px) {{ .case-body {{ grid-template-columns: minmax(0, 1fr); }} }}
.uri {{ grid-column: 1 / -1; font-size: 14px; min-width: 0; }}
.uri summary {{ cursor: pointer; color: var(--accent-text); font-weight: 700; }}
.uri code {{ display: block; margin-top: 8px; font-family: var(--font-mono); font-size: 12px; word-break: break-all; color: var(--muted); }}
.paper {{ background: var(--paper); border: 1px solid var(--line); border-radius: 4px; padding: 16px; font-family: var(--font-mono); font-size: 13px; min-width: 0; }}
.paper-head {{ display: flex; justify-content: space-between; gap: 8px; padding-bottom: 8px; border-bottom: 1px dashed var(--line); margin-bottom: 4px; }}
.paper-scroll {{ max-height: 360px; overflow: auto; }}
.paper table {{ font-size: 13px; }}
.paper td {{ padding: 3px 4px; border: 0; }}
.paper tfoot td {{ border-top: 1px dashed var(--line); }}
.paper tfoot tr + tr td {{ border-top: 0; }}
.paper .tot td {{ font-weight: 600; }}
.qr-card {{ margin: 0; display: flex; flex-direction: column; gap: 10px; min-width: 0; }}
.qr-card figcaption {{ display: flex; justify-content: space-between; align-items: center; gap: 8px; }}
.kind {{ font-weight: 700; }}
.pill {{ font-size: 12px; font-weight: 700; padding: 3px 10px; border-radius: 999px; white-space: nowrap; }}
.pill.easy {{ color: var(--ok); background: var(--ok-bg); }}
.pill.ok {{ color: var(--warn); background: var(--warn-bg); }}
.pill.hard {{ color: var(--bad); background: var(--bad-bg); }}
.qr-wrap {{ background: var(--qr-bg); border-radius: 14px; padding: 6px; width: var(--qr); max-width: 100%; box-sizing: border-box; }}
.qr {{ display: block; width: 100%; height: auto; }}
dl {{ margin: 0; display: grid; grid-template-columns: 1fr 1fr; gap: 6px 16px; font-size: 14px; max-width: var(--qr); }}
dl div {{ display: flex; flex-direction: column; }}
dt {{ font-size: 12px; color: var(--muted); }}
dd {{ margin: 0; font-family: var(--font-mono); font-variant-numeric: tabular-nums; }}
small {{ color: var(--muted); }}
:focus-visible {{ outline: 2px solid var(--accent-text); outline-offset: 2px; }}
</style>

<div class="wrap">
  <header class="intro">
    <h1>Receipt QR scan test</h1>
    <p class="muted">Each code is a real Monero payment URI with the whole receipt as an Open Receipt Format document in one extra parameter, <code>orf=</code>, encoded by the <code>orf-codec</code> library: bit-packed numbers, text compressed with zstd and its shipped dictionary, written as digits. A payment-only code is version {base['qr_version_m']}. To check a scan, copy the URI your phone shows and run <code>cargo run --release -p receipt-qr -- decode '&lt;URI&gt;'</code>.</p>
    <p class="muted">Test receipts use only names held out of training: restaurant dishes from NYPL menus, grocery from Open Food Facts, and hardware, clothing and pharmacy from Amazon product titles cut to till length. {report['receipts_per_category']} receipts per category, 1 to 25 items, mostly small baskets.</p>
    <p class="warn"><strong>Don't pay any of these.</strong> The address is a made-up placeholder and the amounts are examples. A plain camera app shows the URI text.</p>
    <div class="controls">
      <label for="qr-size">Code size on screen</label>
      <input id="qr-size" type="range" min="180" max="440" step="10" value="300">
      <output id="qr-size-out" for="qr-size">300 px</output>
      <span class="muted">About 280 px matches the Till full-screen code on an iPhone.</span>
    </div>
  </header>

  <section class="block">
    <h2>Per category, chosen dictionary ({html.escape(best['label'])})</h2>
    <div class="table-box"><table>
      <thead><tr><th>Category</th><th class=n>Median, food-only dict</th><th class=n>Median</th><th class=n>90th pct</th><th class=n>Worst</th><th class=n>Median at L</th><th class=n>Median data</th></tr></thead>
      <tbody>{cat_rows}</tbody>
    </table></div>
    <p class="muted">QR versions at error correction M unless marked L. "Food-only dict" is the previous dictionary, trained on Open Food Facts alone.</p>
  </section>

  <section class="block">
    <h2>Every training mix and size</h2>
    <div class="table-box"><table>
      <thead><tr><th>Training mix</th><th class=n>Size</th><th>Licence</th>{var_head}<th class=n>Mean</th></tr></thead>
      <tbody>{var_rows}</tbody>
    </table></div>
    <p class="muted">Median QR version per category; the mean is over all {sum(best['categories'][c]['count'] for c in CATS)} test receipts. Training names: {', '.join(f'{k} {v:,}' for k, v in train_counts.items())}. Held-out test names: {', '.join(f'{k} {v:,}' for k, v in test_counts.items())}.</p>
  </section>

  {''.join(sections)}
</div>

<script>
(() => {{
  const input = document.getElementById('qr-size');
  const out = document.getElementById('qr-size-out');
  const apply = v => {{ document.documentElement.style.setProperty('--qr', v + 'px'); out.textContent = v + ' px'; }};
  try {{ const saved = localStorage.getItem('qr-size'); if (saved) {{ input.value = saved; }} }} catch (e) {{}}
  apply(input.value);
  input.addEventListener('input', () => {{ apply(input.value); try {{ localStorage.setItem('qr-size', input.value); }} catch (e) {{}} }});
}})();
</script>
"""
dest.write_text(page)
print(dest, len(page))
