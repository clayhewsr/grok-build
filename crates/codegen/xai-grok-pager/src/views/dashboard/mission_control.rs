use indexmap::IndexMap;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::agent::AgentId;
use crate::app::agent_view::AgentView;
use crate::render::line_utils::truncate_line;
use crate::scrollback::state::VerificationLedgerSnapshot;
use crate::theme::Theme;
use crate::verification::tvl::TvlPublicSnapshot;
use crate::views::agent_status::AgentStatusBar;

fn add_agent_snapshot(agent: &AgentView, snapshot: &mut VerificationLedgerSnapshot) {
    add_snapshot(snapshot, &agent.scrollback.verification_ledger_snapshot());
    for child in agent.subagent_views.values() {
        add_agent_snapshot(child, snapshot);
    }
}

fn add_snapshot(total: &mut VerificationLedgerSnapshot, addition: &VerificationLedgerSnapshot) {
    total.tool_calls += addition.tool_calls;
    total.verified += addition.verified;
    total.failed += addition.failed;
    total.pending += addition.pending;
    for (kind, count) in &addition.kind_counts {
        *total.kind_counts.entry(*kind).or_insert(0) += count;
    }
}

fn collect_mission_control_snapshot(
    agents: &IndexMap<AgentId, AgentView>,
) -> VerificationLedgerSnapshot {
    let mut snapshot = VerificationLedgerSnapshot::default();
    for agent in agents.values() {
        add_agent_snapshot(agent, &mut snapshot);
    }
    snapshot
}

fn chip(label: impl Into<String>, theme: &Theme, color: ratatui::style::Color) -> Line<'static> {
    Line::from(vec![Span::styled(
        label.into(),
        Style::default().fg(color).bg(theme.bg_base),
    )])
}

pub(crate) fn render_mission_control(
    buf: &mut Buffer,
    area: Rect,
    theme: &Theme,
    agents: &IndexMap<AgentId, AgentView>,
    tvl_snapshot: Option<&TvlPublicSnapshot>,
) {
    if area.area() == 0 || area.height == 0 {
        return;
    }

    let snapshot = collect_mission_control_snapshot(agents);
    buf.set_style(area, Style::default().bg(theme.bg_base));

    let mut status = AgentStatusBar::new(theme);
    if snapshot.tool_calls == 0 {
        status.push("ledger", chip("ledger idle", theme, theme.gray_dim));
    } else {
        if snapshot.verified > 0 {
            status.push(
                "verified",
                chip(
                    format!("ok {} verified", snapshot.verified),
                    theme,
                    theme.accent_success,
                ),
            );
        }
        if snapshot.pending > 0 {
            status.push(
                "pending",
                chip(
                    format!("pending {}", snapshot.pending),
                    theme,
                    theme.warning,
                ),
            );
        }
        if snapshot.failed > 0 {
            status.push(
                "failed",
                chip(
                    format!("fail {}", snapshot.failed),
                    theme,
                    theme.accent_error,
                ),
            );
        }
        const TOP_KEYS: [&str; 3] = ["top_1", "top_2", "top_3"];
        for (i, (kind, count)) in snapshot.top_kinds(3).into_iter().enumerate() {
            status.push(
                TOP_KEYS[i],
                chip(
                    format!("{} {}", kind.verb(false), count),
                    theme,
                    theme.text_primary,
                ),
            );
        }
    }
    if let Some(tvl) = tvl_snapshot {
        status.push(
            "tvl",
            chip(
                format!(
                    "TVL {:?} r{}/{}",
                    tvl.status, tvl.round_count, tvl.max_rounds
                ),
                theme,
                theme.text_primary,
            ),
        );
    }

    let right_rects = status.render(buf, area);
    let right_budget = right_rects
        .values()
        .map(|rect| rect.x)
        .min()
        .map(|min_x| min_x.saturating_sub(3).saturating_sub(area.x))
        .unwrap_or(area.width) as usize;

    let mut title_parts = vec!["Mission Control".to_string()];
    if snapshot.tool_calls == 0 {
        title_parts.push("verification ledger idle".to_string());
    } else {
        title_parts.push(format!(
            "{} tool calls · {} verified · {} pending · {} failed",
            snapshot.tool_calls, snapshot.verified, snapshot.pending, snapshot.failed
        ));
    }
    if let Some(tvl) = tvl_snapshot {
        title_parts.push(format!(
            "TVL {:?} · agreement {:?} · unresolved {}",
            tvl.status, tvl.agreement_state, tvl.unresolved_disagreements
        ));
    }

    let title = Line::from(title_parts.join(" · ")).style(
        Style::default()
            .fg(theme.text_primary)
            .bg(theme.bg_base)
            .add_modifier(Modifier::BOLD),
    );
    let title = truncate_line(title, right_budget);
    buf.set_line(area.x, area.y, &title, title.width() as u16);
}
