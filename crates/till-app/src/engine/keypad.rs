//! Till's own keypad as the point of sale (docs/view-key-mode.md §7): the
//! keypad is the till's home once set up (`Screen::Keypad`), the shift's
//! list of sales a tap away ("Sales"), and a sale's code comes back to the
//! keypad when it's left, put aside as a live sale for the next customer.
//! Charge makes the sale and its payment at once (`Till::charge`) and opens
//! its code.

use monokulo_client::PointOfSale;
use till_pos::Rfc3339Instant;
use till_screens::keypad::{self, KeypadInput, KeypadKey, KeypadView};
use till_screens::onboarding::CurrencyRow;
use till_types::CurrencyCode;

use super::{Parts, State, apply};
use crate::Screen;
use crate::phases::Money;

/// What's typed on the keypad, kept by the engine.
#[derive(Default)]
pub(crate) struct KeypadForm {
    typed: String,
    reference: String,
    busy: bool,
    problem: Option<String>,
}

impl State {
    /// The till's point of sale is its own keypad, and setup is done: the
    /// keypad is home.
    pub(super) fn keypad_mode(&self) -> bool {
        self.model()
            .is_some_and(|m| m.setup_done && m.point_of_sale == Some(PointOfSale::Till))
    }

    /// The point of sale chosen is the keypad, in setup or after.
    pub(super) fn on_keypad(&self) -> bool {
        self.model()
            .is_some_and(|m| m.point_of_sale == Some(PointOfSale::Till))
    }

    /// Step 4's way on with the keypad chosen: "Continue with Till", then
    /// on the shop's own wallet its currency, then as any point of sale
    /// once connected (Prices, or back to Settings).
    pub(super) fn keypad_pos_next(&mut self) {
        let Some(model) = self.model() else { return };
        let needs_currency = model.wallet.is_some() && model.connection.is_none();
        if !model.setup_done && !self.pos_confirmed {
            if needs_currency {
                self.pos_confirmed = true;
                return;
            }
            return self.pos_done();
        }
        if needs_currency && model.keypad_currency.is_none() {
            return;
        }
        self.pos_done();
    }

    /// The keypad's currency for Settings: "US dollar · USD", "None (XMR
    /// only)", or the store's code.
    pub(super) fn keypad_currency_text(&self, p: &Parts) -> Option<String> {
        let code = self.keypad_currency(p)?;
        let code = code.as_str();
        if code == "XMR" {
            return Some("None (XMR only)".into());
        }
        let name = till_wallet::prices::currency_name(code);
        Some(if name.is_empty() {
            code.to_owned()
        } else {
            format!("{name} · {code}")
        })
    }

    /// Where the sales step back to: the keypad on a keypad till, else the
    /// shift.
    pub(super) fn shift_screen(&self) -> Screen {
        if self.keypad_mode() {
            Screen::Keypad
        } else {
            Screen::Till
        }
    }

    /// The keypad's currency: on the shop's own wallet the one chosen
    /// (`XMR` for none), with Monokulo the store's own.
    pub(super) fn keypad_currency(&self, p: &Parts) -> Option<CurrencyCode> {
        let model = p.side.model();
        let code = if model.wallet.is_some() && model.connection.is_none() {
            model.keypad_currency.clone()
        } else {
            p.live.now().store.map(|s| s.base_currency)
        }?;
        CurrencyCode::new(&code).ok()
    }

    /// A key, on screen or on a hardware keyboard.
    pub(super) fn keypad_press(&mut self, key: KeypadKey) {
        let Some(currency) = self.parts.as_ref().and_then(|p| self.keypad_currency(p)) else {
            return;
        };
        self.keypad.typed = keypad::press(&self.keypad.typed, key, currency);
        self.keypad.problem = None;
    }

    pub(super) fn keypad_reference(&mut self, text: String) {
        self.keypad.reference = text
            .chars()
            .take(till_pos::keypad::REFERENCE_MAX)
            .collect();
    }

    /// Charge: the sale and its payment made, and its code opened. A charge
    /// that failed says why on the keypad, and tried again with the same
    /// amount and reference keeps its idempotency key.
    pub(super) fn charge(&mut self) {
        if self.keypad.busy {
            return;
        }
        let Some(p) = &self.parts else { return };
        let Some(currency) = self.keypad_currency(p) else {
            return;
        };
        let Some(amount) = keypad::amount(&self.keypad.typed, currency) else {
            return;
        };
        let Some(till) = p.runner.till().filter(|_| p.runner.state().running) else {
            self.keypad.problem = Some("Start the shift to take payments.".into());
            return;
        };
        p.side.keypad.set_merchant(&self.merchant_name(p));
        let at = Rfc3339Instant::from_second(self.clock.now().0);
        let reference = self.keypad.reference.clone();
        let sale = p.side.keypad.charge(amount, Some(&reference), at);
        self.keypad.busy = true;
        self.keypad.problem = None;
        self.task(async move {
            let made = till.charge(&sale).await;
            apply(move |s| {
                s.keypad.busy = false;
                match made {
                    Ok(order_id) => {
                        s.keypad.typed.clear();
                        s.keypad.reference.clear();
                        s.open(order_id);
                    }
                    Err(e) => s.keypad.problem = Some(e),
                }
            })
        });
    }

    /// Who a keypad receipt is from: the store, or the wallet's name in Fob.
    fn merchant_name(&self, p: &Parts) -> String {
        if let Some(store) = p.live.now().store {
            return store.name;
        }
        p.side
            .model()
            .wallet
            .and_then(|w| w.fob.map(|f| f.wallet_name))
            .unwrap_or_default()
    }

    /// The keypad's currency chosen: kept, and on the shop's own wallet
    /// the wallet's prices follow it.
    pub(super) fn choose_keypad_currency(&mut self, code: String) {
        if CurrencyCode::new(&code).is_err() {
            return;
        }
        if !self.keep(|m| m.keypad_currency = Some(code.clone())) {
            return;
        }
        if code != "XMR" {
            self.change_wallet_currency(code);
        }
    }

    /// The keypad, while it's home.
    pub(super) fn keypad_view(&self, p: &Parts) -> KeypadView {
        let input = KeypadInput {
            currency: self.keypad_currency(p),
            typed: self.keypad.typed.clone(),
            reference: self.keypad.reference.clone(),
            busy: self.keypad.busy,
            problem: self.keypad.problem.clone(),
            sales: p.runner.state().till.sales,
            clock: self.device_clock(),
        };
        keypad::keypad(&input, self.config.device, &Money(self.platform.clone()))
    }

    /// The keypad's currencies to choose, on the shop's own wallet; none
    /// with Monokulo, whose store has its own.
    pub(super) fn keypad_currencies(&self, p: &Parts) -> Vec<CurrencyRow> {
        let model = p.side.model();
        if model.wallet.is_none() || model.connection.is_some() {
            return Vec::new();
        }
        let home = self
            .platform
            .currency_symbols("USD".into())
            .home
            .filter(|h| !h.is_empty());
        till_wallet::prices::currency_choices(home.as_deref())
            .into_iter()
            .map(|c| CurrencyRow {
                selected: model.keypad_currency.as_deref() == Some(c.code.as_str()),
                code: c.code,
                name: c.name,
                detail: c.detail,
                sign: c.sign,
                group: c.group.map(str::to_owned),
            })
            .collect()
    }
}
