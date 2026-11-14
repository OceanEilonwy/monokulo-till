//! The node's Change, as Fob's Node screen has it (owner, 2026-11-13: "Copy
//! the node selection dialog from Fob wallet"): the node in use with Fob's
//! checks, checked as it opens and by Check again at the top, "Changing
//! node" while the wallet changes to another; then Change node, every other
//! node (the network's, then the ones this till was given) checked at once,
//! with Use on the ones that pass and Remove on this till's own; and Add your
//! own node, its address checked the same way before Use.
//!
//! Every word of a check comes from Fob's wording (fob-words, through the
//! engine): what's here only lays the rows out and says which buttons are
//! on. A till on a light wallet server has no chooser: its row is the
//! server's, changed by setup's own dialog.

use crate::view::Press;

/// How one of a node's checks came out, as Fob marks it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum NodeMark {
    Pass,
    Fail,
    Checking,
    /// Not run: one it needs failed.
    Skipped,
}

/// Whether a checked node can be changed to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum NodeUse {
    Use,
    /// Only Up to date failed: "Use it anyway".
    UseAnyway,
    No,
}

/// One of a node's checks: "Up to date", "block 1,792,004 · 1 min ago".
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct NodeCheckLine {
    pub title: String,
    pub result: String,
    /// A line of its own under the result: "1.8 MB/s" under "31 blocks/s".
    pub second: Option<String>,
    pub mark: NodeMark,
    /// For TalkBack and VoiceOver: "Up to date: 2 days behind".
    pub label: String,
}

/// A node's checks as far as they've got.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct NodeReport {
    /// The address as it was given, for Use.
    pub url: String,
    /// "node.example:38081": no scheme, no login.
    pub address: String,
    /// "Working", "Checking…", or the failed check's words.
    pub verdict: String,
    /// The verdict's mark: Working's tick, Checking's dots, else the failure's "!".
    pub mark: NodeMark,
    pub rows: Vec<NodeCheckLine>,
    pub usable: NodeUse,
    pub checking: bool,
}

/// Another node, as the engine found it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeChoiceInput {
    pub url: String,
    pub address: String,
    /// "120 ms · in sync · fastest", "Didn't answer in 5 s", "Checking…".
    pub line: String,
    /// Pass when it can be used, Checking, else Fail.
    pub mark: NodeMark,
    pub can_use: bool,
    /// One this till was given, not one of the network's.
    pub removable: bool,
}

/// What the chooser is drawn from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NodeChooserInput {
    /// The node in use, checked; `None` until a check has begun.
    pub current: Option<NodeReport>,
    /// The node the wallet is changing to, until it's answered.
    pub changing: Option<String>,
    pub choices: Vec<NodeChoiceInput>,
    /// Add your own node's checks: the address as typed and its report.
    pub new_node: Option<(String, NodeReport)>,
    pub busy: bool,
    pub problem: Option<String>,
}

/// Another node on the list: its dot, address and one line, Use when it
/// passed, and Remove when it's this till's own.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct NodeChoiceRow {
    /// For `Action::SetNode` and `Action::RemoveNode`.
    pub url: String,
    pub address: String,
    pub line: String,
    /// The dot: Pass green, Fail red, else the outline colour.
    pub mark: NodeMark,
    /// "Use": `Action::SetNode`. Shown only when it's offered.
    pub use_node: Option<Press>,
    /// "Use node.example:18081", for the screen reader.
    pub use_said: String,
    /// "Changing" in Use's place, while the wallet changes to this one.
    pub changing: Option<String>,
    /// The bin, on one this till was given: `Action::RemoveNode`.
    pub remove: bool,
    /// "Remove node.example:18081".
    pub remove_said: String,
}

/// Add your own node (Fob's board 12): an address and its checks, Use, or
/// Use it anyway when only Up to date failed. Closing keeps the node if it
/// passed (`Action::CloseNewNode`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct NewNodeSheet {
    /// "Add your own node".
    pub title: String,
    /// "Address".
    pub field_label: String,
    /// "Your own node, or a public one: host:port, http:// or https://".
    pub hint: String,
    /// "Close".
    pub close: String,
    /// The address the report below is for, as it was typed: the screen
    /// shows the report and its buttons only while the field still says
    /// it, and Check alone otherwise.
    pub checked: String,
    pub report: Option<NodeReport>,
    /// "Check": `Action::CheckNewNode`, on while the field isn't blank.
    pub check: String,
    /// "Check again", or "Checking…" (off) while it's checked.
    pub check_again: Press,
    /// "Use", or "Use it anyway": `Action::SetNode` with the address.
    pub use_node: Press,
}

/// The node's Change, as Fob's Node screen.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct NodeChooser {
    /// "Node".
    pub title: String,
    /// The refresh at the top: "Check again" (`Action::CheckNodes`), turning
    /// while checks run.
    pub refresh: Press,
    /// The node in use, unless the wallet is changing to another.
    pub current: Option<NodeReport>,
    /// "Changing node".
    pub changing_title: String,
    /// The address being changed to, under "Changing node".
    pub changing: Option<String>,
    /// "Change node".
    pub section: String,
    pub choices: Vec<NodeChoiceRow>,
    /// "Add your own node".
    pub add: Press,
    pub new_node: NewNodeSheet,
    pub problem: Option<String>,
}

/// Fob's words for the chooser.
pub const TITLE: &str = "Node";
pub const CHECK_AGAIN: &str = "Check again";
pub const CHANGING_NODE: &str = "Changing node";
pub const CHANGE_NODE: &str = "Change node";
pub const ADD_NODE: &str = "Add your own node";
pub const ADDRESS_HINT: &str = "Your own node, or a public one: host:port, http:// or https://";

/// The chooser as `n` stands.
pub fn node_chooser(n: &NodeChooserInput) -> NodeChooser {
    let changing_to = n.changing.as_deref();
    let free = changing_to.is_none() && !n.busy;
    let checking = n.current.as_ref().is_none_or(|c| c.checking);
    let new_report = n.new_node.as_ref().map(|(_, r)| r);
    NodeChooser {
        title: TITLE.into(),
        refresh: Press::new(CHECK_AGAIN)
            .enabled(!checking && free)
            .busy(checking || changing_to.is_some()),
        current: n.current.clone().filter(|_| changing_to.is_none()),
        changing_title: CHANGING_NODE.into(),
        changing: changing_to.map(crate::wallet_setup::node_address),
        section: CHANGE_NODE.into(),
        choices: n
            .choices
            .iter()
            .map(|c| {
                let using = changing_to == Some(c.url.as_str());
                NodeChoiceRow {
                    url: c.url.clone(),
                    address: c.address.clone(),
                    line: c.line.clone(),
                    mark: c.mark,
                    use_node: (c.can_use && free && !using).then(|| Press::new("Use")),
                    use_said: format!("Use {}", c.address),
                    changing: using.then(|| "Changing".into()),
                    remove: c.removable && free,
                    remove_said: format!("Remove {}", c.address),
                }
            })
            .collect(),
        add: Press::new(ADD_NODE).enabled(free),
        new_node: NewNodeSheet {
            title: ADD_NODE.into(),
            field_label: "Address".into(),
            hint: ADDRESS_HINT.into(),
            close: "Close".into(),
            checked: n
                .new_node
                .as_ref()
                .map(|(typed, _)| typed.clone())
                .unwrap_or_default(),
            report: new_report.cloned(),
            check: "Check".into(),
            check_again: match new_report {
                Some(r) if r.checking => Press::new("Checking…").enabled(false),
                _ => Press::new(CHECK_AGAIN),
            },
            use_node: Press::new(match new_report.map(|r| r.usable) {
                Some(NodeUse::UseAnyway) => "Use it anyway",
                _ => "Use",
            })
            .enabled(free && new_report.is_some_and(|r| r.usable != NodeUse::No)),
        },
        problem: n.problem.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(url: &str, mark: NodeMark, usable: NodeUse) -> NodeReport {
        let checking = mark == NodeMark::Checking;
        NodeReport {
            url: url.into(),
            address: crate::wallet_setup::node_address(url),
            verdict: match mark {
                NodeMark::Pass => "Working",
                NodeMark::Checking => "Checking…",
                _ => "Not up to date",
            }
            .into(),
            mark,
            rows: vec![NodeCheckLine {
                title: "Answers".into(),
                result: if checking { "checking…" } else { "84 ms" }.into(),
                second: None,
                mark,
                label: "Answers: 84 ms".into(),
            }],
            usable,
            checking,
        }
    }

    fn choice(url: &str, mark: NodeMark, removable: bool) -> NodeChoiceInput {
        NodeChoiceInput {
            url: url.into(),
            address: crate::wallet_setup::node_address(url),
            line: "120 ms · in sync".into(),
            mark,
            can_use: mark == NodeMark::Pass,
            removable,
        }
    }

    fn input() -> NodeChooserInput {
        NodeChooserInput {
            current: Some(report(
                "https://node.example:18089",
                NodeMark::Pass,
                NodeUse::Use,
            )),
            changing: None,
            choices: vec![
                choice("https://a.example:18089", NodeMark::Pass, false),
                choice("https://b.example:18089", NodeMark::Fail, true),
            ],
            new_node: None,
            busy: false,
            problem: None,
        }
    }

    #[test]
    fn the_node_in_use_shows_with_use_on_the_nodes_that_passed_and_remove_on_the_tills_own() {
        let view = node_chooser(&input());
        assert_eq!(
            (view.title.as_str(), view.section.as_str()),
            ("Node", "Change node")
        );
        assert_eq!(view.current.as_ref().unwrap().verdict, "Working");
        assert!(view.refresh.enabled && !view.refresh.busy);
        assert_eq!(view.choices[0].use_node.as_ref().unwrap().label, "Use");
        assert_eq!(view.choices[0].use_said, "Use a.example:18089");
        assert!(!view.choices[0].remove, "the network's own stays");
        assert!(view.choices[1].use_node.is_none(), "it failed");
        assert!(view.choices[1].remove);
        assert_eq!(view.choices[1].remove_said, "Remove b.example:18089");
        assert_eq!(view.add.label, "Add your own node");
        assert!(view.add.enabled);
    }

    #[test]
    fn while_checking_the_refresh_turns_and_waits() {
        let view = node_chooser(&NodeChooserInput {
            current: Some(report(
                "https://node.example:18089",
                NodeMark::Checking,
                NodeUse::No,
            )),
            ..input()
        });
        assert!(view.refresh.busy && !view.refresh.enabled);
        let none = node_chooser(&NodeChooserInput {
            current: None,
            ..input()
        });
        assert!(none.refresh.busy, "nothing checked yet");
    }

    #[test]
    fn changing_node_hides_the_card_and_says_changing_on_its_row() {
        let view = node_chooser(&NodeChooserInput {
            changing: Some("https://a.example:18089".into()),
            ..input()
        });
        assert!(view.current.is_none());
        assert_eq!(view.changing.as_deref(), Some("a.example:18089"));
        assert_eq!(view.changing_title, "Changing node");
        assert_eq!(view.choices[0].changing.as_deref(), Some("Changing"));
        assert!(
            view.choices
                .iter()
                .all(|c| c.use_node.is_none() && !c.remove)
        );
        assert!(!view.add.enabled && view.refresh.busy);
    }

    #[test]
    fn a_node_added_by_hand_is_checked_then_used_or_used_anyway() {
        let blank = node_chooser(&input()).new_node;
        assert_eq!(blank.title, "Add your own node");
        assert_eq!(blank.hint, ADDRESS_HINT);
        assert_eq!(blank.check, "Check");
        assert!(blank.report.is_none() && !blank.use_node.enabled);

        let typed = "node.mine:18081".to_owned();
        let checking = node_chooser(&NodeChooserInput {
            new_node: Some((
                typed.clone(),
                report(&typed, NodeMark::Checking, NodeUse::No),
            )),
            ..input()
        })
        .new_node;
        assert_eq!(checking.checked, typed);
        assert_eq!(checking.check_again.label, "Checking…");
        assert!(!checking.check_again.enabled && !checking.use_node.enabled);

        let behind = node_chooser(&NodeChooserInput {
            new_node: Some((
                typed.clone(),
                report(&typed, NodeMark::Fail, NodeUse::UseAnyway),
            )),
            ..input()
        })
        .new_node;
        assert_eq!(behind.check_again.label, "Check again");
        assert_eq!(behind.use_node.label, "Use it anyway");
        assert!(behind.use_node.enabled);

        let dead = node_chooser(&NodeChooserInput {
            new_node: Some((typed.clone(), report(&typed, NodeMark::Fail, NodeUse::No))),
            ..input()
        })
        .new_node;
        assert_eq!(dead.use_node.label, "Use");
        assert!(!dead.use_node.enabled);
    }
}
