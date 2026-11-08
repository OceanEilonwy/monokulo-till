//! Five realistic test receipts, smallest to largest. Their item names are
//! written by hand and are not in the training corpus.

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub name: String,
    pub qty: u32,
    /// Unit price in cents.
    pub cents: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Receipt {
    pub store: String,
    /// The POS's own reference ("#1042", "Table 12").
    pub sale_ref: String,
    /// Unix time, seconds.
    pub time: u64,
    pub currency: String,
    pub items: Vec<Item>,
    /// Tax rate in basis points (888 = 8.88%); tax is subtotal × rate,
    /// rounded half away from zero, as `orf-codec` derives it.
    pub tax_bp: u64,
    pub tip: u64,
}

impl Receipt {
    pub fn subtotal(&self) -> u64 {
        self.items.iter().map(|i| i.qty as u64 * i.cents).sum()
    }
    pub fn tax(&self) -> u64 {
        (self.subtotal() * self.tax_bp + 5_000) / 10_000
    }
    pub fn total(&self) -> u64 {
        self.subtotal() + self.tax() + self.tip
    }
    /// As an ORF 0.1 receipt, the way a till would send it to Monokulo.
    pub fn to_orf(&self) -> orf_codec::Receipt {
        let money = |cents: u64| cents as f64 / 100.0;
        let time = time_rfc3339(self.time);
        let mut json = serde_json::json!({
            "orf_version": "0.1.0",
            "receipt": { "receipt_id": self.sale_ref, "issue_timestamp": time, "receipt_type": "sale" },
            "merchant": { "legal_name": self.store, "country_code": "US" },
            "line_items": self.items.iter().map(|i| serde_json::json!({
                "description": i.name, "quantity": i.qty, "unit_price": money(i.cents),
                "line_total": money(i.qty as u64 * i.cents),
            })).collect::<Vec<_>>(),
            "total": money(self.total()),
            "currency": self.currency,
        });
        if self.tax_bp != 0 {
            json["tax"] = serde_json::json!({ "tax_lines": [
                { "rate": self.tax_bp as f64 / 10_000.0, "tax_amount": money(self.tax()) }
            ] });
        }
        if self.tip != 0 {
            json["extensions"] = serde_json::json!({ "tip": { "amount": money(self.tip) } });
        }
        serde_json::from_value(json).expect("a valid ORF receipt")
    }

    /// At a rate of $172.70 per XMR, in piconero.
    pub fn piconero(&self) -> u64 {
        self.total() * 1_000_000_000_000 / 17_270
    }
}

/// Unix seconds as RFC 3339 in UTC, e.g. 2025-10-19T16:07:00Z.
fn time_rfc3339(unix: u64) -> String {
    let days = unix / 86_400;
    let secs = unix % 86_400;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + (m <= 2) as i64;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    )
}

fn it(name: &str, qty: u32, cents: u64) -> Item {
    Item {
        name: name.into(),
        qty,
        cents,
    }
}

pub fn samples() -> Vec<(&'static str, Receipt)> {
    let mk = |store: &str, sale_ref: &str, items: Vec<Item>, tax_bp: u64, tip: u64| Receipt {
        store: store.into(),
        sale_ref: sale_ref.into(),
        time: 1_760_890_020,
        currency: "USD".into(),
        items,
        tax_bp,
        tip,
    };
    vec![
        (
            "coffee",
            mk(
                "The Bean Room",
                "#1042",
                vec![
                    it("Flat white, oat milk", 2, 450),
                    it("Almond croissant", 1, 375),
                ],
                888,
                0,
            ),
        ),
        (
            "bakery",
            mk(
                "Rise & Crumb Bakery",
                "#2217",
                vec![
                    it("Sourdough loaf", 1, 750),
                    it("Baguette", 2, 325),
                    it("Pain au chocolat", 4, 350),
                    it("Cinnamon bun", 2, 425),
                    it("Latte, large", 1, 525),
                    it("Sparkling water", 1, 250),
                ],
                888,
                0,
            ),
        ),
        (
            "restaurant",
            mk(
                "Luca's Trattoria",
                "Table 12",
                vec![
                    it("Burrata, heirloom tomatoes", 1, 1600),
                    it("Fritto misto", 1, 1450),
                    it("Cacio e pepe", 2, 2200),
                    it("Pizza margherita, add nduja", 1, 1900),
                    it("Side salad, no onion", 1, 700),
                    it("Tiramisu", 2, 1100),
                    it("Glass Chianti Classico", 3, 1400),
                    it("Sparkling water 750 ml", 1, 650),
                ],
                888,
                3200,
            ),
        ),
        (
            "grocery",
            mk(
                "Greenleaf Market",
                "#88213",
                vec![
                    it("Organic bananas 1.24 kg @ 0.69/kg", 1, 86),
                    it("Free range eggs, dozen", 1, 549),
                    it("Whole milk 2 L", 1, 389),
                    it("Sourdough bread", 1, 599),
                    it("Cheddar, aged 400 g", 1, 799),
                    it("Greek yogurt 500 g", 2, 449),
                    it("Tomatoes on the vine 0.62 kg", 1, 310),
                    it("Red onions 1 kg", 1, 229),
                    it("Baby spinach 250 g", 1, 399),
                    it("Basmati rice 2 kg", 1, 649),
                    it("Olive oil extra virgin 1 L", 1, 1299),
                    it("Pasta penne 500 g", 3, 189),
                    it("Chopped tomatoes 400 g", 4, 119),
                    it("Dark chocolate 85%", 2, 329),
                    it("Coffee beans 1 kg", 1, 2199),
                ],
                0,
                0,
            ),
        ),
        (
            "hardware",
            mk(
                "Hollis Hardware",
                "INV-55120",
                vec![
                    it("Wood screws 4x40 mm, box of 200", 2, 899),
                    it("Wood screws 4x60 mm, box of 100", 1, 949),
                    it("Wall plugs brown 7 mm, pack of 100", 1, 449),
                    it("Masking tape 25 mm x 50 m", 3, 399),
                    it("Sandpaper assorted, pack of 10", 2, 549),
                    it("Paint roller kit 9 in", 1, 1499),
                    it("Interior emulsion white 5 L", 2, 3299),
                    it("Paint brush set 3 pc", 1, 1199),
                    it("Dust sheet 3.6 x 2.7 m", 2, 699),
                    it("Filler ready mixed 600 g", 1, 799),
                    it("Caulk white 310 ml", 2, 549),
                    it("Caulking gun", 1, 899),
                    it("Utility knife + 10 blades", 1, 1149),
                    it("Spirit level 600 mm", 1, 1899),
                    it("Tape measure 8 m", 1, 1299),
                    it("Drill bit set HSS 13 pc", 1, 2499),
                    it("Hinges brass 75 mm, pair", 3, 699),
                    it("Cabinet handles steel 128 mm", 8, 349),
                    it("Wood glue 250 ml", 1, 649),
                    it("Pine batten 2.4 m", 6, 425),
                    it("Plywood sheet 12 mm 1220x607", 2, 1899),
                    it("Safety glasses", 1, 599),
                ],
                888,
                0,
            ),
        ),
    ]
}
