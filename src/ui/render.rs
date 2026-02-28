use ratatui::{prelude::*, widgets::*};
use throbber_widgets_tui::{Throbber, WhichUse, BRAILLE_SIX_DOUBLE};
use tui_slider::{Slider, SliderState};

use super::app::{lod_label, App, SettingRow, DPI_VALUES, POLLING_RATES, SETTINGS_ROWS};
use crate::device::protocol::MouseStatus;

pub fn ui(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    let root = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(1),
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

/// Slider rows occupy 2 lines: label+value on top, bar on bottom.
fn row_height(row: &SettingRow) -> u16 {
    match row {
        SettingRow::Dpi | SettingRow::PollingRate | SettingRow::Debounce => 2,
        _ => 1,
    }
}

fn render_settings(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::bordered()
        .title(" Settings ")
        .border_style(Style::new().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let constraints: Vec<Constraint> = SETTINGS_ROWS
        .iter()
        .map(|r| Constraint::Length(row_height(r)))
        .collect();
    let row_rects = Layout::vertical(constraints).split(inner);
    let s = &app.settings;

    for (i, row) in SETTINGS_ROWS.iter().enumerate() {
        let rect = row_rects[i];
        let selected = i == app.settings_row;
        match row {
            SettingRow::Dpi => {
                let idx = app.dpi_slider.value().round() as usize;
                let value = DPI_VALUES[idx.min(DPI_VALUES.len() - 1)];
                render_slider_row(
                    frame,
                    rect,
                    selected,
                    "DPI",
                    &format!("{}", value),
                    &app.dpi_slider,
                );
            }
            SettingRow::PollingRate => {
                let idx = app.polling_slider.value().round() as usize;
                let value = POLLING_RATES[idx.min(POLLING_RATES.len() - 1)];
                render_slider_row(
                    frame,
                    rect,
                    selected,
                    "Polling Rate",
                    &format!("{} Hz", value.as_hz()),
                    &app.polling_slider,
                );
            }
            SettingRow::LiftOffDistance => render_row(
                frame,
                rect,
                selected,
                "Lift-Off Distance",
                lod_label(s.lod()),
            ),
            SettingRow::Debounce => {
                let val = app.debounce_slider.value().round() as u8;
                render_slider_row(
                    frame,
                    rect,
                    selected,
                    "Debounce",
                    &format!("{} ms", val),
                    &app.debounce_slider,
                );
            }
            SettingRow::AngleSnap => {
                render_toggle(frame, rect, selected, "Angle Snap", s.angle_snap)
            }
            SettingRow::RippleControl => {
                render_toggle(frame, rect, selected, "Ripple Control", s.ripple_control)
            }
            SettingRow::MotionSync => {
                render_toggle(frame, rect, selected, "Motion Sync", s.motion_sync)
            }
            SettingRow::TurboMode => {
                render_toggle(frame, rect, selected, "Turbo Mode", s.turbo_mode)
            }
            SettingRow::NotificationThreshold => render_row(
                frame,
                rect,
                selected,
                "Alert Threshold",
                format!("{}%", s.notification_threshold),
            ),
            SettingRow::BatteryInterval => render_row(
                frame,
                rect,
                selected,
                "Battery Interval",
                interval_label(s.battery_interval_secs),
            ),
        }
    }
}

/// Two-line slider row.
///
/// Top line: cursor glyph (left) + label + value flush-right.
/// Bottom line: tui-slider bar aligned under the label text.
///
/// Selection is shown by a › cursor instead of a background colour.
/// Inactive bars render in DarkGray; active bars: Cyan/DarkGray/White.
fn render_slider_row(
    frame: &mut Frame,
    rect: Rect,
    selected: bool,
    label: &str,
    value_str: &str,
    state: &SliderState,
) {
    let [label_rect, bar_rect] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(rect);

    let cursor = if selected { "›" } else { " " };
    let label_text = format!(" {} {}", cursor, label);
    let value_text = format!("{}  ", value_str);
    let label_fg = if selected { Color::White } else { Color::Reset };
    let value_fg = Color::Cyan;

    let [ll, _, vr] = Layout::horizontal([
        Constraint::Length(label_text.len() as u16),
        Constraint::Min(0),
        Constraint::Length(value_text.len() as u16),
    ])
    .areas(label_rect);

    frame.render_widget(
        Paragraph::new(Span::styled(label_text, Style::new().fg(label_fg))),
        ll,
    );
    frame.render_widget(
        Paragraph::new(Span::styled(value_text, Style::new().fg(value_fg).bold())),
        vr,
    );

    // Bar indented 3 chars to sit under the label text (past " › " prefix).
    let bar_area = Rect {
        x: bar_rect.x + 3,
        width: bar_rect.width.saturating_sub(3),
        ..bar_rect
    };
    let (fc, ec, hc) = if selected {
        (Color::Cyan, Color::DarkGray, Color::White)
    } else {
        (Color::DarkGray, Color::DarkGray, Color::DarkGray)
    };
    frame.render_widget(
        Slider::from_state(state)
            .filled_symbol("━")
            .empty_symbol("─")
            .handle_symbol("●")
            .filled_color(fc)
            .empty_color(ec)
            .handle_color(hc)
            .show_value(false),
        bar_area,
    );
}

fn render_row(frame: &mut Frame, rect: Rect, selected: bool, label: &str, value: String) {
    let cursor = if selected { "›" } else { " " };
    let label_text = format!(" {} {}", cursor, label);
    let value_text = format!("{}  ", value);
    let label_fg = if selected { Color::White } else { Color::Reset };

    let [ll, _, vr] = Layout::horizontal([
        Constraint::Length(label_text.len() as u16),
        Constraint::Min(0),
        Constraint::Length(value_text.len() as u16),
    ])
    .areas(rect);

    frame.render_widget(
        Paragraph::new(Span::styled(label_text, Style::new().fg(label_fg))),
        ll,
    );
    frame.render_widget(
        Paragraph::new(Span::styled(
            value_text,
            Style::new().fg(Color::Cyan).bold(),
        )),
        vr,
    );
}

fn render_toggle(
    frame: &mut Frame,
    rect: Rect,
    selected: bool,
    label: &'static str,
    checked: bool,
) {
    let cursor = if selected { "›" } else { " " };
    let label_text = format!(" {} {}", cursor, label);
    // Checkbox + 2-char right margin to match value padding on other rows.
    let check_text = if checked { "☑ " } else { "☐ " };
    let label_fg = if selected { Color::White } else { Color::Reset };

    let [ll, _, vr] = Layout::horizontal([
        Constraint::Length(label_text.len() as u16),
        Constraint::Min(0),
        Constraint::Length(check_text.len() as u16),
    ])
    .areas(rect);

    frame.render_widget(
        Paragraph::new(Span::styled(label_text, Style::new().fg(label_fg))),
        ll,
    );
    frame.render_widget(
        Paragraph::new(Span::styled(
            check_text,
            Style::new().fg(Color::Cyan).bold(),
        )),
        vr,
    );
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

fn push_status_info(lines: &mut Vec<Line>, app: &App) {
    lines.push(Line::raw(""));
    if let Some(fw) = app.firmware.as_deref() {
        lines.push(kv("  Firmware", fw.to_string()));
    }
    lines.push(kv(
        "  Polling",
        format!("{} Hz", app.settings.polling_rate().as_hz()),
    ));
    lines.push(kv("  DPI", format!("{}", app.dpi)));
}

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
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(inner);
    frame.render_widget(
        Paragraph::new(Span::styled(
            "  Battery  ",
            Style::new().fg(Color::DarkGray),
        )),
        layout[1],
    );
    let lw = "  Battery  ".len() as u16;
    let throbber_area = Rect {
        x: layout[1].x + lw,
        y: layout[1].y,
        width: layout[1].width.saturating_sub(lw),
        height: 1,
    };
    frame.render_stateful_widget(
        Throbber::default()
            .label(" loading...")
            .style(Style::new().fg(Color::Yellow))
            .throbber_style(Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            .throbber_set(BRAILLE_SIX_DOUBLE)
            .use_type(WhichUse::Spin),
        throbber_area,
        &mut app.throbber_state,
    );
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

fn interval_label(secs: u64) -> String {
    match secs {
        s if s < 60 => format!("{}s", s),
        s if s % 60 == 0 => format!("{}m", s / 60),
        s => format!("{}m {}s", s / 60, s % 60),
    }
}
