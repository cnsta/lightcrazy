use ratatui::{prelude::*, widgets::*};
use throbber_widgets_tui::{Throbber, WhichUse, BRAILLE_SIX_DOUBLE};
use tui_slider::{Slider, SliderState};

use super::app::{lod_label, App, SettingRow, DPI_VALUES, POLLING_RATES, SETTINGS_ROWS};

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
    render_info(frame, app, columns[1]);
    render_footer(frame, app, root[2]);
}

fn render_header(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::bordered().border_style(Style::new().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    const BATTERY_COL: u16 = 30;
    let [left, right] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(BATTERY_COL)]).areas(inner);

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
    let left_line = Line::from(vec![
        Span::styled(" LIGHTCRAZY", Style::new().bold()),
        Span::raw("   "),
        Span::styled(conn, Style::new().fg(Color::Cyan)),
        if !path.is_empty() {
            Span::styled(format!("  {}", path), Style::new().fg(Color::DarkGray))
        } else {
            Span::raw("")
        },
        Span::styled(format!("   fw {}", fw), Style::new().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(left_line), left);
    render_header_battery(frame, app, right);
}

fn render_header_battery(frame: &mut Frame, app: &mut App, area: Rect) {
    match &app.battery {
        Some(b) => {
            let level = b.battery_level;
            let charging = b.is_charging;
            let color = battery_color(level);
            let pct_str = if charging {
                format!("{:3} % ⚡ ", level)
            } else {
                format!("{:3} % ", level)
            };
            let bar_width = (area.width as usize).saturating_sub(pct_str.len());
            let filled = (level as usize * bar_width / 100).min(bar_width);
            let empty = bar_width - filled;
            let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(bar, Style::new().fg(color)),
                    Span::styled(pct_str, Style::new().fg(color).bold()),
                ])),
                area,
            );
        }
        None if app.battery_loading => {
            let prefix = "battery ";
            let [pl, thr] =
                Layout::horizontal([Constraint::Length(prefix.len() as u16), Constraint::Min(0)])
                    .areas(area);
            frame.render_widget(
                Paragraph::new(Span::styled(prefix, Style::new().fg(Color::DarkGray))),
                pl,
            );
            frame.render_stateful_widget(
                Throbber::default()
                    .style(Style::new().fg(Color::Yellow))
                    .throbber_style(Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                    .throbber_set(BRAILLE_SIX_DOUBLE)
                    .use_type(WhichUse::Spin),
                thr,
                &mut app.throbber_state,
            );
        }
        None => {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    "no battery data",
                    Style::new().fg(Color::DarkGray),
                )),
                area,
            );
        }
    }
}

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
            SettingRow::LiftOffDistance => render_row(
                frame,
                rect,
                selected,
                "Lift-Off Distance",
                lod_label(s.lod()),
            ),
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
                format!("{} %", s.notification_threshold),
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
        Paragraph::new(Span::styled(
            value_text,
            Style::new().fg(Color::Cyan).bold(),
        )),
        vr,
    );

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

fn render_info(frame: &mut Frame, app: &App, area: Rect) {
    let row = SETTINGS_ROWS[app.settings_row];
    let (title, body) = setting_description(row);
    frame.render_widget(
        Paragraph::new(body)
            .block(
                Block::bordered()
                    .title(format!(" {} ", title))
                    .border_style(Style::new().fg(Color::DarkGray)),
            )
            .wrap(Wrap { trim: true })
            .style(Style::new().fg(Color::DarkGray)),
        area,
    );
}

fn setting_description(row: SettingRow) -> (&'static str, &'static str) {
    match row {
        SettingRow::Dpi => (
            "DPI",
            "DPI (dots per inch) controls how far the cursor travels on screen \
relative to physical mouse movement. Higher values move the cursor further \
with less effort, which suits large monitors or fast-paced scenarios. \
Lower values give more precise control over small movements.\n\n\
The X2 supports six presets: 400, 800, 1600, 3200, 6400, and 12800. Use \
left/right to position the slider, then press Enter to apply the value \
to the sensor.",
        ),
        SettingRow::PollingRate => (
            "Polling Rate",
            "Polling rate is how many times per second the mouse reports its \
position to the computer. Higher rates reduce the delay between physical \
movement and the on-screen response. 8000 Hz means one report every \
0.125 ms, versus 8 ms at 125 Hz.\n\n\
4K Hz requires the dedicated 4K receiver. 8K Hz requires the 8K dongle. \
Both also require a compatible host port. The improvement is most \
perceptible during fast, precise movements.",
        ),
        SettingRow::Debounce => (
            "Debounce Time",
            "Debounce sets the minimum time that must pass between two clicks \
registering as separate events. Mechanical switches can produce brief \
electrical noise on actuation, which may read as an unintended double-click \
at 0 ms.\n\n\
2-4 ms absorbs most switch variance without any perceptible delay. Higher \
values add a slight but measurable lag to click registration. Set to 0 only \
if your switches are clean and you need the fastest possible response.",
        ),
        SettingRow::LiftOffDistance => (
            "Lift-Off Distance",
            "Lift-off distance is the height above the surface at which the sensor \
stops tracking when the mouse is lifted.\n\n\
Low (0.7 mm) is ideal for players who frequently reposition — the sensor \
disengages almost immediately on lift, preventing cursor drift. Medium \
(1 mm) suits most surfaces and is the default. High (2 mm) helps on thick \
mouse feet or textured and reflective mousepads where the sensor may \
struggle to lock at very low clearance.",
        ),
        SettingRow::AngleSnap => (
            "Angle Snapping",
            "Angle snapping applies a filter that pulls near-horizontal and \
near-vertical cursor movements toward a perfectly straight line. You can \
test the effect by slowly drawing a nearly-straight line in a paint program \
— with angle snapping on it will snap to the axis.\n\n\
Useful in productivity contexts where straight strokes matter. Most users \
leave it off in games, as it subtly alters diagonal movement direction and \
can interfere with precise aim.",
        ),
        SettingRow::RippleControl => (
            "Ripple Control",
            "Ripple control smooths out jagged micro-movements in fast cursor \
strokes, reducing the irregular edges that appear at high speed and DPI. \
Like angle snapping, the effect is visible when drawing fast lines in a \
paint application.\n\n\
Most relevant for graphic and design work. In games it is generally left \
off, as the additional filtering can soften fine motor input.",
        ),
        SettingRow::MotionSync => (
            "Motion Sync",
            "Motion sync aligns the sensor's data transmission to the exact \
moment the computer polls for mouse position. Without it, sensor samples \
and polling cycles are slightly out of phase, introducing minor variability \
in when data arrives.\n\n\
With motion sync on, each position report is sent at the start of the \
polling window. This does not reduce average latency but decreases its \
variance, producing more consistent input timing — most noticeable at \
high polling rates.",
        ),
        SettingRow::TurboMode => (
            "Turbo Mode",
            "Turbo mode locks the sensor's internal scanning rate at 20,000 FPS \
regardless of DPI or polling rate. Normally the sensor scales its frame \
rate with those settings — at low DPI and polling rate it can drop well \
below 10K FPS, reducing tracking fidelity during fast movements.\n\n\
With turbo mode on, the sensor always operates at peak rate, ensuring \
maximum tracking accuracy under all conditions. The trade-off is \
meaningfully higher power consumption, which shortens battery life \
between charges.",
        ),
        SettingRow::NotificationThreshold => (
            "Alert Threshold",
            "The battery percentage at which a desktop notification is sent \
warning that the mouse needs charging. Adjustable in 5% steps between \
5% and 50%.\n\n\
Set higher if you want an early warning and time to locate a cable. Set \
lower if you charge on a fixed routine and want to avoid frequent \
notifications during normal use.",
        ),
        SettingRow::BatteryInterval => (
            "Battery Interval",
            "How often the background tray service polls the mouse for its \
current battery level. Shorter intervals keep the tray icon more up to \
date but cause the device to wake from idle slightly more often.\n\n\
60 seconds is a good default. 30 seconds is reasonable if you charge \
frequently and want accurate readings. Changes take effect the next time \
the tray service starts.",
        ),
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
        s if s < 60 => format!("{} s", s),
        s if s % 60 == 0 => format!("{} m", s / 60),
        s => format!("{} m {} s", s / 60, s % 60),
    }
}
