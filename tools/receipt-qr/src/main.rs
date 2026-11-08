//! Prototype: how big does a Monero payment QR get when it carries the whole
//! receipt? Encodes with `orf-codec`, trains zstd dictionaries on several
//! mixes of the corpus to compare against its shipped one, and scores them
//! on test receipts built only from held-out names.
//!
//! ```sh
//! ./fetch-corpus.sh                       # once
//! cargo run --release                     # evaluate, write out/
//! cargo run --release -- decode '<uri>'   # decode a scanned payment URI
//! ```

mod monero;
mod receipts;
mod v1;

use std::collections::BTreeMap;
use std::{env, fs, process};

use orf_codec::Codec;
use qrcode::bits::Bits;
use qrcode::{EcLevel, QrCode, Version};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use serde_json::{Map, Value, json};

use receipts::{Item, Receipt};

const DICT_SIZES: [usize; 3] = [32 << 10, 64 << 10, 110 << 10];
const TRAINING_SAMPLES: usize = 40_000;
const RECEIPTS_PER_CATEGORY: usize = 40;

/// Dictionary variants: which training sources each one mixes. `orf-codec`
/// ships the "open" mix at 64 KB, trained by this tool on 2026-10-22.
const VARIANTS: [(&str, &[&str]); 4] = [
    ("food only", &["off"]),
    ("open", &["nypl", "wikidata", "synthetic"]),
    ("open + food", &["nypl", "wikidata", "synthetic", "off"]),
    (
        "all",
        &[
            "nypl",
            "wikidata",
            "synthetic",
            "off",
            "amazon_tools",
            "amazon_clothing",
            "amazon_health",
            "amazon_grocery",
        ],
    ),
];
const SHIPPED: &str = "shipped (open, 64k)";

/// Test categories: held-out names, typical prices in cents, tax and tip.
struct Category {
    name: &'static str,
    stores: &'static [&'static str],
    cents: (u64, u64),
    tax_bp: u64,
    tip_pct: u64,
}

const CATEGORIES: [Category; 5] = [
    Category {
        name: "restaurant",
        stores: &["Harbour Kitchen", "The Olive Tree", "Saigon Street"],
        cents: (450, 3800),
        tax_bp: 888,
        tip_pct: 18,
    },
    Category {
        name: "grocery",
        stores: &["Corner Fresh", "Valley Co-op", "Market Hall"],
        cents: (89, 1499),
        tax_bp: 0,
        tip_pct: 0,
    },
    Category {
        name: "hardware",
        stores: &["Benson Hardware", "Tool & Timber", "Northside DIY"],
        cents: (199, 8999),
        tax_bp: 888,
        tip_pct: 0,
    },
    Category {
        name: "clothing",
        stores: &["Thread & Co", "Second Stitch", "Harlow Outfitters"],
        cents: (900, 12000),
        tax_bp: 888,
        tip_pct: 0,
    },
    Category {
        name: "pharmacy",
        stores: &["Riverside Pharmacy", "Green Cross Chemist"],
        cents: (299, 2499),
        tax_bp: 888,
        tip_pct: 0,
    },
];

enum Segment<'a> {
    Alphanumeric(&'a str),
    Numeric(&'a str),
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [] => evaluate(),
        [cmd, uri] if cmd == "decode" => decode(uri),
        _ => {
            eprintln!("usage: receipt-qr [decode '<monero: URI>']");
            process::exit(2);
        }
    }
}

/// Decodes a scanned payment URI with the shipped dictionary, prints the
/// receipt, and says which known receipt it is, if any.
fn decode(uri: &str) {
    let parsed = monero::parse(uri).unwrap_or_else(|e| fail(e));
    println!("address:   {}", parsed.address);
    println!(
        "tx_amount: {}",
        parsed.tx_amount.as_deref().unwrap_or("(none)")
    );
    let Some(text) = orf_codec::query::find(uri) else {
        fail("this URI has no receipt (no orf= parameter)")
    };
    let receipt = orf_codec::decode_text(text).unwrap_or_else(|e| fail(&e.to_string()));
    println!(
        "receipt:   {} digits, decoded:\n{}",
        text.len(),
        serde_json::to_string_pretty(&receipt).unwrap()
    );

    let known = known_receipts();
    match known
        .iter()
        .find(|(_, r)| orf_codec::encode_text(&r.to_orf()).ok().as_deref() == Some(text))
    {
        Some((id, _)) => println!("\nmatches {id} exactly: the scan read every digit correctly"),
        None => println!(
            "\ndecoded cleanly, but it isn't one of this tool's {} test receipts",
            known.len()
        ),
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("error: {msg}");
    process::exit(1);
}

/// The hand-written receipts, plus the generated test receipts when the
/// corpus is on disk.
fn known_receipts() -> Vec<(String, Receipt)> {
    let mut all: Vec<(String, Receipt)> = receipts::samples()
        .into_iter()
        .map(|(slug, r)| (format!("hand/{slug}"), r))
        .collect();
    if let Ok(test) = try_load_dir("corpus/test") {
        for cat in &CATEGORIES {
            if let Some(names) = test.get(cat.name) {
                all.extend(test_receipts(cat, names));
            }
        }
    }
    all
}

fn evaluate() {
    let train = try_load_dir("corpus/train").unwrap_or_else(|e| fail(&e));
    let tests = known_receipts();
    if tests.len() == receipts::samples().len() {
        fail("corpus/test is missing: run ./fetch-corpus.sh first");
    }
    fs::create_dir_all("out").unwrap();
    let address = fake_subaddress();
    let orfs: Vec<orf_codec::Receipt> = tests.iter().map(|(_, r)| r.to_orf()).collect();

    // Every variant and size, plus the dictionary the library ships.
    let mut dicts: Vec<(String, String, usize, Vec<u8>)> = vec![(
        SHIPPED.into(),
        "open".into(),
        orf_codec::DEFAULT_DICTIONARY.len(),
        orf_codec::DEFAULT_DICTIONARY.to_vec(),
    )];
    for (variant, sources) in VARIANTS {
        let samples = training_samples(sources, &train);
        let refs: Vec<&[u8]> = samples.iter().map(|s| s.as_bytes()).collect();
        for size in DICT_SIZES {
            let dict = zstd::dict::from_samples(&refs, size).expect("dictionary training");
            dicts.push((
                format!("{variant}, {}k", size >> 10),
                variant.into(),
                size,
                dict,
            ));
        }
    }

    let mut results = Vec::new();
    for (label, variant, size, dict) in &dicts {
        // Trained dictionaries go under an experimental id; the shipped one is id 0.
        let trained;
        let codec = if label == SHIPPED {
            Codec::shared()
        } else {
            trained = Codec::with_dictionary(15, dict);
            &trained
        };
        let mut by_cat: BTreeMap<String, Vec<(usize, i16, i16)>> = BTreeMap::new();
        for ((id, r), orf) in tests.iter().zip(&orfs) {
            let bytes = codec
                .encode(orf)
                .unwrap_or_else(|e| fail(&format!("{id}: {e}")));
            assert_eq!(
                &codec.decode(&bytes).expect("decodes"),
                orf,
                "{id}: round trip"
            );
            let digits = orf_codec::to_digits(&bytes);
            let prefix = uri_prefix(&address, r);
            let m = smallest_qr(&prefix, &Segment::Numeric(&digits), EcLevel::M).1;
            let l = smallest_qr(&prefix, &Segment::Numeric(&digits), EcLevel::L).1;
            let cat = id.split('/').next().unwrap().to_string();
            by_cat.entry(cat).or_default().push((bytes.len(), m, l));
        }
        let n: usize = by_cat.values().map(Vec::len).sum();
        let mean = by_cat.values().flatten().map(|x| x.1 as f64).sum::<f64>() / n as f64;
        println!("{label:<24} mean QR version {mean:.2}");
        let cats: Map<String, Value> = by_cat
            .iter()
            .map(|(c, xs)| (c.clone(), stats(xs)))
            .collect();
        results.push(json!({
            "variant": variant, "dict_bytes": size, "label": label,
            "mean_version_m": mean, "categories": cats,
        }));
    }

    // QR codes for the page, made exactly as an app would: the library's
    // digits in a monero: URI.
    let mut shown = Vec::new();
    for (id, r) in &tests {
        let cat = id.split('/').next().unwrap();
        let is_hand = cat == "hand";
        if !is_hand && !is_median_of(id, &tests) {
            continue;
        }
        let slug = id.replace('/', "-");
        let prefix = uri_prefix(&address, r);
        let bytes = orf_codec::encode(&r.to_orf()).unwrap();
        let digits = orf_codec::to_digits(&bytes);
        let mut after = qr_row(&format!("{slug}-v2"), &prefix, Segment::Numeric(&digits));
        after["payload_bytes"] = json!(bytes.len());
        after["uri"] = json!(format!("{prefix}{digits}"));
        let before = if is_hand {
            qr_row(
                &format!("{slug}-v1"),
                &prefix,
                Segment::Alphanumeric(&v1::encode(r, false)),
            )
        } else {
            Value::Null
        };
        shown.push(json!({ "id": id, "category": cat, "receipt": receipt_json(r), "before": before, "after": after }));
    }

    let baseline_uri = monero::build(&address, tests[0].1.piconero());
    let baseline =
        QrCode::with_error_correction_level(baseline_uri.as_bytes(), EcLevel::M).unwrap();
    write_svg("out/baseline.svg", &baseline);
    let counts = |m: &BTreeMap<String, Vec<String>>| {
        m.iter()
            .map(|(k, v)| (k.clone(), json!(v.len())))
            .collect::<Map<_, _>>()
    };
    let report = json!({
        "baseline": { "uri_chars": baseline_uri.len(), "qr_version_m": version_num(baseline.version()), "svg": "baseline.svg" },
        "chosen": SHIPPED,
        "training": counts(&train),
        "test": counts(&try_load_dir("corpus/test").unwrap()),
        "receipts_per_category": RECEIPTS_PER_CATEGORY,
        "results": results,
        "shown": shown,
    });
    fs::write(
        "out/report.json",
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
}

fn try_load_dir(dir: &str) -> Result<BTreeMap<String, Vec<String>>, String> {
    let entries =
        fs::read_dir(dir).map_err(|_| format!("{dir} is missing: run ./fetch-corpus.sh first"))?;
    Ok(entries
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .map(|p| {
            let names = fs::read_to_string(&p)
                .unwrap()
                .lines()
                .map(str::to_string)
                .collect();
            (p.file_stem().unwrap().to_string_lossy().into_owned(), names)
        })
        .collect())
}

/// Synthetic receipt texts in the layout `orf-codec` compresses: the store,
/// the reference, then 1 to 25 names, all from one source (a real receipt
/// comes from one kind of shop).
fn training_samples(sources: &[&str], train: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    let mut rng = StdRng::seed_from_u64(42);
    let pools: Vec<&Vec<String>> = sources
        .iter()
        .map(|s| {
            train
                .get(*s)
                .unwrap_or_else(|| fail(&format!("corpus/train/{s}.txt missing")))
        })
        .collect();
    (0..TRAINING_SAMPLES)
        .map(|_| {
            let pool = pools.choose(&mut rng).unwrap();
            let mut lines = vec![
                pool.choose(&mut rng).unwrap().clone(),
                format!("#{}", rng.gen_range(100..99_999)),
            ];
            let n = item_count(&mut rng);
            lines.extend((0..n).map(|_| pool.choose(&mut rng).unwrap().clone()));
            lines.join("\n")
        })
        .collect()
}

/// 1 to 25 items, mostly small baskets: P(n) falls by 15% per extra item.
fn item_count(rng: &mut StdRng) -> usize {
    let mut n = 1;
    while n < 25 && rng.gen_bool(0.85) {
        n += 1;
    }
    n
}

fn test_receipts(cat: &Category, names: &[String]) -> Vec<(String, Receipt)> {
    let mut rng = StdRng::seed_from_u64(1000 + cat.name.len() as u64);
    (0..RECEIPTS_PER_CATEGORY)
        .map(|i| {
            let n = item_count(&mut rng);
            let items: Vec<Item> = (0..n)
                .map(|_| Item {
                    name: names.choose(&mut rng).unwrap().clone(),
                    qty: if rng.gen_bool(0.8) {
                        1
                    } else {
                        rng.gen_range(2..=4)
                    },
                    cents: rng.gen_range(cat.cents.0..=cat.cents.1) / 5 * 5
                        + 4 * rng.gen_range(0..2),
                })
                .collect();
            let sub: u64 = items.iter().map(|i| i.qty as u64 * i.cents).sum();
            let r = Receipt {
                store: cat.stores.choose(&mut rng).unwrap().to_string(),
                sale_ref: format!("#{}", rng.gen_range(1000..99_999)),
                time: 1_760_890_020 + 60 * rng.gen_range(0..100_000),
                currency: "USD".into(),
                items,
                tax_bp: cat.tax_bp,
                tip: sub * cat.tip_pct / 100,
            };
            (format!("{}/{i:02}", cat.name), r)
        })
        .collect()
}

/// The test receipt whose encoded size is its category's median, so the page
/// shows a typical one.
fn is_median_of(id: &str, tests: &[(String, Receipt)]) -> bool {
    let cat = id.split('/').next().unwrap();
    let mut sized: Vec<(usize, &str)> = tests
        .iter()
        .filter(|(i, _)| i.starts_with(&format!("{cat}/")))
        .map(|(i, r)| (orf_codec::encode(&r.to_orf()).unwrap().len(), i.as_str()))
        .collect();
    sized.sort();
    sized[sized.len() / 2].1 == id
}

fn stats(xs: &[(usize, i16, i16)]) -> Value {
    let mut m: Vec<i16> = xs.iter().map(|x| x.1).collect();
    let mut l: Vec<i16> = xs.iter().map(|x| x.2).collect();
    let mut b: Vec<usize> = xs.iter().map(|x| x.0).collect();
    m.sort();
    l.sort();
    b.sort();
    let p = |v: &[i16], q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
    json!({
        "count": xs.len(),
        "median_m": p(&m, 0.5), "p90_m": p(&m, 0.9), "max_m": m[m.len() - 1],
        "median_l": p(&l, 0.5), "p90_l": p(&l, 0.9),
        "median_bytes": b[b.len() / 2], "max_bytes": b[b.len() - 1],
    })
}

/// The URI up to and including `orf=`, so the digits can go in their own
/// QR segment.
fn uri_prefix(address: &str, r: &Receipt) -> String {
    format!(
        "{}&{}=",
        monero::build(address, r.piconero()),
        orf_codec::query::PARAM
    )
}

fn qr_row(name: &str, prefix: &str, value: Segment) -> Value {
    let (code, m) = smallest_qr(prefix, &value, EcLevel::M);
    let (_, l) = smallest_qr(prefix, &value, EcLevel::L);
    write_svg(&format!("out/{name}.svg"), &code);
    let len = match value {
        Segment::Alphanumeric(s) | Segment::Numeric(s) => s.len(),
    };
    json!({
        "value_chars": len, "uri_chars": prefix.len() + len,
        "qr_version_m": m, "qr_version_l": l, "modules": code.width(),
        "svg": format!("{name}.svg"),
    })
}

/// The URI as two QR segments: byte mode for `monero:<address>?…&orf=`, then
/// the receipt in the mode that suits it.
fn smallest_qr(prefix: &str, value: &Segment, ec: EcLevel) -> (QrCode, i16) {
    for v in 1..=40 {
        let mut bits = Bits::new(Version::Normal(v));
        let fits = bits.push_byte_data(prefix.as_bytes()).is_ok()
            && match value {
                Segment::Alphanumeric(s) => bits.push_alphanumeric_data(s.as_bytes()).is_ok(),
                Segment::Numeric(s) => bits.push_numeric_data(s.as_bytes()).is_ok(),
            }
            && bits.push_terminator(ec).is_ok();
        if fits && let Ok(code) = QrCode::with_bits(bits, ec) {
            return (code, v);
        }
    }
    panic!("doesn't fit in a QR code");
}

fn version_num(v: Version) -> i16 {
    match v {
        Version::Normal(n) | Version::Micro(n) => n,
    }
}

fn write_svg(path: &str, code: &QrCode) {
    let svg = code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(320, 320)
        .quiet_zone(true)
        .build();
    fs::write(path, svg).unwrap();
}

/// A subaddress-shaped string: 95 base58 characters starting with 8.
/// Seeded, so every run makes the same codes.
fn fake_subaddress() -> String {
    const B58: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let mut rng = StdRng::seed_from_u64(7);
    std::iter::once('8')
        .chain((0..94).map(|_| B58[rng.gen_range(0..B58.len())] as char))
        .collect()
}

fn receipt_json(r: &Receipt) -> Value {
    json!({
        "store": r.store, "ref": r.sale_ref,
        "items": r.items.iter().map(|i| json!([i.name, i.qty, i.cents])).collect::<Vec<_>>(),
        "subtotal": r.subtotal(), "tax": r.tax(), "tip": r.tip, "total": r.total(),
        "xmr": monero::xmr(r.piconero()),
    })
}
