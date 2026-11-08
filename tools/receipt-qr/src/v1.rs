//! The first prototype's encoding, kept as the "before" for comparison:
//! a CBOR array with every field, raw deflate with a hand-written preset
//! dictionary, a 64-byte signature when signed, then a URI-safe base-41
//! alphabet for QR alphanumeric mode.

use serde::Serialize;

use crate::receipts::Receipt;

pub const HAND_DICT: &str = "Total Subtotal Tax Tip Service charge Discount Cash Card Monero XMR \
Regular Small Medium Large Extra shot Oat milk Almond milk Soy milk Decaf Iced Hot \
Coffee Espresso Americano Latte Cappuccino Flat white Mocha Tea Chai Hot chocolate \
Croissant Pain au chocolat Muffin Scone Cookie Brownie Cake slice Sourdough Baguette Bagel \
Sandwich Wrap Salad Soup Burger Fries Pizza Pasta Chicken Beef Pork Fish Vegetarian Vegan \
Water Sparkling Juice Orange Apple Lemonade Soda Cola Beer Wine Glass Bottle Pint \
Bread Milk Eggs Butter Cheese Yogurt Bananas Apples Tomatoes Onions Potatoes Carrots Rice \
Organic Free range Whole Semi skimmed 1 L 2 L 500 g 250 g 1 kg kg @ /kg each pack x \
Screws Nails Bolts Washers Hinges Tape Glue Paint Brush Roller Sandpaper Drill bit \
Wood Pine Oak Plywood Steel Brass Zinc Stainless mm cm in ft Box of 100 Pack of 50 \
Starter Main Dessert Side Kids Combo Meal Add Without No ice With ";

#[derive(Serialize)]
struct Item(String, u32, u64);

#[derive(Serialize)]
struct Wire(
    u8,
    u64,
    String,
    u64,
    String,
    Vec<Item>,
    u64,
    u64,
    u64,
    u64,
    u64,
    String,
);

/// The packed `r=` value, as it went into the QR code.
pub fn encode(r: &Receipt, signed: bool) -> String {
    let wire = Wire(
        1,
        0x7f3a_91c2_44d0_1e5b,
        r.store.clone(),
        r.time,
        r.currency.clone(),
        r.items
            .iter()
            .map(|i| Item(i.name.clone(), i.qty, i.cents))
            .collect(),
        r.subtotal(),
        r.tax(),
        r.tip,
        r.total(),
        r.piconero(),
        r.sale_ref.clone(),
    );
    let mut cbor = Vec::new();
    ciborium::into_writer(&wire, &mut cbor).unwrap();
    let mut c = flate2::Compress::new(flate2::Compression::best(), false);
    c.set_dictionary(HAND_DICT.as_bytes()).unwrap();
    let mut deflated = Vec::with_capacity(cbor.len() + 64);
    c.compress_vec(&cbor, &mut deflated, flate2::FlushCompress::Finish)
        .unwrap();
    let mut framed = vec![1u8, 2];
    framed.extend_from_slice(&deflated);
    if signed {
        // Same size as an Ed25519 signature; its content doesn't change the QR size.
        framed.extend_from_slice(&[0xA5; 64]);
    }
    base41(&framed)
}

const B41: &[u8; 41] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ$*-.:";

fn base41(data: &[u8]) -> String {
    let mut s = String::new();
    for chunk in data.chunks(2) {
        let (mut n, digits) = match chunk {
            [a, b] => (((*a as u32) << 8) | *b as u32, 3),
            [a] => (*a as u32, 2),
            _ => unreachable!(),
        };
        for _ in 0..digits {
            s.push(B41[(n % 41) as usize] as char);
            n /= 41;
        }
    }
    s
}
