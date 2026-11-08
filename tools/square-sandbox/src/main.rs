//! A local web page for ringing up "Other: Monero" sales in Square's
//! sandbox, which has no Point of Sale app, for a Till till signed in
//! to the same test seller. It makes what the till looks for
//! (`docs/decisions/android.md` 13): an order with line items, paid by a
//! completed `EXTERNAL` payment whose source is the tender's name.
//!
//! ```sh
//! cargo run -p square-sandbox   # then open http://127.0.0.1:8739
//! ```
//!
//! The page asks for the sandbox access token (the Developer Console's
//! Credentials page) and keeps it in memory only; `SQUARE_SANDBOX_TOKEN`
//! gives it at start instead. `SQUARE_SANDBOX_PORT` picks the port. It
//! listens on 127.0.0.1 only, and answers only requests for that address
//! and forms posted from its own page.

mod pages;
mod square;

use std::sync::Arc;

use axum::Router;
use axum::extract::{Form, Path, Query, Request, State};
use axum::http::header::{HOST, ORIGIN};
use axum::http::{Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use maud::Markup;
use serde::Deserialize;

use square::{Item, Location, Square};

const SANDBOX: &str = "https://connect.squareupsandbox.com";

/// Amounts in a currency's smallest unit, as Square keeps them.
mod money {
    use crate::square::Money;

    /// Digits after the point: none for the currencies Square counts in whole units.
    pub fn decimals(currency: &str) -> u32 {
        match currency {
            "JPY" | "KRW" | "VND" | "CLP" | "ISK" => 0,
            _ => 2,
        }
    }

    /// "12.75 AUD".
    pub fn show(money: &Money) -> String {
        let places = decimals(&money.currency);
        if places == 0 {
            return format!("{} {}", money.amount, money.currency);
        }
        let unit = 10i64.pow(places);
        let sign = if money.amount < 0 { "-" } else { "" };
        let amount = money.amount.abs();
        format!(
            "{sign}{}.{:0width$} {}",
            amount / unit,
            amount % unit,
            money.currency,
            width = places as usize
        )
    }

    /// "4.5" in AUD is 450; None for anything but a plain, non-negative amount.
    pub fn parse(text: &str, currency: &str) -> Option<i64> {
        let places = decimals(currency) as usize;
        let (whole, frac) = text.trim().split_once('.').unwrap_or((text.trim(), ""));
        let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
        if whole.is_empty() || !digits(whole) || !digits(frac) || frac.len() > places {
            return None;
        }
        let whole: i64 = whole.parse().ok()?;
        let frac: i64 = if places == 0 {
            0
        } else {
            format!("{frac:0<places$}").parse().ok()?
        };
        whole
            .checked_mul(10i64.pow(places as u32))?
            .checked_add(frac)
    }
}

struct App {
    square: Square,
    /// The port it listens on, which its own pages' requests name.
    port: u16,
}

type Shared = State<Arc<App>>;

fn router(app: App) -> Router {
    let app = Arc::new(app);
    Router::new()
        .route("/", get(index))
        .route("/token", post(token))
        .route("/token/forget", post(forget))
        .route("/sales", post(sale))
        .route("/payments/{id}/refund", post(refund))
        .layer(middleware::from_fn_with_state(app.clone(), same_site))
        .with_state(app)
}

/// Only requests for this machine's address, so another site's name
/// pointed at 127.0.0.1 can't read the page, and only forms posted from
/// the page itself, so another site can't replace the token or ring up a
/// sale. A client that names no origin (curl) is taken at its word.
async fn same_site(State(app): Shared, request: Request, next: Next) -> Response {
    match refusal(&request, app.port) {
        Some(why) => (StatusCode::FORBIDDEN, why).into_response(),
        None => next.run(request).await,
    }
}

/// Why [same_site] turns [request] away, if it does.
fn refusal(request: &Request, port: u16) -> Option<&'static str> {
    let header = |name: &str| request.headers().get(name).and_then(|v| v.to_str().ok());
    let host = header(HOST.as_str()).unwrap_or_default();
    let ours = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    if !ours.iter().any(|o| o == host) {
        return Some("Open this page at http://127.0.0.1 with its port.");
    }
    if request.method() == Method::GET {
        return None;
    }
    let from_here = match header(ORIGIN.as_str()) {
        Some(origin) => origin == format!("http://{host}"),
        None => header("sec-fetch-site").is_none_or(|site| site == "same-origin" || site == "none"),
    };
    (!from_here).then_some("Only this page's own forms may change anything.")
}

#[derive(Deserialize)]
struct Shown {
    location: Option<String>,
    notice: Option<String>,
    error: Option<String>,
}

async fn index(State(app): Shared, Query(shown): Query<Shown>) -> Markup {
    if !app.square.has_token() {
        return pages::token(shown.notice.as_deref(), shown.error.as_deref());
    }
    let mut error = shown.error;
    let locations = app.square.locations().await.unwrap_or_else(|e| {
        error.get_or_insert(e.to_string());
        Vec::new()
    });
    let location = shown
        .location
        .as_deref()
        .and_then(|id| locations.iter().find(|l| l.id == id))
        .or(locations.first());
    let payments = match location {
        Some(l) => app.square.payments(&l.id).await.unwrap_or_else(|e| {
            error.get_or_insert(e.to_string());
            Vec::new()
        }),
        None => Vec::new(),
    };
    pages::index(&pages::Index {
        locations: &locations,
        location,
        payments: &payments,
        notice: shown.notice.as_deref(),
        error: error.as_deref(),
    })
}

/// Back to the page, at [location], saying [notice] or [error].
fn back(location: &str, said: Result<String, String>) -> Response {
    let (key, text) = match &said {
        Ok(notice) => ("notice", notice),
        Err(error) => ("error", error),
    };
    let query = match location {
        "" => serde_urlencoded::to_string([(key, text)]),
        location => serde_urlencoded::to_string([("location", location), (key, text)]),
    }
    .unwrap_or_default();
    Redirect::to(&format!("/?{query}")).into_response()
}

/// One of the sale form's columns, in order.
fn column<'a>(fields: &'a [(String, String)], key: &'a str) -> impl Iterator<Item = &'a str> {
    fields
        .iter()
        .filter(move |(k, _)| k == key)
        .map(|(_, v)| v.trim())
}

/// The sale form's fields, in order: a row's name, quantity and price arrive together.
fn items(fields: &[(String, String)], currency: &str) -> Result<Vec<Item>, String> {
    let rows = column(fields, "name")
        .zip(column(fields, "quantity"))
        .zip(column(fields, "price"));
    let mut items = Vec::new();
    for ((name, quantity), price) in rows {
        if name.is_empty() {
            continue;
        }
        let quantity = quantity
            .parse::<u32>()
            .ok()
            .filter(|q| *q > 0)
            .ok_or_else(|| format!("{name}: the quantity must be a whole number above 0."))?;
        let price = money::parse(price, currency).ok_or_else(|| {
            format!("{name}: the price must be an amount in {currency}, like 4.50.")
        })?;
        items.push(Item {
            name: name.to_owned(),
            quantity,
            price,
        });
    }
    if items.is_empty() {
        return Err("Add at least one item.".into());
    }
    if items
        .iter()
        .map(|i| i.price * i64::from(i.quantity))
        .sum::<i64>()
        <= 0
    {
        return Err("The sale must come to more than nothing.".into());
    }
    Ok(items)
}

async fn location(app: &App, id: &str) -> Result<Location, String> {
    let locations = app.square.locations().await.map_err(|e| e.to_string())?;
    locations
        .into_iter()
        .find(|l| l.id == id)
        .ok_or_else(|| "That location isn't one of the test seller's active ones.".into())
}

async fn sale(State(app): Shared, Form(fields): Form<Vec<(String, String)>>) -> Response {
    let field = |key: &str| {
        fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.trim())
            .unwrap_or("")
    };
    let location_id = field("location");
    let tender = match field("tender") {
        "" => "Monero",
        named => named,
    };
    let said = async {
        let location = location(&app, location_id).await?;
        let items = items(&fields, &location.currency)?;
        let payment = app
            .square
            .sale(&location, &items, tender)
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!(
            "Rang up {} as Other: {tender} ({}).",
            money::show(payment.total()),
            payment.id
        ))
    }
    .await;
    back(location_id, said)
}

#[derive(Deserialize)]
struct TokenForm {
    token: String,
}

async fn token(State(app): Shared, Form(form): Form<TokenForm>) -> Response {
    let said = match form.token.trim() {
        "" => Err("Enter the token.".to_owned()),
        token => match app.square.use_token(token.to_owned()).await {
            Ok(()) => Ok("Signed in to the sandbox.".to_owned()),
            Err(e) => Err(format!("That token didn't work. {e}")),
        },
    };
    back("", said)
}

async fn forget(State(app): Shared) -> Response {
    app.square.set_token(None);
    back("", Ok("Forgot the token.".to_owned()))
}

#[derive(Deserialize)]
struct RefundForm {
    location: String,
}

async fn refund(
    State(app): Shared,
    Path(id): Path<String>,
    Form(form): Form<RefundForm>,
) -> Response {
    let said = async {
        // Square's ids are letters, digits, - and _: anything else isn't one.
        if id.is_empty()
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err("That isn't a Square payment id.".to_owned());
        }
        let payment = app.square.payment(&id).await.map_err(|e| e.to_string())?;
        app.square
            .refund(&payment)
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("Refunded {} ({id}).", money::show(payment.total())))
    }
    .await;
    back(&form.location, said)
}

#[tokio::main]
async fn main() {
    let token = std::env::var("SQUARE_SANDBOX_TOKEN")
        .ok()
        .filter(|t| !t.trim().is_empty());
    let port: u16 = match std::env::var("SQUARE_SANDBOX_PORT") {
        Ok(port) => port.parse().unwrap_or_else(|_| {
            eprintln!("SQUARE_SANDBOX_PORT must be a port number, not {port:?}.");
            std::process::exit(2);
        }),
        Err(_) => 8739,
    };
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Couldn't listen on 127.0.0.1:{port}: {e}");
            std::process::exit(1);
        });
    println!("Square sandbox till on http://127.0.0.1:{port}");
    let app = router(App {
        square: Square::new(SANDBOX, token),
        port,
    });
    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("The server stopped: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Json;
    use serde_json::{Value, json};
    use std::sync::Mutex;

    #[test]
    fn amounts_are_read_and_written_in_the_currency_s_smallest_unit() {
        assert_eq!(money::parse("4.5", "AUD"), Some(450));
        assert_eq!(money::parse("4.50", "USD"), Some(450));
        assert_eq!(money::parse("12", "AUD"), Some(1200));
        assert_eq!(money::parse("500", "JPY"), Some(500));
        for bad in ["", "4.505", "-1", "1e3", "4,50", ".5"] {
            assert_eq!(money::parse(bad, "AUD"), None, "{bad:?}");
        }
        assert_eq!(money::parse("5.5", "JPY"), None);
        let show = |amount, currency: &str| {
            money::show(&square::Money {
                amount,
                currency: currency.into(),
            })
        };
        assert_eq!(show(1275, "AUD"), "12.75 AUD");
        assert_eq!(show(5, "USD"), "0.05 USD");
        assert_eq!(show(500, "JPY"), "500 JPY");
    }

    fn fields(rows: &[(&str, &str, &str)]) -> Vec<(String, String)> {
        rows.iter()
            .flat_map(|(n, q, p)| [("name", *n), ("quantity", *q), ("price", *p)])
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect()
    }

    #[test]
    fn the_form_s_rows_become_items_and_blank_rows_are_left_out() {
        let rows = fields(&[
            ("Flat white", "2", "4.50"),
            ("", "1", ""),
            ("Croissant", "1", "3.75"),
        ]);
        assert_eq!(
            items(&rows, "AUD").unwrap(),
            vec![
                Item {
                    name: "Flat white".into(),
                    quantity: 2,
                    price: 450
                },
                Item {
                    name: "Croissant".into(),
                    quantity: 1,
                    price: 375
                },
            ]
        );
        assert!(
            items(&fields(&[("", "1", "")]), "AUD")
                .unwrap_err()
                .contains("at least one")
        );
        assert!(
            items(&fields(&[("Tea", "0", "3")]), "AUD")
                .unwrap_err()
                .contains("quantity")
        );
        assert!(
            items(&fields(&[("Tea", "1", "three")]), "AUD")
                .unwrap_err()
                .contains("price")
        );
        assert!(
            items(&fields(&[("Free", "1", "0")]), "AUD")
                .unwrap_err()
                .contains("more than nothing")
        );
    }

    type Seen = Arc<Mutex<Vec<(String, Value)>>>;

    /// A stand-in Square: one AUD location, and the orders and payments it's sent.
    async fn stand_in() -> (String, Seen) {
        let seen: Seen = Arc::default();
        let record = |path: &'static str, answer: fn(&Value) -> Value| {
            let seen = seen.clone();
            move |Json(body): Json<Value>| async move {
                let reply = answer(&body);
                seen.lock().unwrap().push((path.to_owned(), body));
                Json(reply)
            }
        };
        let square = Router::new()
            .route(
                "/v2/locations",
                get(|| async { Json(json!({"locations": [{"id": "L1", "name": "Front", "currency": "AUD", "status": "ACTIVE"}]})) }),
            )
            .route(
                "/v2/orders",
                post(record("/v2/orders", |_| json!({"order": {"id": "O1", "total_money": {"amount": 1275, "currency": "AUD"}}}))),
            )
            .route(
                "/v2/payments",
                post(record("/v2/payments", |body| {
                    json!({"payment": {"id": "P1", "status": "COMPLETED", "created_at": "2026-10-26T11:00:00.000Z",
                        "amount_money": body["amount_money"], "source_type": "EXTERNAL",
                        "external_details": {"source": body["external_details"]["source"]}, "order_id": body["order_id"]}})
                }))
                .get(|| async { Json(json!({"payments": []})) }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, square).await.unwrap() });
        (base, seen)
    }

    async fn serve(base: String, token: Option<&str>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{port}");
        let app = router(App {
            square: Square::new(base, token.map(str::to_owned)),
            port,
        });
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        url
    }

    #[tokio::test]
    async fn ringing_up_a_sale_over_http_makes_an_order_and_pays_it_with_the_named_other_tender() {
        let (square, seen) = stand_in().await;
        let page = serve(square, Some("EAAA-test")).await;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let form = "location=L1&name=Flat+white&quantity=2&price=4.50&name=&quantity=1&price=\
                    &name=Almond+croissant&quantity=1&price=3.75&tender=Monero";
        let answer = client
            .post(format!("{page}/sales"))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form)
            .send()
            .await
            .unwrap();
        assert_eq!(answer.status(), 303);
        let to = answer.headers()["location"].to_str().unwrap().to_owned();
        assert!(
            to.starts_with("/?location=L1&notice=Rang+up+12.75+AUD+as+Other%3A+Monero"),
            "{to}"
        );

        let seen = seen.lock().unwrap().clone();
        let (order, payment) = (&seen[0], &seen[1]);
        assert_eq!(order.0, "/v2/orders");
        assert_eq!(
            order.1["order"]["line_items"],
            json!([
                {"name": "Flat white", "quantity": "2", "base_price_money": {"amount": 450, "currency": "AUD"}},
                {"name": "Almond croissant", "quantity": "1", "base_price_money": {"amount": 375, "currency": "AUD"}},
            ])
        );
        assert_eq!(payment.0, "/v2/payments");
        assert_eq!(payment.1["source_id"], "EXTERNAL");
        assert_eq!(
            payment.1["external_details"],
            json!({"type": "OTHER", "source": "Monero"})
        );
        assert_eq!(
            payment.1["amount_money"],
            json!({"amount": 1275, "currency": "AUD"}),
            "the order's own total"
        );
        assert_eq!(payment.1["order_id"], "O1");
        assert_eq!(payment.1["autocomplete"], true);

        // The page itself shows the location and the form.
        let html = client
            .get(format!("{page}/{to}", to = to.trim_start_matches('/')))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(
            html.contains("Rang up 12.75 AUD as Other: Monero (P1)."),
            "{html}"
        );
        assert!(html.contains("Ring up a sale at Front"));
    }

    #[tokio::test]
    async fn a_refund_of_something_that_isn_t_a_payment_id_never_reaches_square_over_http() {
        let (square, seen) = stand_in().await;
        let page = serve(square, Some("EAAA-test")).await;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let answer = client
            .post(format!("{page}/payments/..%2Fv2%2Flocations/refund"))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body("location=L1")
            .send()
            .await
            .unwrap();
        assert_eq!(answer.status(), 303);
        assert!(
            answer.headers()["location"]
                .to_str()
                .unwrap()
                .contains("error=That+isn%27t+a+Square+payment+id")
        );
        assert!(seen.lock().unwrap().is_empty());
    }
}
