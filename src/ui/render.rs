use ratatui::{prelude::*, widgets::*};
use throbber_widgets_tui::{Throbber, WhichUse, BRAILLE_SIX_DOUBLE};

use super::app::{lod_label, App, DPI_VALUES};
use crate::device::protocol::MouseStatus;

pub fn ui(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    let root = Layout::vertical([
        Constraint::Length(3), // header
        Constraint::Min(0),    // content
        Constraint::Length(1), // footer
    ])
    .split(area);

    render_header(frame, app, root[0]);

    let columns =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(root[1]);

    render_settings(frame, app, columns[0]);
    render_status(frame, app, columns[1]);
    render_footer(frame, app, root[2]);
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    // Use the cached fields, no mutex lock in the render loop.
    let (conn, path) = if app.device.is_some() {
        (
            if app.device_wired {
                "wired"
            } else {
                "wireless"
            },
            app.device_path.as_str(),
        )
    } else {
        ("not connected", "")
    };

    let fw = app.firmware.as_deref().unwrap_or("—");

    let line = Line::from(vec![
        Span::styled("LIGHTCRAZY", Style::new().bold()),
        Span::raw("   "),
        Span::styled(conn, Style::new().fg(Color::Cyan)),
        if !path.is_empty() {
            Span::styled(format!("  {}", path), Style::new().fg(Color::DarkGray))
        } else {
            Span::raw("")
        },
        Span::styled(format!("   fw {}", fw), Style::new().fg(Color::DarkGray)),
    ]);

    frame.render_widget(
        Paragraph::new(line)
            .block(Block::bordered().border_style(Style::new().fg(Color::DarkGray))),
        area,
    );
}

fn render_settings(frame: &mut Frame, app: &App, area: Rect) {
    let s = &app.settings;

    let rows: Vec<ListItem> = vec![
        setting_item(
            0,
            app.settings_row,
            "DPI",
            format!("{}", DPI_VALUES[app.dpi_stage]),
        ),
        setting_item(
            1,
            app.settings_row,
            "Polling Rate",
            format!("{} Hz", s.polling_rate().as_hz()),
        ),
        setting_item(2, app.settings_row, "Lift-Off Distance", lod_label(s.lod())),
        setting_item(
            3,
            app.settings_row,
            "Debounce",
            format!("{} ms", s.debounce_ms),
        ),
        setting_toggle(4, app.settings_row, "Angle Snap", s.angle_snap),
        setting_toggle(5, app.settings_row, "Ripple Control", s.ripple_control),
        setting_toggle(6, app.settings_row, "Motion Sync", s.motion_sync),
        setting_toggle(7, app.settings_row, "Turbo Mode", s.turbo_mode),
    ];

    frame.render_widget(
        List::new(rows).block(
            Block::bordered()
                .title(" Settings ")
                .border_style(Style::new().fg(Color::DarkGray)),
        ),
        area,
    );
}

fn setting_item(idx: usize, selected_idx: usize, label: &str, value: String) -> ListItem<'static> {
    let selected = idx == selected_idx;
    let (ls, vs) = if selected {
        (
            Style::new().fg(Color::Black).bg(Color::White),
            Style::new().fg(Color::Black).bg(Color::White).bold(),
        )
    } else {
        (Style::new(), Style::new().fg(Color::Cyan))
    };

    ListItem::new(Line::from(vec![
        Span::styled(format!("  {:<20}", label), ls),
        Span::styled(value, vs),
    ]))
}

fn setting_toggle(
    idx: usize,
    selected_idx: usize,
    label: &'static str,
    checked: bool,
) -> ListItem<'static> {
    let selected = idx == selected_idx;
    let checkbox = if checked { "☑ " } else { "☐ " };
    let (ls, cs) = if selected {
        (
            Style::new().fg(Color::Black).bg(Color::White),
            Style::new().fg(Color::Black).bg(Color::White).bold(),
        )
    } else {
        (Style::new(), Style::new().fg(Color::Cyan))
    };

    ListItem::new(Line::from(vec![
        Span::styled(format!("  {:<20}", label), ls),
        Span::styled(checkbox, cs),
    ]))
}

fn render_status(frame: &mut Frame, app: &mut App, area: Rect) {
    let mut lines: Vec<Line> = vec![Line::raw("")];

    if app.device.is_none() {
        lines.push(Line::from(Span::styled(
            "  No device found",
            Style::new().fg(Color::Red).bold(),
        )));
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "  Connect via USB or 8K dongle",
            Style::new().fg(Color::DarkGray),
        )));
        frame.render_widget(
            Paragraph::new(lines).block(
                Block::bordered()
                    .title(" Status ")
                    .border_style(Style::new().fg(Color::DarkGray)),
            ),
            area,
        );
        return;
    }

    if app.battery_loading {
        render_battery_throbber(frame, app, area, &mut lines);
    } else {
        render_battery_lines(&mut lines, app.battery.as_ref());
        push_status_info(&mut lines, app);
        frame.render_widget(
            Paragraph::new(lines).block(
                Block::bordered()
                    .title(" Status ")
                    .border_style(Style::new().fg(Color::DarkGray)),
            ),
            area,
        );
    }
}

/// Shared helper: appends firmware, polling rate, and DPI lines.
fn push_status_info(lines: &mut Vec<Line>, app: &App) {
    lines.push(Line::raw(""));
    if let Some(fw) = app.firmware.as_deref() {
        lines.push(kv("  Firmware", fw.to_string()));
    }
    lines.push(kv(
        "  Polling",
        format!("{} Hz", app.settings.polling_rate().as_hz()),
    ));
    lines.push(kv(
        "  Current DPI",
        format!("{}", DPI_VALUES[app.dpi_stage]),
    ));
}

/// Renders the status panel with a live throbber while battery is loading.
fn render_battery_throbber(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    extra_lines: &mut Vec<Line>,
) {
    let block = Block::bordered()
        .title(" Status ")
        .border_style(Style::new().fg(Color::DarkGray));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let layout = Layout::vertical([
        Constraint::Length(1), // blank
        Constraint::Length(1), // "Battery  [throbber] loading..."
        Constraint::Min(0),    // remaining info
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(Span::styled(
            "  Battery  ",
            Style::new().fg(Color::DarkGray),
        )),
        layout[1],
    );

    let label_width = "  Battery  ".len() as u16;
    let throbber_area = Rect {
        x: layout[1].x + label_width,
        y: layout[1].y,
        width: layout[1].width.saturating_sub(label_width),
        height: 1,
    };

    let throbber = Throbber::default()
        .label(" loading...")
        .style(Style::new().fg(Color::Yellow))
        .throbber_style(Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .throbber_set(BRAILLE_SIX_DOUBLE)
        .use_type(WhichUse::Spin);

    frame.render_stateful_widget(throbber, throbber_area, &mut app.throbber_state);

    push_status_info(extra_lines, app);
    frame.render_widget(Paragraph::new(std::mem::take(extra_lines)), layout[2]);
}

fn render_battery_lines(lines: &mut Vec<Line>, battery: Option<&MouseStatus>) {
    match battery {
        Some(b) => {
            let color = battery_color(b.battery_level);
            let charging = if b.is_charging { " ⚡" } else { "" };

            lines.push(Line::from(vec![
                Span::styled("  Battery  ", Style::new().fg(Color::DarkGray)),
                Span::styled(
                    format!("{}%{}", b.battery_level, charging),
                    Style::new().fg(color).bold(),
                ),
            ]));

            let filled = (b.battery_level as usize * 30 / 100).min(30);
            lines.push(Line::from(Span::styled(
                format!("  {}{}", "█".repeat(filled), "░".repeat(30 - filled)),
                Style::new().fg(color),
            )));
        }
        None => {
            lines.push(Line::from(Span::styled(
                "  Battery  unavailable",
                Style::new().fg(Color::DarkGray),
            )));
        }
    }
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let (msg, style) = match &app.status_msg {
        Some((text, true, _)) => (text.as_str(), Style::new().fg(Color::Red)),
        Some((text, false, _)) => (text.as_str(), Style::new().fg(Color::Green)),
        None => (
            "↑↓: navigate  ←→: adjust  Enter: apply  q: quit",
            Style::new().fg(Color::DarkGray),
        ),
    };

    frame.render_widget(Paragraph::new(Line::from(Span::styled(msg, style))), area);
}

fn kv(key: &'static str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(key, Style::new().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::raw(value),
    ])
}

fn battery_color(level: u8) -> Color {
    if level <= 10 {
        Color::Red
    } else if level <= 25 {
        Color::Yellow
    } else {
        Color::Green
    }
}
