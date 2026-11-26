//! Discord V3 in-app message reporting command execution and safe error mapping.

use std::collections::HashMap;

use tokio::sync::mpsc::UnboundedSender;
use yapfast_discord::report as discord_report;

use super::*;
use crate::report::{
    ReportButton, ReportButtonKind, ReportChild, ReportElement, ReportError, ReportErrorKind,
    ReportMenu, ReportNode, ReportOption, ReportSubmission,
};

pub(super) fn command(
    command: Command,
    discord: Result<&Discord, &str>,
    events: &UnboundedSender<Event>,
) -> Option<Command> {
    match command {
        Command::Discord(DiscordCommand::ReportMessageMenu { request }) => {
            let discord = match discord {
                Ok(discord) => discord.clone(),
                Err(why) => {
                    let _ = events.send(Event::ReportMessageMenu {
                        request,
                        result: Err(unavailable_error(why)),
                    });
                    return None;
                }
            };
            let events = events.clone();
            tokio::spawn(async move {
                tracing::debug!(stage = "report_menu", request, "fetching message report menu");
                let result = discord
                    .message_report_menu()
                    .await
                    .map(convert_menu)
                    .map_err(|error| classify_error("report_menu", &error));
                let _ = events.send(Event::ReportMessageMenu { request, result });
            });
            None
        }
        Command::Discord(DiscordCommand::SubmitMessageReport {
            request,
            submission,
        }) => {
            let discord = match discord {
                Ok(discord) => discord.clone(),
                Err(why) => {
                    let _ = events.send(Event::MessageReported {
                        request,
                        result: Err(unavailable_error(why)),
                    });
                    return None;
                }
            };
            let events = events.clone();
            let discord_submission = convert_submission(submission);
            tokio::spawn(async move {
                tracing::debug!(stage = "report_submit", request, "submitting message report");
                let result = discord
                    .submit_message_report(&discord_submission)
                    .await
                    .map_err(|error| classify_error("report_submit", &error));
                let _ = events.send(Event::MessageReported { request, result });
            });
            None
        }
        other => Some(other),
    }
}

fn unavailable_error(why: &str) -> ReportError {
    let kind = if why.contains("not linked") {
        ReportErrorKind::SessionRevoked
    } else {
        ReportErrorKind::Network
    };
    ReportError {
        kind,
        message: why.to_owned(),
    }
}

fn classify_error(stage: &'static str, error: &anyhow::Error) -> ReportError {
    if error
        .downcast_ref::<discord_report::ReportTimeout>()
        .is_some()
    {
        if stage == "report_menu" {
            tracing::warn!(stage, kind = "Network", "Discord report menu request timed out");
            return ReportError {
                kind: ReportErrorKind::Network,
                message: "Timed out loading report options from Discord. Please try again."
                    .to_owned(),
            };
        }
        tracing::warn!(stage, kind = "TimeoutUnknown", "Discord report timed out");
        return ReportError {
            kind: ReportErrorKind::TimeoutUnknown,
            message: "Timed out waiting for Discord. It is unknown whether the report was received."
                .to_owned(),
        };
    }

    let status = error
        .downcast_ref::<yapfast_discord::api::Refused>()
        .map(|refused| refused.0.as_u16());
    let code = yapfast_discord::api::code(error);

    let mapped = if status == Some(401) {
        ReportError {
            kind: ReportErrorKind::SessionRevoked,
            message: "Your Discord session is no longer valid. Link Discord again to submit reports."
                .to_owned(),
        }
    } else if matches!(code, Some(10008 | 10003)) || status == Some(404) {
        ReportError {
            kind: ReportErrorKind::DeletedOrMissing,
            message: "Discord no longer has that message or channel.".to_owned(),
        }
    } else if status == Some(403) || matches!(code, Some(50001 | 50013)) {
        ReportError {
            kind: ReportErrorKind::Forbidden,
            message: "Discord says you do not have permission to report that message.".to_owned(),
        }
    } else if status == Some(429) {
        ReportError {
            kind: ReportErrorKind::RateLimited,
            message: error.to_string(),
        }
    } else if yapfast_discord::api::is_network_error(error) {
        ReportError {
            kind: ReportErrorKind::Network,
            message: "Could not reach Discord. Check your connection and try again.".to_owned(),
        }
    } else if error.to_string().contains("empty report menu") {
        ReportError {
            kind: ReportErrorKind::InvalidMenu,
            message: "Discord returned an empty report menu.".to_owned(),
        }
    } else {
        ReportError {
            kind: ReportErrorKind::Other,
            message: error.to_string(),
        }
    };

    tracing::warn!(
        stage,
        status = ?status,
        code = ?code,
        kind = ?mapped.kind,
        "Discord report request failed"
    );
    mapped
}

fn convert_menu(menu: discord_report::ReportMenu) -> ReportMenu {
    let nodes: HashMap<i64, ReportNode> = menu
        .nodes
        .into_iter()
        .map(|(id, node)| {
            (
                id,
                ReportNode {
                    id: node.id,
                    key: node.key,
                    report_type: node.report_type,
                    header: node.header,
                    subheader: node.subheader,
                    info: node.info,
                    children: node
                        .children
                        .into_iter()
                        .map(|child| ReportChild {
                            label: child.label,
                            target_node_id: child.target_node_id,
                        })
                        .collect(),
                    elements: node
                        .elements
                        .into_iter()
                        .map(|el| ReportElement {
                            name: el.name,
                            kind: el.kind,
                            should_submit_data: el.should_submit_data,
                            options: el
                                .options
                                .into_iter()
                                .map(|opt| ReportOption {
                                    value: opt.value,
                                    label: opt.label,
                                })
                                .collect(),
                        })
                        .collect(),
                    button: node.button.map(|button| ReportButton {
                        kind: match button.kind {
                            discord_report::ReportButtonKind::Next => ReportButtonKind::Next,
                            discord_report::ReportButtonKind::Submit => ReportButtonKind::Submit,
                            discord_report::ReportButtonKind::Done => ReportButtonKind::Done,
                            discord_report::ReportButtonKind::Cancel => ReportButtonKind::Cancel,
                        },
                        target: button.target,
                    }),
                    is_multi_select_required: node.is_multi_select_required,
                    is_auto_submit: node.is_auto_submit,
                },
            )
        })
        .collect();
    ReportMenu {
        name: menu.name,
        version: menu.version,
        variant: menu.variant,
        postback_path: menu.postback_path,
        language: menu.language,
        root_node_id: menu.root_node_id,
        success_node_id: menu.success_node_id,
        fail_node_id: menu.fail_node_id,
        nodes,
    }
}

fn convert_submission(submission: ReportSubmission) -> discord_report::ReportSubmission {
    discord_report::ReportSubmission {
        version: submission.version,
        variant: submission.variant,
        language: submission.language,
        name: submission.name,
        postback_path: submission.postback_path,
        breadcrumbs: submission.breadcrumbs,
        elements: submission.elements,
        channel_id: submission.channel_id,
        message_id: submission.message_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_classification_distinguishes_menu_fetch_from_submit() {
        let menu_err = classify_error("report_menu", &anyhow::anyhow!(discord_report::ReportTimeout));
        assert_eq!(menu_err.kind, ReportErrorKind::Network);
        assert!(
            !menu_err.message.contains("whether the report was received"),
            "menu timeout must not imply submission ambiguity: {}",
            menu_err.message
        );

        let submit_err =
            classify_error("report_submit", &anyhow::anyhow!(discord_report::ReportTimeout));
        assert_eq!(submit_err.kind, ReportErrorKind::TimeoutUnknown);
        assert!(
            submit_err.message.contains("whether the report was received"),
            "submit timeout must warn outcome is unknown: {}",
            submit_err.message
        );
    }
}
