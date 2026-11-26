//! Reporting a specific Discord message: target binding, multi-step category
//! navigation, explicit submission, and lifecycle invalidation.

use std::collections::{BTreeMap, HashSet};

use super::*;
use crate::report::{
    ReportButtonKind, ReportError, ReportErrorKind, ReportMenu, ReportNode, ReportSubmission,
    attachment_summary, can_report_message, redacted_report_snippet, resolve_report_ids,
};

const MAX_SNIPPET_CHARS: usize = 160;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReportPhase {
    LoadingMenu,
    Selecting,
    Submitting,
    Submitted { report_id: Option<String> },
    MenuError(String),
    Blocked(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportDialog {
    pub request_id: u64,
    pub account_id: Option<String>,
    pub room_id: String,
    pub channel_id: String,
    pub message_id: String,
    pub message_link: String,
    pub author: String,
    pub time: String,
    pub preview: String,
    pub attachments_note: String,
    pub menu: Option<ReportMenu>,
    pub breadcrumbs: Vec<i64>,
    pub selected_options: BTreeMap<String, Vec<String>>,
    pub phase: ReportPhase,
    pub submit_error: Option<ReportError>,
    pub focus_tick: u32,
}

impl ReportDialog {
    pub fn current_node(&self) -> Option<&ReportNode> {
        let menu = self.menu.as_ref()?;
        let id = *self.breadcrumbs.last()?;
        menu.nodes.get(&id)
    }

    pub fn can_back(&self) -> bool {
        self.phase == ReportPhase::Selecting && self.breadcrumbs.len() > 1
    }

    pub fn can_submit_step(&self) -> bool {
        self.phase == ReportPhase::Selecting
            && self.current_node().is_some_and(|node| {
                node.children.is_empty()
                    || node
                        .button
                        .as_ref()
                        .is_some_and(|button| button.kind == ReportButtonKind::Submit)
            })
    }

    pub fn can_submit_now(&self, discord_online: bool) -> bool {
        discord_online
            && self.can_submit_step()
            && self.current_node().is_some_and(|node| {
                !node.is_multi_select_required
                    || node.elements.iter().any(|element| {
                        self.selected_options
                            .get(&element.name)
                            .is_some_and(|values| !values.is_empty())
                    })
            })
    }

    pub fn summary(&self) -> String {
        let Some(menu) = &self.menu else {
            return String::new();
        };
        let mut labels = Vec::new();
        for pair in self.breadcrumbs.windows(2) {
            if let Some(parent) = menu.nodes.get(&pair[0])
                && let Some(child) = parent
                    .children
                    .iter()
                    .find(|child| child.target_node_id == pair[1])
            {
                labels.push(child.label.as_str());
            }
        }
        labels.join(" → ")
    }

    fn submission(&self) -> Option<ReportSubmission> {
        let menu = self.menu.as_ref()?;
        let submittable: HashSet<&str> = self
            .breadcrumbs
            .iter()
            .filter_map(|id| menu.nodes.get(id))
            .flat_map(|node| node.elements.iter())
            .filter(|el| el.should_submit_data)
            .map(|el| el.name.as_str())
            .collect();
        let elements = self
            .selected_options
            .iter()
            .filter(|(key, values)| !values.is_empty() && submittable.contains(key.as_str()))
            .map(|(key, values)| (key.clone(), values.clone()))
            .collect();
        Some(ReportSubmission {
            version: menu.version.clone(),
            variant: menu.variant.clone(),
            language: menu.language.clone(),
            name: menu.name.clone(),
            postback_path: menu.postback_path.clone(),
            breadcrumbs: self.breadcrumbs.clone(),
            elements,
            channel_id: self.channel_id.clone(),
            message_id: self.message_id.clone(),
        })
    }
}

impl Application {
    /// Resolves `(room_id, message)` strictly from the side thread when `aside` is
    /// true, or strictly from the main timeline when false.
    fn resolve_report_target(&self, item: &str, aside: bool) -> Option<(String, &ChatMessage)> {
        if aside {
            let panel = self.state.aside.as_ref()?;
            let msg = panel.messages.iter().find(|msg| msg.item == item)?;
            return Some((panel.room.clone(), msg.as_ref()));
        }
        let room = self.state.selected.clone()?;
        let msg = self.state.messages.iter().find(|msg| msg.item == item)?;
        Some((room, msg.as_ref()))
    }

    fn next_report_id(&mut self) -> u64 {
        self.next_report_request = self.next_report_request.wrapping_add(1).max(1);
        self.next_report_request
    }

    pub(super) fn open_report_message(&mut self, item: String, aside: bool) -> Vec<Effect> {
        let Some((room_id, message)) = self.resolve_report_target(&item, aside) else {
            return vec![];
        };
        if !can_report_message(&room_id, message) {
            return vec![];
        }
        let Some((target_channel_id, target_message_id, link)) =
            resolve_report_ids(&room_id, message)
        else {
            return vec![];
        };
        let channel_id = target_channel_id.to_owned();
        let message_id = target_message_id.to_owned();
        let author = message.author.clone();
        let time = message.time.clone();
        let preview = redacted_report_snippet(&message.body, MAX_SNIPPET_CHARS);
        let attachments_note = attachment_summary(&message.attachments);
        let request_id = self.next_report_id();

        let (phase, dispatch_menu) = if !self.state.discord_linked {
            (
                ReportPhase::Blocked(
                    "Link your Discord account before reporting a message.".to_owned(),
                ),
                false,
            )
        } else if !self.state.discord_online {
            (
                ReportPhase::MenuError(
                    "Discord is offline. Reconnect to load report options or copy the message link."
                        .to_owned(),
                ),
                false,
            )
        } else {
            (ReportPhase::LoadingMenu, true)
        };

        self.state.report = Some(ReportDialog {
            request_id,
            account_id: self.state.discord_user.clone(),
            room_id,
            channel_id,
            message_id,
            message_link: link,
            author,
            time,
            preview,
            attachments_note,
            menu: None,
            breadcrumbs: Vec::new(),
            selected_options: BTreeMap::new(),
            phase,
            submit_error: None,
            focus_tick: 1,
        });

        if dispatch_menu {
            vec![Effect::Backend(Command::Discord(
                DiscordCommand::ReportMessageMenu {
                    request: request_id,
                },
            ))]
        } else {
            vec![]
        }
    }

    pub(super) fn report_choose_child(&mut self, target_node_id: i64) -> Vec<Effect> {
        let Some(report) = &mut self.state.report else {
            return vec![];
        };
        if report.phase != ReportPhase::Selecting {
            return vec![];
        }
        let valid = report.current_node().is_some_and(|node| {
            node.children
                .iter()
                .any(|child| child.target_node_id == target_node_id)
                || node.button.as_ref().is_some_and(|button| {
                    button.kind == ReportButtonKind::Next && button.target == Some(target_node_id)
                })
        }) && report
            .menu
            .as_ref()
            .is_some_and(|menu| menu.nodes.contains_key(&target_node_id));
        if valid {
            report.breadcrumbs.push(target_node_id);
            report.submit_error = None;
            report.focus_tick = report.focus_tick.wrapping_add(1);
        }
        vec![]
    }

    pub(super) fn report_toggle_option(&mut self, element: String, value: String) -> Vec<Effect> {
        let Some(report) = &mut self.state.report else {
            return vec![];
        };
        if report.phase != ReportPhase::Selecting {
            return vec![];
        }
        let valid = report.current_node().is_some_and(|node| {
            node.elements.iter().any(|el| {
                el.name == element && el.options.iter().any(|option| option.value == value)
            })
        });
        if !valid {
            return vec![];
        }
        let entry = report.selected_options.entry(element).or_default();
        if let Some(pos) = entry.iter().position(|existing| existing == &value) {
            entry.remove(pos);
        } else {
            entry.push(value);
        }
        report.submit_error = None;
        vec![]
    }

    pub(super) fn report_back(&mut self) -> Vec<Effect> {
        let Some(report) = &mut self.state.report else {
            return vec![];
        };
        if !report.can_back() {
            return vec![];
        }
        if let Some(popped) = report.breadcrumbs.pop()
            && let Some(node) = report
                .menu
                .as_ref()
                .and_then(|menu| menu.nodes.get(&popped))
        {
            for element in &node.elements {
                report.selected_options.remove(&element.name);
            }
        }
        report.submit_error = None;
        report.focus_tick = report.focus_tick.wrapping_add(1);
        vec![]
    }

    pub(super) fn report_retry_menu(&mut self) -> Vec<Effect> {
        let Some(current) = &self.state.report else {
            return vec![];
        };
        if !matches!(current.phase, ReportPhase::MenuError(_)) {
            return vec![];
        }
        if !self.state.discord_linked {
            if let Some(report) = &mut self.state.report {
                report.phase = ReportPhase::Blocked(
                    "Link your Discord account before reporting a message.".to_owned(),
                );
                report.focus_tick = report.focus_tick.wrapping_add(1);
            }
            return vec![];
        }
        if !self.state.discord_online {
            if let Some(report) = &mut self.state.report {
                report.phase = ReportPhase::MenuError(
                    "Discord is offline. Reconnect to load report options or copy the message link."
                        .to_owned(),
                );
                report.focus_tick = report.focus_tick.wrapping_add(1);
            }
            return vec![];
        }
        let request_id = self.next_report_id();
        if let Some(report) = &mut self.state.report {
            report.request_id = request_id;
            report.phase = ReportPhase::LoadingMenu;
            report.submit_error = None;
            report.focus_tick = report.focus_tick.wrapping_add(1);
        }
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::ReportMessageMenu {
                request: request_id,
            },
        ))]
    }

    pub(super) fn submit_report(&mut self) -> Vec<Effect> {
        let can_submit = self
            .state
            .report
            .as_ref()
            .is_some_and(|report| report.can_submit_now(self.state.discord_online));
        if !can_submit {
            return vec![];
        }
        let submission = self
            .state
            .report
            .as_ref()
            .and_then(ReportDialog::submission);
        let Some(submission) = submission else {
            return vec![];
        };
        let request_id = self.next_report_id();
        if let Some(report) = &mut self.state.report {
            report.request_id = request_id;
            report.phase = ReportPhase::Submitting;
            report.submit_error = None;
        }
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::SubmitMessageReport {
                request: request_id,
                submission,
            },
        ))]
    }

    pub(super) fn close_report(&mut self) -> Vec<Effect> {
        if self
            .state
            .report
            .as_ref()
            .is_some_and(|report| report.phase == ReportPhase::Submitting)
        {
            return vec![];
        }
        self.next_report_id();
        self.state.report = None;
        vec![]
    }

    pub(super) fn on_report_menu_event(
        &mut self,
        request: u64,
        result: Result<ReportMenu, ReportError>,
    ) {
        let Some(report) = &mut self.state.report else {
            return;
        };
        if report.request_id != request
            || report.phase != ReportPhase::LoadingMenu
            || report.account_id != self.state.discord_user
        {
            return;
        }
        match result {
            Ok(menu) => {
                report.breadcrumbs = vec![menu.root_node_id];
                report.selected_options.clear();
                report.menu = Some(menu);
                report.phase = ReportPhase::Selecting;
                report.submit_error = None;
            }
            Err(error) => {
                report.phase = if error.kind == ReportErrorKind::SessionRevoked {
                    ReportPhase::Blocked(error.message)
                } else {
                    ReportPhase::MenuError(error.message)
                };
            }
        }
        report.focus_tick = report.focus_tick.wrapping_add(1);
    }

    pub(super) fn on_message_reported_event(
        &mut self,
        request: u64,
        result: Result<Option<String>, ReportError>,
    ) {
        let Some(report) = &mut self.state.report else {
            return;
        };
        if report.request_id != request
            || report.phase != ReportPhase::Submitting
            || report.account_id != self.state.discord_user
        {
            return;
        }
        match result {
            Ok(report_id) => {
                report.phase = ReportPhase::Submitted { report_id };
                report.submit_error = None;
            }
            Err(error) => {
                if error.kind == ReportErrorKind::SessionRevoked {
                    report.phase = ReportPhase::Blocked(error.message);
                } else {
                    report.phase = ReportPhase::Selecting;
                    report.submit_error = Some(error);
                }
            }
        }
        report.focus_tick = report.focus_tick.wrapping_add(1);
    }

    pub(super) fn invalidate_report_on_account_change(&mut self) {
        self.next_report_id();
        self.state.report = None;
    }

    pub(super) fn check_report_room_access(&mut self) {
        let lost = self.state.report.as_ref().is_some_and(|report| {
            !matches!(
                report.phase,
                ReportPhase::Submitted { .. } | ReportPhase::Blocked(_)
            ) && !self.state.rooms.iter().any(|room| room.id == report.room_id)
        });
        if lost {
            let next = self.next_report_id();
            if let Some(report) = &mut self.state.report {
                report.request_id = next;
                report.phase = ReportPhase::Blocked(
                    "You no longer have access to the channel containing this message.".to_owned(),
                );
                report.focus_tick = report.focus_tick.wrapping_add(1);
            }
        }
    }

    pub(super) fn fail_report_request(&mut self, request: u64, message: &str) {
        let Some(report) = &mut self.state.report else {
            return;
        };
        if report.request_id != request {
            return;
        }
        match report.phase {
            ReportPhase::LoadingMenu => {
                report.phase = ReportPhase::MenuError(message.to_owned());
            }
            ReportPhase::Submitting => {
                report.phase = ReportPhase::Selecting;
                report.submit_error = Some(ReportError {
                    kind: ReportErrorKind::Network,
                    message: message.to_owned(),
                });
            }
            _ => {}
        }
    }

    pub(super) fn stop_report_wait(&mut self, message: &str) {
        let Some(report) = &mut self.state.report else {
            return;
        };
        match report.phase {
            ReportPhase::LoadingMenu => {
                report.phase = ReportPhase::MenuError(message.to_owned());
            }
            ReportPhase::Submitting => {
                report.phase = ReportPhase::Selecting;
                report.submit_error = Some(ReportError {
                    kind: ReportErrorKind::Network,
                    message: message.to_owned(),
                });
            }
            _ => {}
        }
    }
}
