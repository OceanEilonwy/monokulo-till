//! Writes `index.html` into a folder of screenshots: each key screen or state
//! as a row, the iPhone, Android phone, iPad and Android tablet side by side,
//! with a switch between light and dark (screenshots.yml's artifact).
//!
//! ```sh
//! cargo run -p screenshot-compare -- <folder>
//! ```
//!
//! The folder holds one folder per device, as the platforms' screenshot tests
//! write them (ios/TillTests/ScreenshotTests.swift, android/…/ScreenshotTest.kt):
//! `iphone/`, `ipad/`, `android-phone/`, `android-tablet/`, each with
//! `<name>-light.png` and `<name>-dark.png`, `shots.txt` (and any
//! `shots-*.txt`) naming the screens in order, and `device.txt` naming the
//! device. Any other folder of PNGs (the iPad's window sizes) is shown after,
//! as it is. The page only points at the pictures, so it opens from the
//! unzipped artifact as it is.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The device folders the page knows, in its columns' order, with their names.
const DEVICES: [(&str, &str); 4] = [
    ("iphone", "iPhone"),
    ("android-phone", "Android phone"),
    ("ipad", "iPad"),
    ("android-tablet", "Android tablet"),
];

/// The sections the screens fall into, by their names' first word.
const SECTIONS: [(&str, &str); 5] = [
    ("setup", "Setup"),
    ("till", "The till"),
    ("sale", "A sale"),
    ("settings", "Settings"),
    ("landscape", "Tablets on their side"),
];

#[derive(Debug, PartialEq, Eq)]
struct Device {
    /// The folder's name.
    folder: String,
    /// "iPhone", or the folder's name for one the page doesn't know.
    label: String,
    /// What `device.txt` says: "iPhone 17 Pro · iOS 27.0".
    model: Option<String>,
    /// The screens it has, by name: whether each mode's picture is there.
    shots: BTreeMap<String, Modes>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Modes {
    light: bool,
    dark: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct Gallery {
    folder: String,
    pictures: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct Catalogue {
    devices: Vec<Device>,
    /// Every screen's name, in the order the devices list them.
    names: Vec<String>,
    galleries: Vec<Gallery>,
}

fn main() -> ExitCode {
    let Some(root) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: screenshot-compare <folder of screenshots>");
        return ExitCode::from(2);
    };
    match run(&root) {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("screenshot-compare: {}: {error}", root.display());
            ExitCode::FAILURE
        }
    }
}

fn run(root: &Path) -> io::Result<String> {
    fs::create_dir_all(root)?;
    let catalogue = scan(root)?;
    fs::write(root.join("index.html"), render(&catalogue))?;
    let pictures: usize = catalogue
        .devices
        .iter()
        .flat_map(|d| d.shots.values())
        .map(|m| usize::from(m.light) + usize::from(m.dark))
        .sum();
    Ok(format!(
        "index.html: {} screens on {} devices ({pictures} pictures), {} other sets",
        catalogue.names.len(),
        catalogue.devices.len(),
        catalogue.galleries.len()
    ))
}

/// The devices' screens and the other sets of pictures under `root`.
fn scan(root: &Path) -> io::Result<Catalogue> {
    let mut folders: Vec<String> = fs::read_dir(root)?
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    folders.sort_by_key(|f| {
        (
            DEVICES
                .iter()
                .position(|(d, _)| d == f)
                .unwrap_or(usize::MAX),
            f.clone(),
        )
    });

    let mut catalogue = Catalogue {
        devices: Vec::new(),
        names: Vec::new(),
        galleries: Vec::new(),
    };
    for folder in folders {
        let path = root.join(&folder);
        let mut lists: Vec<PathBuf> = fs::read_dir(&path)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                name == "shots.txt" || (name.starts_with("shots-") && name.ends_with(".txt"))
            })
            .collect();
        // shots.txt first, then the rest by name.
        lists.sort_by_key(|p| (p.file_name().is_some_and(|n| n != "shots.txt"), p.clone()));
        let pictures = pngs(&path)?;
        if lists.is_empty() {
            if !pictures.is_empty() {
                catalogue.galleries.push(Gallery { folder, pictures });
            }
            continue;
        }
        let mut shots = BTreeMap::new();
        for list in &lists {
            for name in fs::read_to_string(list)?
                .lines()
                .map(str::trim)
                .filter(|n| !n.is_empty())
            {
                let modes = Modes {
                    light: pictures.iter().any(|p| *p == format!("{name}-light.png")),
                    dark: pictures.iter().any(|p| *p == format!("{name}-dark.png")),
                };
                shots.insert(name.to_owned(), modes);
                if !catalogue.names.iter().any(|n| n == name) {
                    catalogue.names.push(name.to_owned());
                }
            }
        }
        let model = fs::read_to_string(path.join("device.txt"))
            .ok()
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty());
        let label = DEVICES
            .iter()
            .find(|(d, _)| *d == folder)
            .map_or_else(|| folder.clone(), |(_, l)| (*l).to_owned());
        catalogue.devices.push(Device {
            folder,
            label,
            model,
            shots,
        });
    }
    // Keep each section together, in the sections' order; within one, the devices' order.
    let section = |name: &str| {
        SECTIONS
            .iter()
            .position(|(s, _)| name.split('-').next() == Some(s))
            .unwrap_or(SECTIONS.len())
    };
    let order: Vec<String> = catalogue.names.clone();
    catalogue
        .names
        .sort_by_key(|n| (section(n), order.iter().position(|o| o == n)));
    Ok(catalogue)
}

fn pngs(folder: &Path) -> io::Result<Vec<String>> {
    let mut found: Vec<String> = fs::read_dir(folder)?
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".png"))
        .collect();
    found.sort();
    Ok(found)
}

/// "sale-close-sheet" as "Close sheet", "setup-4-point-of-sale" as "Step 4:
/// Point of sale", "landscape-sale-paid" as "Sale paid".
fn title(name: &str) -> String {
    fn capital(text: &str) -> String {
        let mut chars = text.chars();
        chars
            .next()
            .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
            .unwrap_or_default()
    }
    let mut words: Vec<&str> = name.split('-').collect();
    if words.first() == Some(&"landscape") {
        words.remove(0);
        return capital(&words.join(" "));
    }
    if words.len() > 2 && words[0] == "setup" && words[1].chars().all(|c| c.is_ascii_digit()) {
        return format!("Step {}: {}", words[1], capital(&words[2..].join(" ")));
    }
    if words.len() > 1 && SECTIONS.iter().any(|(s, _)| words[0] == *s) && words[0] != "settings" {
        words.remove(0);
    }
    capital(&words.join(" "))
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A path for `src` and `href`: each byte outside the unreserved set encoded.
fn url(path: &str) -> String {
    let mut out = String::new();
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char)
            }
            _ => {
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}

const STYLE: &str = r#"
:root { color-scheme: light dark; --ground: #f4f1ea; --ink: #1a1917; --muted: #6b665c; --card: #ffffff; --line: #e2ddd2; --accent: #1a1917; --on-accent: #fff; --h: 460px; }
@media (prefers-color-scheme: dark) { :root { --ground: #0d0d0c; --ink: #ece8df; --muted: #a8a29a; --card: #1b1a18; --line: #2c2a27; --accent: #ece8df; --on-accent: #111; } }
* { box-sizing: border-box; }
body { margin: 0; background: var(--ground); color: var(--ink); font: 15px/1.45 -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif; }
header { position: sticky; top: 0; z-index: 2; background: var(--ground); border-bottom: 1px solid var(--line); padding: 12px 16px; display: flex; flex-wrap: wrap; gap: 12px 20px; align-items: center; }
header h1 { font-size: 18px; margin: 0; margin-right: auto; }
.switch { display: inline-flex; border: 1px solid var(--line); border-radius: 999px; padding: 2px; background: var(--card); }
.switch button { border: 0; background: none; color: inherit; font: inherit; padding: 5px 14px; border-radius: 999px; cursor: pointer; }
.switch button[aria-pressed="true"] { background: var(--accent); color: var(--on-accent); }
label.size { display: inline-flex; gap: 8px; align-items: center; color: var(--muted); }
main { padding: 0 16px 48px; }
.devices { color: var(--muted); font-size: 13px; padding: 10px 0 0; margin: 0; }
h2 { font-size: 20px; margin: 32px 0 4px; }
section.shot { border-top: 1px solid var(--line); padding: 14px 0; }
section.shot h3 { font-size: 15px; margin: 0 0 10px; }
section.shot h3 code { font-weight: 400; color: var(--muted); font-size: 12px; margin-left: 8px; }
.row { display: flex; gap: 16px; overflow-x: auto; padding-bottom: 6px; }
figure { margin: 0; flex: none; }
figure img { height: var(--h); width: auto; display: block; border-radius: 10px; border: 1px solid var(--line); background: var(--card); }
figcaption { font-size: 12px; color: var(--muted); margin-top: 6px; }
.missing { height: var(--h); width: calc(var(--h) * 0.46); border: 1px dashed var(--line); border-radius: 10px; display: flex; align-items: center; justify-content: center; text-align: center; color: var(--muted); font-size: 13px; padding: 12px; }
.missing.tablet { width: calc(var(--h) * 0.75); }
body[data-mode="light"] .dark, body[data-mode="dark"] .light { display: none; }
body[data-show="phones"] .tablet, body[data-show="tablets"] .phone { display: none; }
.gallery { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 16px; }
.gallery img { width: 100%; height: auto; border-radius: 8px; border: 1px solid var(--line); }
"#;

const SCRIPT: &str = r#"
const body = document.body;
function remember(key, value) { try { localStorage.setItem("shots." + key, value); } catch (e) {} }
function recall(key) { try { return localStorage.getItem("shots." + key); } catch (e) { return null; } }
function choose(group, value) {
  body.dataset[group] = value;
  document.querySelectorAll('[data-group="' + group + '"] button').forEach(b => b.setAttribute("aria-pressed", String(b.dataset.value === value)));
  remember(group, value);
}
document.querySelectorAll("[data-group]").forEach(group => {
  group.querySelectorAll("button").forEach(b => b.addEventListener("click", () => choose(group.dataset.group, b.dataset.value)));
});
const size = document.getElementById("size");
function resize(v) { document.documentElement.style.setProperty("--h", v + "px"); remember("size", v); }
size.addEventListener("input", () => resize(size.value));
choose("mode", recall("mode") || (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light"));
choose("show", recall("show") || "all");
const kept = recall("size"); if (kept) { size.value = kept; resize(kept); }
"#;

fn render(catalogue: &Catalogue) -> String {
    let mut html = String::new();
    html.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str("<title>Till screenshots</title>\n<style>");
    html.push_str(STYLE);
    html.push_str("</style>\n</head>\n<body data-mode=\"light\" data-show=\"all\">\n<header>\n<h1>Till on iOS and Android</h1>\n");
    html.push_str(
        "<div class=\"switch\" data-group=\"mode\" role=\"group\" aria-label=\"Appearance\">\
         <button data-value=\"light\">Light</button><button data-value=\"dark\">Dark</button></div>\n",
    );
    html.push_str(
        "<div class=\"switch\" data-group=\"show\" role=\"group\" aria-label=\"Devices\">\
         <button data-value=\"all\">All</button><button data-value=\"phones\">Phones</button>\
         <button data-value=\"tablets\">Tablets</button></div>\n",
    );
    html.push_str(
        "<label class=\"size\">Size <input id=\"size\" type=\"range\" min=\"200\" max=\"1000\" step=\"20\" value=\"460\"></label>\n",
    );
    html.push_str("</header>\n<main>\n");

    if catalogue.devices.is_empty() {
        html.push_str("<p class=\"devices\">No device's screenshots were found: see the test runs' logs.</p>\n");
    } else {
        html.push_str("<p class=\"devices\">");
        let models: Vec<String> = catalogue
            .devices
            .iter()
            .map(|d| match &d.model {
                Some(model) => format!("<strong>{}</strong>: {}", escape(&d.label), escape(model)),
                None => format!("<strong>{}</strong>", escape(&d.label)),
            })
            .collect();
        html.push_str(&models.join(" · "));
        html.push_str("</p>\n");
    }

    let mut current: Option<&str> = None;
    for name in &catalogue.names {
        let heading = SECTIONS
            .iter()
            .find(|(s, _)| name.split('-').next() == Some(s))
            .map_or("Other", |(_, h)| h);
        if current != Some(heading) {
            let _ = writeln!(html, "<h2>{}</h2>", escape(heading));
            current = Some(heading);
        }
        let _ = writeln!(
            html,
            "<section class=\"shot\" id=\"{}\">\n<h3>{}<code>{}</code></h3>\n<div class=\"row\">",
            url(name),
            escape(&title(name)),
            escape(name)
        );
        let landscape = name.starts_with("landscape-");
        for device in &catalogue.devices {
            let class = if device.folder.contains("ipad") || device.folder.contains("tablet") {
                "tablet"
            } else {
                "phone"
            };
            // A phone isn't turned on its side: no empty column for it.
            if landscape && class == "phone" {
                continue;
            }
            match device.shots.get(name) {
                Some(modes) => {
                    for (mode, there) in [("light", modes.light), ("dark", modes.dark)] {
                        let caption = format!("{} · {mode}", device.label);
                        if there {
                            let path = url(&format!("{}/{name}-{mode}.png", device.folder));
                            let _ = writeln!(
                                html,
                                "<figure class=\"{class} {mode}\"><a href=\"{path}\" target=\"_blank\"><img loading=\"lazy\" src=\"{path}\" alt=\"{}\"></a><figcaption>{}</figcaption></figure>",
                                escape(&format!("{} on {}, {mode}", title(name), device.label)),
                                escape(&caption)
                            );
                        } else {
                            let _ = writeln!(
                                html,
                                "<figure class=\"{class} {mode}\"><div class=\"missing {class}\">No {mode} picture</div><figcaption>{}</figcaption></figure>",
                                escape(&caption)
                            );
                        }
                    }
                }
                None => {
                    let _ = writeln!(
                        html,
                        "<figure class=\"{class}\"><div class=\"missing {class}\">Not on {}</div><figcaption>{}</figcaption></figure>",
                        escape(&device.label),
                        escape(&device.label)
                    );
                }
            }
        }
        html.push_str("</div>\n</section>\n");
    }

    for gallery in &catalogue.galleries {
        let _ = writeln!(
            html,
            "<h2>{}</h2>\n<div class=\"gallery\">",
            escape(&gallery.folder)
        );
        for picture in &gallery.pictures {
            let path = url(&format!("{}/{picture}", gallery.folder));
            let label = picture.trim_end_matches(".png");
            let _ = writeln!(
                html,
                "<figure><a href=\"{path}\" target=\"_blank\"><img loading=\"lazy\" src=\"{path}\" alt=\"{0}\"></a><figcaption>{0}</figcaption></figure>",
                escape(label)
            );
        }
        html.push_str("</div>\n");
    }

    html.push_str("</main>\n<script>");
    html.push_str(SCRIPT);
    html.push_str("</script>\n</body>\n</html>\n");
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("screenshot-compare-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn put(root: &Path, path: &str, text: &str) {
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }

    #[test]
    fn each_screen_is_a_row_of_every_device_in_order() {
        let root = folder();
        put(
            &root,
            "android-phone/shots.txt",
            "setup-1-welcome\nsale-waiting\nsetup-5-float\n",
        );
        put(&root, "android-phone/device.txt", "Pixel 9 · Android 17\n");
        for name in ["setup-1-welcome", "sale-waiting", "setup-5-float"] {
            put(&root, &format!("android-phone/{name}-light.png"), "");
            put(&root, &format!("android-phone/{name}-dark.png"), "");
        }
        put(
            &root,
            "iphone/shots.txt",
            "setup-1-welcome\nsale-waiting\nsettings-wake-ups\n",
        );
        put(&root, "iphone/setup-1-welcome-light.png", "");
        put(&root, "iphone/sale-waiting-light.png", "");
        put(&root, "iphone/sale-waiting-dark.png", "");
        put(&root, "ipad/shots.txt", "landscape-sale-waiting\n");
        put(&root, "ipad/landscape-sale-waiting-light.png", "");
        put(&root, "iPad window sizes/ipad-mini.png", "");

        let catalogue = scan(&root).unwrap();
        let folders: Vec<&str> = catalogue
            .devices
            .iter()
            .map(|d| d.folder.as_str())
            .collect();
        assert_eq!(folders, ["iphone", "android-phone", "ipad"]);
        assert_eq!(
            catalogue.names,
            [
                "setup-1-welcome",
                "setup-5-float",
                "sale-waiting",
                "settings-wake-ups",
                "landscape-sale-waiting"
            ]
        );
        assert_eq!(
            catalogue.devices[0].shots["setup-1-welcome"],
            Modes {
                light: true,
                dark: false
            }
        );
        assert_eq!(
            catalogue.devices[1].model.as_deref(),
            Some("Pixel 9 · Android 17")
        );
        assert_eq!(
            catalogue.galleries,
            [Gallery {
                folder: "iPad window sizes".into(),
                pictures: vec!["ipad-mini.png".into()]
            }]
        );

        let html = render(&catalogue);
        assert!(html.contains("src=\"iphone/sale-waiting-dark.png\""));
        assert!(
            html.contains("Not on Android phone"),
            "the iPhone's Wake-ups has no Android picture"
        );
        assert!(html.contains("No dark picture"));
        assert!(html.contains("src=\"iPad%20window%20sizes/ipad-mini.png\""));
        // A landscape row has no phone columns.
        let landscape = html.split("id=\"landscape-sale-waiting\"").nth(1).unwrap();
        assert!(
            !landscape
                .split("</section>")
                .next()
                .unwrap()
                .contains("class=\"phone")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn names_read_as_titles() {
        assert_eq!(title("sale-close-sheet"), "Close sheet");
        assert_eq!(title("setup-4-point-of-sale"), "Step 4: Point of sale");
        assert_eq!(title("landscape-sale-paid"), "Sale paid");
        assert_eq!(title("settings"), "Settings");
        assert_eq!(title("settings-wake-ups"), "Settings wake ups");
    }

    #[test]
    fn paths_are_encoded_and_text_escaped() {
        assert_eq!(url("a b/c&d.png"), "a%20b/c%26d.png");
        assert_eq!(escape("<a & \"b\">"), "&lt;a &amp; &quot;b&quot;&gt;");
    }
}
