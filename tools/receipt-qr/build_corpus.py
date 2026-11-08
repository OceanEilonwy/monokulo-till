"""Turns corpus/raw/ into training and test name lists.

    corpus/train/<source>.txt   names to train dictionaries on, one per line
    corpus/test/<category>.txt  held-out names to build test receipts from

Every real source is split by a hash of the name, 85% train and 15% test, so
no test name is ever trained on. Wikidata labels and generated names are
training only. Standard library only; run with python3 -I.
"""
import csv
import hashlib
import json
import random
import re
import sys
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).parent / "corpus"
RAW, TRAIN, TEST = ROOT / "raw", ROOT / "train", ROOT / "test"
csv.field_size_limit(sys.maxsize)


def clean(name: str, cut=False) -> str | None:
    """A till-style item name: printable ASCII, single spaces, 3 to 48 chars.
    With cut, keep the head of a long web title (before ',', ' - ', '|', '(')."""
    name = " ".join(name.split())
    if cut:
        name = re.split(r",| - | – |\||\(|\[", name, maxsplit=1)[0].strip()
        if len(name) > 48:
            name = name[:48].rsplit(" ", 1)[0]
    if not re.fullmatch(r"[ -~]{3,48}", name or ""):
        return None
    return name


def is_test(name: str) -> bool:
    return hashlib.sha1(name.lower().encode()).digest()[0] < 256 * 15 // 100


def write(path: Path, names) -> int:
    path.parent.mkdir(parents=True, exist_ok=True)
    names = sorted(set(names))
    path.write_text("\n".join(names) + "\n")
    print(f"{len(names):>8}  {path.relative_to(ROOT)}")
    return len(names)


def split(source: str, names, test_category: str | None):
    train, test = [], []
    for n in names:
        (test if is_test(n) else train).append(n)
    write(TRAIN / f"{source}.txt", train)
    if test_category:
        write(TEST / f"{test_category}.txt", test)


def tsv_names(path: Path):
    with path.open(newline="", errors="replace") as f:
        reader = csv.reader(f, delimiter="\t", quoting=csv.QUOTE_NONE)
        header = next(reader)
        col = header.index("product_name")
        for row in reader:
            if len(row) > col and (n := clean(row[col])):
                yield n


def nypl_names():
    with (RAW / "nypl" / "Dish.csv").open(newline="", errors="replace") as f:
        for row in csv.DictReader(f):
            if (n := clean(row["name"])):
                yield n


def amazon_names(category: str):
    with (RAW / f"amazon-{category}.jsonl").open(errors="replace") as f:
        for line in f:
            try:
                title = json.loads(line).get("title") or ""
            except json.JSONDecodeError:
                continue  # the last line of a cut-short download
            if (n := clean(title, cut=True)):
                yield n


# Wikidata classes whose (sub)class labels are useful item words.
WIKIDATA_CLASSES = {
    "Q746549": "dish", "Q40050": "drink", "Q2095": "food", "Q11460": "clothing",
    "Q11451": "footwear", "Q39546": "tool", "Q1130322": "fastener",
    "Q206615": "building material", "Q12140": "medication",
}


def wikidata_names():
    out = []
    for qid in WIKIDATA_CLASSES:
        # Big classes time out at the query service; retry with a smaller limit.
        for limit in (30000, 10000, 3000):
            query = f"""SELECT DISTINCT ?l WHERE {{
              ?i wdt:P279* wd:{qid} . ?i rdfs:label ?l . FILTER(lang(?l) = "en") }} LIMIT {limit}"""
            url = "https://query.wikidata.org/sparql?" + urllib.parse.urlencode({"query": query, "format": "json"})
            req = urllib.request.Request(url, headers={"User-Agent": "till-receipt-qr/0.1 (corpus build)"})
            try:
                with urllib.request.urlopen(req, timeout=120) as r:
                    # Some labels hold raw control characters, which strict JSON refuses.
                    rows = json.loads(r.read().decode("utf-8"), strict=False)["results"]["bindings"]
            except Exception as e:
                print(f"wikidata {qid} (limit {limit}): {e}", file=sys.stderr)
                continue
            out += [n for b in rows if (n := clean(b["l"]["value"]))]
            break
    return out


def synthetic_names(count=60000, seed=1):
    """Till-style names from templates, for the categories the open data
    covers poorly. Different vocabulary from the hand-written test receipts."""
    rng = random.Random(seed)
    pick = rng.choice
    colors = ["black", "white", "navy", "grey", "olive", "red", "blue", "beige", "khaki", "charcoal", "cream", "green", "pink", "brown", "burgundy"]
    sizes = ["XS", "S", "M", "L", "XL", "XXL", "6", "8", "10", "12", "14", "28/30", "30/32", "32/32", "34/32", "UK 7", "UK 9", "EU 42", "US 10"]
    garments = ["T-shirt", "Tee", "Polo", "Shirt", "Oxford shirt", "Hoodie", "Sweatshirt", "Jumper", "Cardigan", "Jeans", "Chinos", "Shorts", "Joggers", "Leggings", "Dress", "Skirt", "Jacket", "Rain jacket", "Coat", "Blazer", "Vest", "Socks 3pk", "Boxers 2pk", "Beanie", "Cap", "Scarf", "Belt", "Sneakers", "Boots", "Sandals"]
    fits = ["slim", "regular", "relaxed", "oversized", "crew neck", "v-neck", "organic cotton", "merino", "linen", "denim", "fleece", "stretch"]
    tools = ["Hammer", "Claw hammer", "Screwdriver", "Screwdriver set", "Pliers", "Wrench", "Adjustable wrench", "Socket set", "Hex key set", "Utility knife", "Hacksaw", "Handsaw", "Chisel", "Tape measure", "Level", "Stud finder", "Drill", "Impact driver", "Jigsaw", "Sander", "Clamp", "Pry bar", "Trowel", "Putty knife", "Wire stripper"]
    fasteners = ["Wood screws", "Drywall screws", "Deck screws", "Machine screws", "Nails", "Finishing nails", "Bolts", "Hex bolts", "Nuts", "Washers", "Wall plugs", "Anchors", "Rivets", "Cable ties", "Hooks", "Brackets", "Hinges", "Shelf brackets"]
    materials = ["Plywood", "MDF board", "Pine board", "Oak board", "Timber", "Copper pipe", "PVC pipe", "Cable", "Extension lead", "Light bulb LED", "Paint", "Primer", "Wood stain", "Varnish", "Silicone sealant", "Caulk", "Grout", "Tile adhesive", "Cement", "Sand", "Gravel", "Insulation roll", "Duct tape", "Electrical tape", "Sandpaper"]
    metals = ["zinc", "stainless", "brass", "steel", "black", "galvanised"]
    units = ["mm", "cm", "m", "in", "ft"]
    drinks = ["Espresso", "Double espresso", "Americano", "Long black", "Latte", "Flat white", "Cappuccino", "Cortado", "Macchiato", "Mocha", "Chai latte", "Matcha latte", "Hot chocolate", "Tea", "Earl Grey", "Green tea", "Iced latte", "Iced coffee", "Cold brew", "Smoothie", "Orange juice", "Lemonade", "Kombucha", "Sparkling water"]
    cafe_sizes = ["small", "regular", "medium", "large", "12oz", "16oz", "8oz"]
    milks = ["oat milk", "almond milk", "soy milk", "skim", "whole milk", "coconut milk", "lactose free"]
    extras = ["extra shot", "decaf", "no sugar", "vanilla syrup", "caramel syrup", "hazelnut", "extra hot", "no foam", "to go", "half sweet"]
    bakes = ["Croissant", "Pain au chocolat", "Muffin", "Blueberry muffin", "Scone", "Cookie", "Brownie", "Banana bread", "Bagel", "Toastie", "Sandwich", "Wrap", "Quiche", "Sausage roll", "Danish", "Cinnamon roll", "Carrot cake", "Cheesecake"]
    beers = ["Lager", "Pale ale", "IPA", "Stout", "Pilsner", "Cider", "House red", "House white", "Rose", "Prosecco", "Gin and tonic", "Margarita", "Negroni", "Old fashioned", "Mojito", "Whisky", "Vodka soda"]
    pours = ["pint", "half", "schooner", "glass", "bottle", "175ml", "250ml", "double", "single", "carafe"]
    meds = ["Paracetamol", "Ibuprofen", "Aspirin", "Antihistamine", "Cough syrup", "Throat lozenges", "Vitamin C", "Vitamin D", "Multivitamin", "Zinc", "Magnesium", "Fish oil", "Probiotic", "Antiseptic cream", "Plasters", "Bandage", "Hand sanitiser", "Sunscreen SPF50", "Lip balm", "Toothpaste", "Mouthwash", "Shampoo", "Conditioner", "Body wash", "Deodorant", "Razor blades", "Cotton pads", "Nasal spray", "Eye drops"]
    strengths = ["200mg", "400mg", "500mg", "1000mg", "10mg", "25mcg", "1000IU", "50ml", "100ml", "200ml", "250ml", "500ml"]
    counts = ["16 tablets", "24 tablets", "32 caplets", "30 capsules", "60 capsules", "90 tablets", "20 pack", "40 pack"]

    def hardware():
        r = rng.random()
        if r < 0.4:
            return f"{pick(fasteners)} {rng.choice([3, 4, 5, 6, 8, 10])}x{rng.choice([16, 20, 25, 30, 40, 50, 60, 75, 100])} {pick(units[:1])}, {pick(['box', 'pack', 'bag'])} of {pick([10, 20, 25, 50, 100, 200, 500])}"
        if r < 0.7:
            return f"{pick(materials)} {rng.choice([1, 2, 2.5, 5, 10, 12, 18, 25, 50])} {pick(['L', 'kg', 'm', 'mm', 'ml'])}"
        return f"{pick(tools)} {pick(['', pick(metals), str(rng.choice([6, 8, 10, 12, 16, 18, 20, 24])) + ' ' + pick(units[:4])])}".strip()

    def clothing():
        return f"{pick(garments)} {pick(fits)}, {pick(colors)}, {pick(sizes)}"

    def cafe():
        if rng.random() < 0.6:
            parts = [pick(drinks), pick(cafe_sizes)]
            parts += rng.sample(milks + extras, rng.randint(0, 2))
            return ", ".join(parts)
        return pick(bakes) + rng.choice(["", ", warmed", ", to go", ", gluten free", ", vegan"])

    def bar():
        return f"{pick(beers)} {pick(pours)}"

    def pharmacy():
        return f"{pick(meds)} {pick(strengths)}{pick(['', ' ' + pick(counts)])}"

    makers = [hardware, clothing, cafe, bar, pharmacy]
    out = set()
    while len(out) < count:
        if (n := clean(pick(makers)())):
            out.add(n)
    return out


def main():
    split("off", list(tsv_names(RAW / "off.tsv")) + list(tsv_names(RAW / "opf.tsv")), "grocery")
    split("nypl", nypl_names(), "restaurant")
    split("amazon_tools", amazon_names("Tools_and_Home_Improvement"), "hardware")
    split("amazon_clothing", amazon_names("Clothing_Shoes_and_Jewelry"), "clothing")
    split("amazon_health", amazon_names("Health_and_Personal_Care"), "pharmacy")
    split("amazon_grocery", amazon_names("Grocery_and_Gourmet_Food"), None)
    write(TRAIN / "wikidata.txt", wikidata_names())
    write(TRAIN / "synthetic.txt", synthetic_names())


if __name__ == "__main__":
    main()
