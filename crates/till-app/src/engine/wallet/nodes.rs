//! The node's Change as Fob's Node screen has it (owner, 2026-11-13), in
//! D4 and in Settings: the node in use with Fob's checks, every other node
//! (the network's, then the ones this till was given) checked at once, Add
//! your own node checked before Use, and Remove. The checks and their words
//! are fob-wallet's (`node::check`, `node::check_all`, fob-words), with Fob's
//! name made Till's where a check says it.
//!
//! The nodes this till was given are kept in the model, as Fob keeps its
//! custom nodes: the one left and the one chosen on each change, and one
//! added by hand whose check passed. A till on a light wallet server has
//! none of this: its row is the server's.

use fob_wallet::node::{self, Line, Mark, Report, Setting, Usable};
use fob_wallet::words::node as said;
use till_screens::node_chooser::{
    NodeCheckLine, NodeChoiceInput, NodeChooserInput, NodeMark, NodeReport, NodeUse,
};
use till_screens::wallet_setup::CheckMark;
use till_wallet::Network;
use till_wallet::setup;

use super::super::{Msg, State, apply};
use crate::Screen;

/// Fob's words for a check, with Fob's name made Till's: "Sends blocks to
/// Till", "not an address Till can read".
fn till_words(text: &str) -> String {
    text.replace("Fob", "Till")
}

fn mark(mark: Mark) -> NodeMark {
    match mark {
        Mark::Pass => NodeMark::Pass,
        Mark::Fail => NodeMark::Fail,
        Mark::Checking => NodeMark::Checking,
        Mark::Skipped => NodeMark::Skipped,
    }
}

/// A report as the chooser shows it, `url` the address as it was given.
pub(super) fn report_view(url: &str, report: &Report) -> NodeReport {
    NodeReport {
        url: url.to_owned(),
        address: report.address.clone(),
        verdict: till_words(said::verdict(report.verdict())),
        mark: mark(report.mark()),
        rows: report
            .rows
            .iter()
            .map(|row| NodeCheckLine {
                title: till_words(said::title(row.step)),
                result: till_words(&said::result(&row.outcome)),
                second: said::second(&row.outcome).map(|s| till_words(&s)),
                mark: mark(row.mark),
                label: till_words(&said::label(row)),
            })
            .collect(),
        usable: match report.usable() {
            Usable::Use => NodeUse::Use,
            Usable::UseAnyway => NodeUse::UseAnyway,
            Usable::No => NodeUse::No,
        },
        checking: report.checking(),
    }
}

/// Another node's row, as Fob's `NodeChoice`: Use only when every check
/// passed, Remove when it's one this till was given.
fn choice(report: &Report, line: &Line, custom: &[String]) -> NodeChoiceInput {
    let can_use = report.usable() == Usable::Use;
    NodeChoiceInput {
        url: report.url.clone(),
        address: report.address.clone(),
        line: till_words(&said::line(line)),
        mark: if report.checking() {
            NodeMark::Checking
        } else if can_use {
            NodeMark::Pass
        } else {
            NodeMark::Fail
        },
        can_use,
        removable: custom.contains(&report.url),
    }
}

/// The nodes to offer besides the one in use: the network's own, then the
/// ones this till was given, each once (fob-wallet's `other_nodes`).
pub(super) fn other_nodes(
    network: Network,
    current: Option<&str>,
    custom: &[String],
) -> Vec<String> {
    let current = current.map(node::normalize);
    let mut out: Vec<String> = Vec::new();
    for url in network
        .default_nodes()
        .iter()
        .copied()
        .chain(custom.iter().map(String::as_str))
    {
        let url = node::normalize(url);
        if Some(&url) != current.as_ref() && !out.contains(&url) {
            out.push(url);
        }
    }
    out
}

/// `custom` with `url` kept on it, unless it's one of the network's own
/// (fob-wallet's `keep_node`).
pub(super) fn keep_node(network: Network, custom: &mut Vec<String>, url: &str) {
    let url = node::normalize(url);
    let default = network
        .default_nodes()
        .iter()
        .any(|n| node::normalize(n) == url);
    if !default && !custom.contains(&url) {
        custom.push(url);
    }
}

fn setting(url: &str) -> Setting {
    Setting {
        url: url.to_owned(),
        self_signed: false,
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// D4's lines for a finished check: setup's three (`setup::checks_of`), or
/// why the address can't be reached at all.
fn row_lines(network: Network, url: &str, report: &Report) -> Vec<(String, CheckMark)> {
    if let Err(problem) = node::reachable(&node::normalize(url)) {
        return vec![(
            fob_wallet::FobError::Endpoint(problem).to_string(),
            CheckMark::Failed,
        )];
    }
    setup::checks_of(network, report)
        .into_iter()
        .map(|c| (c.text, super::check_mark(c.state)))
        .collect()
}

impl State {
    /// Where the chooser is up, its node in use and network: D4, or
    /// Settings on a node (not a light wallet server).
    fn chooser_node(&self) -> Option<(Option<String>, Network)> {
        if self.screen == Screen::ThisTill {
            return Some((self.wallet.node.clone(), self.wallet.network()));
        }
        if !self.in_wallet_settings() || self.light_self_signed().is_some() {
            return None;
        }
        let link = self.parts.as_ref()?.side.model().wallet?;
        Some((link.node.clone(), till_wallet::backend::network_of(&link)))
    }

    fn custom_nodes(&self) -> Vec<String> {
        self.model().map(|m| m.custom_nodes).unwrap_or_default()
    }

    /// Keeps `urls` among this till's nodes, as Fob keeps the node left and
    /// the one chosen.
    pub(super) fn keep_nodes(&mut self, urls: impl IntoIterator<Item = String>) {
        let network = self.wallet.network();
        let mut custom = self.custom_nodes();
        let before = custom.len();
        for url in urls {
            keep_node(network, &mut custom, &url);
        }
        if custom.len() != before {
            self.keep(|m| m.custom_nodes = custom);
        }
    }

    /// Sends `change` to the engine from a check's own thread, as each of
    /// its steps finishes.
    fn progress(&self) -> impl Fn(super::super::Apply) + Send + Sync + 'static {
        let (tx, generation) = (self.tx.clone(), self.generation);
        move |change| {
            let _ = tx.send(Msg::Done(generation, change));
        }
    }

    /// The node at `url` checked as Fob checks one: its report for the
    /// chooser's card as each check finishes, and setup's lines under the
    /// row once they're done.
    pub(super) fn run_node_check(&mut self, url: String) {
        let network = self.wallet.network();
        let form = self.form();
        form.checks = vec![("Checking the node…".into(), CheckMark::Checking)];
        form.checking += 1;
        form.report = Some((url.clone(), Report::pending(network, &setting(&url))));
        let token = form.checking;
        let send = self.progress();
        self.task(async move {
            let shown = url.clone();
            let progress = move |report: &Report| {
                let (report, url) = (report.clone(), shown.clone());
                send(apply(move |s| {
                    if s.wallet.checking == token {
                        s.wallet.report = Some((url, report));
                    }
                }));
            };
            let report = node::check(network, &setting(&url), None, 0, now(), &progress).await;
            apply(move |s| {
                if s.wallet.checking != token {
                    return;
                }
                s.wallet.checks = row_lines(network, &url, &report);
                s.wallet.report = Some((url, report));
            })
        });
    }

    /// The node in use checked again, after a change was refused.
    pub(super) fn recheck_node_in_use(&mut self) {
        if let Some((Some(url), network)) = self.chooser_node() {
            self.wallet.network = super::wallet_network(network);
            self.run_node_check(url);
        }
    }

    /// The other nodes listed, "Checking…" each, then checked at once.
    pub(super) fn check_other_nodes(&mut self) {
        let Some((current, network)) = self.chooser_node() else {
            return;
        };
        let urls = other_nodes(network, current.as_deref(), &self.custom_nodes());
        let settings: Vec<Setting> = urls.iter().map(|u| setting(u)).collect();
        let form = self.form();
        form.choices_checking += 1;
        let token = form.choices_checking;
        form.choices = settings
            .iter()
            .map(|s| (Report::pending(network, s), Line::Checking))
            .collect();
        self.task(async move {
            let checked = node::check_all(network, &settings, None, 0, now()).await;
            apply(move |s| {
                if s.wallet.choices_checking == token {
                    s.wallet.choices = checked;
                }
            })
        });
    }

    /// `Action::CheckNodes`: the chooser opened, or its Check again.
    pub(in crate::engine) fn check_nodes(&mut self) {
        let Some((current, network)) = self.chooser_node() else {
            return;
        };
        if self.wallet.changing.is_some() {
            return;
        }
        self.wallet.network = super::wallet_network(network);
        if let Some(url) = current {
            self.run_node_check(url);
        }
        self.check_other_nodes();
    }

    /// `Action::CheckNewNode`: Add your own node's address, checked.
    pub(in crate::engine) fn check_new_node(&mut self, url: String) {
        let Some((_, network)) = self.chooser_node() else {
            return;
        };
        let typed = url.trim().to_owned();
        if typed.is_empty() {
            return;
        }
        let form = self.form();
        form.new_checking += 1;
        let token = form.new_checking;
        form.new_node = Some((typed.clone(), Report::pending(network, &setting(&typed))));
        let send = self.progress();
        self.task(async move {
            let shown = typed.clone();
            let progress = move |report: &Report| {
                let (report, typed) = (report.clone(), shown.clone());
                send(apply(move |s| {
                    if s.wallet.new_checking == token {
                        s.wallet.new_node = Some((typed, report));
                    }
                }));
            };
            let report = node::check(network, &setting(&typed), None, 0, now(), &progress).await;
            apply(move |s| {
                if s.wallet.new_checking == token {
                    s.wallet.new_node = Some((typed, report));
                }
            })
        });
    }

    /// `Action::CloseNewNode`: a node whose check passed (or only Up to date
    /// failed) is kept and the list checked again; any check still running
    /// is dropped.
    pub(in crate::engine) fn close_new_node(&mut self) {
        let form = self.form();
        form.new_checking += 1;
        let Some((typed, report)) = form.new_node.take() else {
            return;
        };
        if report.checking() || report.usable() == Usable::No || self.chooser_node().is_none() {
            return;
        }
        let before = self.custom_nodes();
        self.keep_nodes([typed]);
        if self.custom_nodes() != before {
            self.check_other_nodes();
        }
    }

    /// `Action::RemoveNode`: one this till was given, off the list at once.
    pub(in crate::engine) fn remove_node(&mut self, url: String) {
        if self.chooser_node().is_none() || self.wallet.changing.is_some() {
            return;
        }
        let url = node::normalize(&url);
        let mut custom = self.custom_nodes();
        if !custom.contains(&url) {
            return;
        }
        custom.retain(|n| *n != url);
        if self.keep(|m| m.custom_nodes = custom) {
            self.wallet.choices.retain(|(r, _)| r.url != url);
        }
    }

    /// Settings' Use: the wallet changes to `url` ("Changing node" till it
    /// has), then the node and the list are checked again. Refused, the
    /// node before is checked again and Settings says why.
    pub(super) fn change_node(&mut self, url: String) {
        if self.busy || self.wallet.changing.is_some() {
            return;
        }
        let Some((left, network)) = self.chooser_node() else {
            return;
        };
        self.wallet.network = super::wallet_network(network);
        self.wallet.changing = Some(url.clone());
        self.wallet.new_node = None;
        let node = url.clone();
        self.change_wallet_then(
            move |link| {
                link.node = Some(node);
                if let Some(fob) = &mut link.fob {
                    fob.node_from_fob = false;
                }
            },
            move |s| {
                s.wallet.changing = None;
                s.keep_nodes([left, Some(url.clone())].into_iter().flatten());
                s.run_node_check(url);
                if !s.wallet.choices.is_empty() {
                    s.check_other_nodes();
                }
            },
        );
    }

    /// What the chooser is drawn from, `current` the node in use.
    pub(super) fn node_chooser_input(
        &self,
        current: Option<&str>,
        custom: &[String],
    ) -> NodeChooserInput {
        let current = current.map(node::normalize);
        NodeChooserInput {
            current: self
                .wallet
                .report
                .as_ref()
                .filter(|(url, _)| Some(node::normalize(url)) == current)
                .map(|(url, report)| report_view(url, report)),
            changing: self.wallet.changing.clone(),
            choices: self
                .wallet
                .choices
                .iter()
                .filter(|(r, _)| Some(&r.url) != current.as_ref())
                .map(|(report, line)| choice(report, line, custom))
                .collect(),
            new_node: self
                .wallet
                .new_node
                .as_ref()
                .map(|(typed, report)| (typed.clone(), report_view(typed, report))),
            busy: self.busy,
            problem: self.problem.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_other_nodes_are_the_networks_then_the_tills_own_without_the_one_in_use() {
        let defaults: Vec<String> = Network::Stagenet
            .default_nodes()
            .iter()
            .map(|n| node::normalize(n))
            .collect();
        let custom = vec!["http://node.mine:38081".to_owned(), defaults[1].clone()];
        let others = other_nodes(Network::Stagenet, Some(&defaults[0]), &custom);
        assert!(!others.contains(&defaults[0]), "not the one in use");
        assert_eq!(others[..defaults.len() - 1], defaults[1..]);
        assert_eq!(
            others.last().map(String::as_str),
            Some("http://node.mine:38081")
        );
        assert_eq!(others.len(), defaults.len(), "each once");
    }

    #[test]
    fn only_a_node_besides_the_networks_own_is_kept_and_once() {
        let mut custom = Vec::new();
        keep_node(
            Network::Mainnet,
            &mut custom,
            Network::Mainnet.default_nodes()[0],
        );
        assert!(custom.is_empty());
        keep_node(Network::Mainnet, &mut custom, "node.mine:18081");
        keep_node(Network::Mainnet, &mut custom, "node.mine:18081");
        assert_eq!(custom, [node::normalize("node.mine:18081")]);
    }

    #[test]
    fn a_report_reads_in_fobs_words_with_tills_name() {
        let pending = Report::pending(Network::Mainnet, &setting("https://node.example:18089"));
        let view = report_view("https://node.example:18089", &pending);
        assert_eq!(view.verdict, "Checking…");
        assert_eq!(view.address, "node.example:18089");
        assert!(view.checking && view.usable == NodeUse::No);
        let titles: Vec<&str> = view.rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Certificate",
                "Answers",
                "Right network",
                "Up to date",
                "Sends blocks to Till"
            ]
        );
        assert!(view.rows.iter().all(|r| r.mark == NodeMark::Checking));
        let row = choice(
            &pending,
            &Line::Checking,
            std::slice::from_ref(&pending.url),
        );
        assert_eq!(row.line, "Checking…");
        assert!(row.removable && !row.can_use && row.mark == NodeMark::Checking);
    }

    #[tokio::test]
    async fn a_node_that_cant_be_reached_is_said_in_tills_words() {
        let report = node::check(
            Network::Mainnet,
            &setting("http://abcdefghijklmnop.onion:18081"),
            None,
            0,
            now(),
            &|_| {},
        )
        .await;
        let view = report_view("x", &report);
        assert!(!view.checking);
        assert_eq!(view.usable, NodeUse::No);
        assert!(
            view.rows.iter().all(|r| !r.result.contains("Fob")),
            "{:?}",
            view.rows
        );
    }
}
