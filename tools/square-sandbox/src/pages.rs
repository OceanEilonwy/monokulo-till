//! The page: plain forms, so it works without JavaScript.

use maud::{DOCTYPE, Markup, html};

use crate::money;
use crate::square::{Location, Payment};

/// What the page shows.
pub struct Index<'a> {
    pub locations: &'a [Location],
    pub location: Option<&'a Location>,
    pub payments: &'a [Payment],
    /// What just happened, or what went wrong.
    pub notice: Option<&'a str>,
    pub error: Option<&'a str>,
    /// The link for another device, when the program listens on the network.
    pub share: Option<&'a Share>,
}

/// The page's link with its key, for another device on the network
/// (`SQUARE_SANDBOX_LISTEN`).
pub struct Share {
    pub link: String,
}

/// [link] as a QR code drawn in the terminal; None if it's too long for one.
pub fn terminal_qr(link: &str) -> Option<String> {
    use qrcode::render::unicode::Dense1x2;
    let code = qrcode::QrCode::new(link.as_bytes()).ok()?;
    Some(
        code.render::<Dense1x2>()
            .dark_color(Dense1x2::Light)
            .light_color(Dense1x2::Dark)
            .quiet_zone(true)
            .build(),
    )
}

/// The QR code and link for another device, shown on this computer's page.
fn share(share: Option<&Share>) -> Markup {
    let Some(share) = share else { return html! {} };
    let svg = qrcode::QrCode::new(share.link.as_bytes()).ok().map(|code| {
        code.render::<qrcode::render::svg::Color>()
            .min_dimensions(200, 200)
            .dark_color(qrcode::render::svg::Color("#23201c"))
            .light_color(qrcode::render::svg::Color("#ffffff"))
            .build()
    });
    html! {
        section.share {
            h2 { "Open on an iPad" }
            p.lede {
                "Scan this with the iPad's camera to ring sales up beside the till. "
                "Keep it to yourself: the link rings up and refunds sales."
            }
            @if let Some(svg) = svg { div.qr { (maud::PreEscaped(svg)) } }
            p { code { (share.link) } }
        }
    }
}

/// Blank item rows on the sale form; empty ones are left out.
const ROWS: usize = 4;

const STYLE: &str = r#"
:root { --ground: #fbfaf7; --surface: #ffffff; --ink: #23201c; --muted: #6b655c; --line: #e4dfd6;
  --accent: #e8590c; --on-accent: #ffffff; --good: #2b7a3d; --bad: #b3261e; color-scheme: light; }
@media (prefers-color-scheme: dark) { :root { --ground: #1b1916; --surface: #24211d; --ink: #f2efe9;
  --muted: #a59e93; --line: #3a362f; --good: #6cc581; --bad: #f2867d; color-scheme: dark; } }
* { box-sizing: border-box; }
body { margin: 0; background: var(--ground); color: var(--ink); font: 16px/1.5 system-ui, sans-serif; }
main { max-width: 52rem; margin: 0 auto; padding: 24px 16px 64px; }
h1 { font-size: 1.5rem; margin: 0 0 4px; } h2 { font-size: 1.1rem; margin: 0 0 12px; }
p.lede { color: var(--muted); margin: 0 0 24px; }
section { background: var(--surface); border: 1px solid var(--line); border-radius: 12px; padding: 16px; margin-bottom: 16px; }
label { display: block; font-size: 0.9rem; color: var(--muted); margin-bottom: 4px; }
input, select { font: inherit; color: inherit; background: var(--ground); border: 1px solid var(--line); border-radius: 8px; padding: 8px 10px; width: 100%; }
.rows { display: grid; grid-template-columns: 1fr 5rem 7rem; gap: 8px; margin-bottom: 12px; }
.inline { display: flex; gap: 8px; align-items: end; flex-wrap: wrap; } .inline > div { flex: 1; min-width: 10rem; }
button { font: inherit; border: 1px solid var(--line); background: var(--surface); color: var(--ink); border-radius: 999px; padding: 8px 16px; cursor: pointer; }
button.primary { background: var(--accent); border-color: var(--accent); color: var(--on-accent); font-weight: 600; }
.notice, .error { border-radius: 8px; padding: 10px 12px; margin-bottom: 16px; border: 1px solid var(--line); }
.notice { color: var(--good); } .error { color: var(--bad); }
table { width: 100%; border-collapse: collapse; font-size: 0.95rem; }
th, td { text-align: left; padding: 8px 6px; border-bottom: 1px solid var(--line); vertical-align: top; }
th { color: var(--muted); font-weight: 500; } td.num { text-align: right; white-space: nowrap; }
code { font-size: 0.85em; color: var(--muted); word-break: break-all; }
.wide { overflow-x: auto; }
.qr svg { display: block; width: 200px; height: 200px; border-radius: 8px; margin-bottom: 8px; }
"#;

/// The page around [body], with what just happened or went wrong.
fn layout(notice: Option<&str>, error: Option<&str>, body: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Square sandbox till" }
                style { (maud::PreEscaped(STYLE)) }
            }
            body { main {
                h1 { "Square sandbox till" }
                p.lede { "Rings up sales in Square's sandbox the way a cashier's \"Other: Monero\" tap would, for a Till till signed in as the same test seller." }
                @if let Some(notice) = notice { p.notice role="status" { (notice) } }
                @if let Some(error) = error { p.error role="alert" { (error) } }
                (body)
            } }
        }
    }
}

/// Before there's a token: where to enter one.
pub fn token(notice: Option<&str>, error: Option<&str>, link: Option<&Share>) -> Markup {
    layout(
        notice,
        error,
        html! {
            section {
                h2 { "Sandbox access token" }
                p.lede {
                    "From the Square Developer Console: your application, then Credentials, then Sandbox. "
                    "Use the test seller the till signed in as. It's kept in this program's memory only, "
                    "until you forget it or stop the program."
                }
                form.inline method="post" action="/token" {
                    div {
                        label for="token" { "Token" }
                        input #token name="token" type="password" autocomplete="off" spellcheck="false" required placeholder="EAAA…";
                    }
                    button.primary type="submit" { "Use it" }
                }
            }
            (share(link))
        },
    )
}

pub fn index(page: &Index) -> Markup {
    layout(
        page.notice,
        page.error,
        html! {
            @if page.locations.is_empty() && page.error.is_none() {
                p.error role="alert" { "The test seller has no active location." }
            }
            @if page.locations.len() > 1 {
                section {
                    form.inline method="get" action="/" {
                        div {
                            label for="location" { "Location" }
                            select #location name="location" {
                                @for l in page.locations {
                                    option value=(l.id) selected[page.location.is_some_and(|c| c.id == l.id)] { (l.name) " (" (l.currency) ")" }
                                }
                            }
                        }
                        button type="submit" { "Show" }
                    }
                }
            }
            @if let Some(location) = page.location {
                (sale_form(location))
                (payments(location, page.payments))
            }
            form method="post" action="/token/forget" {
                button type="submit" { "Forget token" }
            }
            (share(page.share))
        },
    )
}

fn sale_form(location: &Location) -> Markup {
    html! {
        section {
            h2 { "Ring up a sale at " (location.name) }
            form method="post" action="/sales" {
                input type="hidden" name="location" value=(location.id);
                div.rows {
                    label { "Item" } label { "Quantity" } label { "Price (" (location.currency) ")" }
                    @for row in 0..ROWS {
                        @let (name, quantity, price) = match row {
                            0 => ("Flat white", "2", "4.50"),
                            1 => ("Almond croissant", "1", "3.75"),
                            _ => ("", "1", ""),
                        };
                        input name="name" value=(name) aria-label={ "Item " (row + 1) };
                        input name="quantity" value=(quantity) inputmode="numeric" aria-label={ "Quantity " (row + 1) };
                        input name="price" value=(price) inputmode="decimal" aria-label={ "Price " (row + 1) };
                    }
                }
                div.inline {
                    div {
                        label for="tender" { "Tendered as Other:" }
                        input #tender name="tender" value="Monero";
                    }
                    button.primary type="submit" { "Ring it up" }
                }
                p.lede { "Name the tender anything but Monero to check the till leaves the sale alone." }
            }
        }
    }
}

fn payments(location: &Location, payments: &[Payment]) -> Markup {
    html! {
        section {
            h2 { "Latest payments" }
            @if payments.is_empty() {
                p.lede { "None yet at this location." }
            } @else {
                div.wide { table {
                    thead { tr { th { "When (UTC)" } th { "Tender" } th { "Status" } th.num { "Total" } th { "Payment and order" } th {} } }
                    tbody {
                        @for p in payments {
                            tr {
                                td { (p.created_at.get(..19).unwrap_or(&p.created_at).replace('T', " ")) }
                                td { (p.tender()) }
                                td {
                                    (p.status.to_lowercase())
                                    @if let Some(refunded) = &p.refunded_money { ", " (money::show(refunded)) " refunded" }
                                }
                                td.num { (money::show(p.total())) }
                                td {
                                    code { (p.id) }
                                    @if let Some(order) = &p.order_id { br; code { "order " (order) } }
                                }
                                td {
                                    @if p.status == "COMPLETED" && p.refunded_money.is_none() {
                                        form method="post" action={ "/payments/" (p.id) "/refund" } {
                                            input type="hidden" name="location" value=(location.id);
                                            button type="submit" { "Refund" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } }
            }
            form method="get" action="/" {
                input type="hidden" name="location" value=(location.id);
                button type="submit" { "Refresh" }
            }
        }
    }
}
