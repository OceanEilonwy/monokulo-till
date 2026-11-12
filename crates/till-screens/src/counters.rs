//! Counters (boards CounterMenu, CounterScope, CounterSettings; core.md
//! decisions 113 to 115): the point of sale names the device that took
//! each payment, the onboarding test sale names this counter's, and only
//! this counter's sales alert or start a Live Activity, unless the cashier chose every device. Every counter's sales
//! stay in the shift, and the title menu chooses whose the list shows.

use till_pos::PosDeviceId;
use till_sales::TillSale;

// A device's id crosses to Kotlin and Swift as the bare string it was.
#[cfg(feature = "uniffi")]
uniffi::custom_type!(PosDeviceId, String, {
    remote,
    lower: |id| id.0,
    try_lift: |id| Ok(PosDeviceId(id)),
});

/// This counter's point-of-sale device, as the test sale named it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CounterDevice {
    pub id: PosDeviceId,
    /// What the point of sale calls it ("Front iPad"); Clover names none.
    pub name: Option<String>,
}

/// How this till channels sales by device.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    /// `None` until a test sale named it (or with the test skipped): then
    /// every device's sales are this till's.
    pub device: Option<CounterDevice>,
    /// "Every device at Front Counter": every sale alerts here.
    pub every_device: bool,
    /// "Show other counters' sales in the list": the list opens on every
    /// counter's sales rather than this one's.
    pub show_others: bool,
    /// What the point of sale calls its devices, by id, where its payments
    /// don't say: Clover's devices endpoint (core.md decision 142).
    pub names: std::collections::BTreeMap<PosDeviceId, String>,
}

impl Counters {
    /// Whether `sale` is another counter's: taken on another device, or
    /// found by another till (which device took it, this till can't
    /// know). With no device of its own, every sale is this till's.
    pub fn other_counter(&self, sale: &TillSale) -> bool {
        let Some(mine) = &self.device else {
            return false;
        };
        match &sale.found {
            Some(found) => found.device_id.as_ref().is_some_and(|d| *d != mine.id),
            None => true,
        }
    }

    /// Whether `sale` stays quiet here: no alert, no Live Activity, no
    /// rising out of the shift. Only another
    /// counter's, and only while alerts are for this device.
    pub fn quiet(&self, sale: &TillSale) -> bool {
        !self.every_device && self.other_counter(sale)
    }

    /// The list's choice before the cashier makes one.
    pub fn default_choice(&self) -> CounterChoice {
        match &self.device {
            Some(device) if !self.show_others => CounterChoice::Device {
                device_id: device.id.clone(),
            },
            _ => CounterChoice::All,
        }
    }
}

/// Whose sales the shift's list shows (the title menu, C3).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum CounterChoice {
    /// "All counters".
    All,
    /// One point-of-sale device's: this counter's, or another's.
    Device { device_id: PosDeviceId },
    /// Sales other tills found, whose device this till doesn't know.
    OtherTills,
}

impl CounterChoice {
    /// Whether the list under this choice shows `sale`. This counter's
    /// shows every sale that isn't another counter's, a sale with no
    /// device named among them.
    pub fn shows(&self, sale: &TillSale, counters: &Counters) -> bool {
        match self {
            CounterChoice::All => true,
            CounterChoice::Device { device_id }
                if counters.device.as_ref().is_some_and(|d| d.id == *device_id) =>
            {
                !counters.other_counter(sale)
            }
            _ => group(sale).as_ref() == Some(self),
        }
    }
}

/// The group `sale` is listed under in the menu: its device, or other
/// tills'. A sale this till found with no device named has none.
fn group(sale: &TillSale) -> Option<CounterChoice> {
    match &sale.found {
        Some(found) => found.device_id.as_ref().map(|id| CounterChoice::Device {
            device_id: id.clone(),
        }),
        None => Some(CounterChoice::OtherTills),
    }
}

/// One line of the title menu.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct CounterItem {
    pub choice: CounterChoice,
    /// "Front iPad", "Other tills", "All counters".
    pub name: String,
    /// "This counter · alerts", "No alerts here".
    pub sub: Option<String>,
    /// "1 waiting", when any of its sales waits for payment.
    pub count: Option<String>,
    pub selected: bool,
}

/// The shift's title menu (C3): its title is the counter shown ("Front
/// iPad"), and it lists this counter, every other device with sales this
/// shift, other tills' sales when there are any, and "All counters".
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct CounterMenu {
    pub title: String,
    /// "Show sales from".
    pub heading: String,
    pub items: Vec<CounterItem>,
}

/// "All counters".
pub const ALL_COUNTERS: &str = "All counters";
/// The menu's heading.
pub const SHOW_SALES_FROM: &str = "Show sales from";
/// Sales other tills found, by name.
pub const OTHER_TILLS: &str = "Other tills";

/// What the menu calls `device_id`: its name on a sale, else the point of
/// sale's list of devices, else this counter's kept name, else "Device"
/// and the end of its id.
pub fn device_name(device_id: &PosDeviceId, sales: &[TillSale], counters: &Counters) -> String {
    sales
        .iter()
        .filter_map(TillSale::pos_device)
        .find(|(id, name)| *id == device_id && name.is_some())
        .and_then(|(_, name)| name.map(str::to_owned))
        .or_else(|| counters.names.get(device_id).cloned())
        .or_else(|| {
            counters
                .device
                .as_ref()
                .filter(|d| d.id == *device_id)
                .and_then(|d| d.name.clone())
        })
        .unwrap_or_else(|| {
            let end: String = device_id
                .as_str()
                .chars()
                .rev()
                .take(4)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            format!("Device {end}")
        })
}

/// The name a row shows for `sale`'s counter, when it's another counter's.
pub fn row_device(sale: &TillSale, sales: &[TillSale], counters: &Counters) -> Option<String> {
    if !counters.other_counter(sale) {
        return None;
    }
    Some(match group(sale)? {
        CounterChoice::Device { device_id } => device_name(&device_id, sales, counters),
        _ => sale
            .device_name
            .clone()
            .unwrap_or_else(|| OTHER_TILLS.to_owned()),
    })
}

fn waiting(sales: &[&TillSale]) -> Option<String> {
    let n = sales.iter().filter(|sale| sale.live()).count();
    (n > 0).then(|| format!("{n} waiting"))
}

/// The title menu over `sales` with `chosen` shown; `None` while there's
/// nothing to choose between: no device of this counter's and every sale
/// from one place.
pub fn menu(
    sales: &[TillSale],
    counters: &Counters,
    chosen: &CounterChoice,
) -> Option<CounterMenu> {
    let mut groups: Vec<CounterChoice> = Vec::new();
    if let Some(device) = &counters.device {
        groups.push(CounterChoice::Device {
            device_id: device.id.clone(),
        });
    }
    for sale in sales {
        if let Some(g) = group(sale)
            && !groups.contains(&g)
        {
            groups.push(g);
        }
    }
    // Other tills' sales last, before "All counters".
    groups.sort_by_key(|g| *g == CounterChoice::OtherTills);
    if counters.device.is_none() && groups.len() < 2 {
        return None;
    }
    let mine = counters.device.as_ref().map(|d| &d.id);
    let mut items: Vec<CounterItem> = groups
        .iter()
        .map(|g| {
            let of: Vec<&TillSale> = sales
                .iter()
                .filter(|sale| g.shows(sale, counters))
                .collect();
            let (name, sub) = match g {
                CounterChoice::Device { device_id } if Some(device_id) == mine => (
                    device_name(device_id, sales, counters),
                    "This counter · alerts",
                ),
                CounterChoice::Device { device_id } => (
                    device_name(device_id, sales, counters),
                    if counters.every_device {
                        "Alerts here too"
                    } else {
                        "No alerts here"
                    },
                ),
                _ => (
                    OTHER_TILLS.to_owned(),
                    if counters.every_device {
                        "Found by another till"
                    } else {
                        "Found by another till · no alerts here"
                    },
                ),
            };
            CounterItem {
                choice: g.clone(),
                name,
                sub: Some(sub.into()),
                count: waiting(&of),
                selected: g == chosen,
            }
        })
        .collect();
    items.push(CounterItem {
        choice: CounterChoice::All,
        name: ALL_COUNTERS.into(),
        sub: None,
        count: waiting(&sales.iter().collect::<Vec<_>>()),
        selected: *chosen == CounterChoice::All,
    });
    let title = items
        .iter()
        .find(|item| item.selected)
        .map_or_else(|| ALL_COUNTERS.to_owned(), |item| item.name.clone());
    Some(CounterMenu {
        title,
        heading: SHOW_SALES_FROM.into(),
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{found, till_sale};
    use monokulo_client::SalePhase;

    fn sale(id: &str, device: Option<(&str, &str)>, phase: SalePhase) -> TillSale {
        let mut found = found(id, 100, "USD", Some(id), "o", "2026-11-09T10:00:00Z");
        found.device_id = device.map(|d| d.0.into());
        found.device_name = device.map(|d| d.1.into());
        TillSale {
            found: Some(found),
            phase,
            ..till_sale(id, "1.00", "USD", "0.01")
        }
    }

    fn front() -> Counters {
        Counters {
            device: Some(CounterDevice {
                id: "dev_front".into(),
                name: Some("Front iPad".into()),
            }),
            every_device: false,
            show_others: true,
            names: Default::default(),
        }
    }

    #[test]
    fn only_this_counters_sales_alert_unless_every_device_does() {
        let mine = sale("a", Some(("dev_front", "Front iPad")), SalePhase::Waiting);
        let bar = sale("b", Some(("dev_bar", "Bar iPad")), SalePhase::Waiting);
        let unnamed = sale("c", None, SalePhase::Waiting);
        let other_till = till_sale("d", "1.00", "USD", "0.01");
        let c = front();
        assert!(!c.quiet(&mine));
        assert!(c.quiet(&bar));
        assert!(!c.quiet(&unnamed), "no device named: this till's");
        assert!(c.quiet(&other_till), "found by another till");
        let every = Counters {
            every_device: true,
            ..front()
        };
        assert!(!every.quiet(&bar) && !every.quiet(&other_till));
        assert!(every.other_counter(&bar));
        let unknown = Counters::default();
        assert!(!unknown.quiet(&bar) && !unknown.quiet(&other_till));
    }

    #[test]
    fn the_menu_lists_this_counter_each_device_and_all_with_waiting_counts() {
        let sales = [
            sale("a", Some(("dev_front", "Front iPad")), SalePhase::Waiting),
            sale("b", Some(("dev_bar", "Bar iPad")), SalePhase::Waiting),
            sale("c", Some(("dev_kiosk", "Kiosk")), SalePhase::Paid),
            till_sale("d", "1.00", "USD", "0.01"),
        ];
        let c = front();
        let chosen = CounterChoice::Device {
            device_id: "dev_front".into(),
        };
        let menu = menu(&sales, &c, &chosen).unwrap();
        assert_eq!(menu.title, "Front iPad");
        assert_eq!(menu.heading, "Show sales from");
        let lines: Vec<(&str, Option<&str>, Option<&str>, bool)> = menu
            .items
            .iter()
            .map(|i| {
                (
                    i.name.as_str(),
                    i.sub.as_deref(),
                    i.count.as_deref(),
                    i.selected,
                )
            })
            .collect();
        assert_eq!(
            lines,
            [
                (
                    "Front iPad",
                    Some("This counter · alerts"),
                    Some("1 waiting"),
                    true
                ),
                ("Bar iPad", Some("No alerts here"), Some("1 waiting"), false),
                ("Kiosk", Some("No alerts here"), None, false),
                (
                    "Other tills",
                    Some("Found by another till · no alerts here"),
                    Some("1 waiting"),
                    false
                ),
                ("All counters", None, Some("3 waiting"), false),
            ]
        );
        assert!(chosen.shows(&sales[0], &c) && !chosen.shows(&sales[1], &c));
        assert!(CounterChoice::OtherTills.shows(&sales[3], &c));
        assert!(sales.iter().all(|s| CounterChoice::All.shows(s, &c)));
        assert_eq!(menu_title(&sales, &c, &CounterChoice::All), "All counters");
    }

    fn menu_title(sales: &[TillSale], c: &Counters, chosen: &CounterChoice) -> String {
        menu(sales, c, chosen).unwrap().title
    }

    #[test]
    fn with_no_counter_of_its_own_and_one_place_theres_no_menu() {
        let sales = [sale(
            "a",
            Some(("dev_front", "Front iPad")),
            SalePhase::Waiting,
        )];
        assert_eq!(
            menu(&sales, &Counters::default(), &CounterChoice::All),
            None
        );
        let two = [
            sales[0].clone(),
            sale("b", Some(("dev_bar", "Bar iPad")), SalePhase::Waiting),
        ];
        assert!(menu(&two, &Counters::default(), &CounterChoice::All).is_some());
        assert!(
            menu(&[], &front(), &CounterChoice::All).is_some(),
            "this counter, before any sale"
        );
    }

    #[test]
    fn the_list_opens_on_this_counter_or_every_counter_as_settings_say() {
        let mut c = front();
        assert_eq!(c.default_choice(), CounterChoice::All);
        c.show_others = false;
        assert_eq!(
            c.default_choice(),
            CounterChoice::Device {
                device_id: "dev_front".into()
            }
        );
        assert_eq!(Counters::default().default_choice(), CounterChoice::All);
    }

    #[test]
    fn a_device_with_no_name_is_called_by_the_end_of_its_id() {
        let sales = [sale("a", None, SalePhase::Waiting)];
        let mut nameless = sales[0].clone();
        nameless.found.as_mut().unwrap().device_id = Some("A1B2C3D4".into());
        assert_eq!(
            device_name(&"A1B2C3D4".into(), &[nameless], &Counters::default()),
            "Device C3D4"
        );
        assert_eq!(
            device_name(&"dev_front".into(), &[], &front()),
            "Front iPad"
        );
        let bar = sale("b", Some(("dev_bar", "Bar iPad")), SalePhase::Waiting);
        assert_eq!(
            row_device(&bar, std::slice::from_ref(&bar), &front()).as_deref(),
            Some("Bar iPad")
        );
        assert_eq!(row_device(&bar, &[], &Counters::default()), None);
    }

    #[test]
    fn a_clover_device_is_called_by_the_name_clovers_list_gives_it() {
        let mut clover = sale("c", None, SalePhase::Waiting);
        clover.found.as_mut().unwrap().device_id = Some("DEV3F2A".into());
        let counters = Counters {
            names: [("DEV3F2A".into(), "Front counter".to_owned())].into(),
            ..Counters::default()
        };
        assert_eq!(
            device_name(&"DEV3F2A".into(), std::slice::from_ref(&clover), &counters),
            "Front counter",
            "not \"Device 3F2A\""
        );
        assert_eq!(
            device_name(&"DEV3F2A".into(), &[clover], &Counters::default()),
            "Device 3F2A"
        );
    }
}
