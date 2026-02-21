use crate::Device;
use anyhow::{Context, Result};
use std::time::Duration;

const CMD_PREPARE: u8 = 0x03;
const CMD_READY: u8 = 0x04;
const CMD_CONFIG: u8 = 0x07;
const CMD_QUERY_STAGE: u8 = 0x08;

const SUB_POLLING_RATE: u8 = 0x00;
const SUB_STAGE_SWITCH: u8 = 0x04;
const SUB_LOD: u8 = 0x0a;
const SUB_DPI_CONFIG: u8 = 0x0a;
const SUB_DEBOUNCE: u8 = 0xa9;
const SUB_MOTION_SYNC: u8 = 0xab;
const SUB_ANGLE_SNAP: u8 = 0xaf;
const SUB_RIPPLE: u8 = 0xb1;
const SUB_TURBO: u8 = 0xb5;

pub const DPI_MIN: u16 = 400;
pub const DPI_MAX: u16 = 12800;

pub const DPI_STAGES: [(u8, u16); 6] = [
    (0, 400),
    (1, 800),
    (2, 1600),
    (3, 3200),
    (4, 6400),
    (5, 12800),
];

// CMD packets
const CMD01_PACKET_A: [u8; 17] = [
    0x08, 0x01, 0x00, 0x00, 0x08, 0x8e, 0x0c, 0x4d, 0x4c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x11,
];
const CMD01_PACKET_B: [u8; 17] = [
    0x08, 0x01, 0x00, 0x00, 0x08, 0x95, 0x05, 0xdd, 0x4b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x82,
];
const CMD02_PACKET: [u8; 17] = [
    0x08, 0x02, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x49,
];
const CMD03_PACKET: [u8; 17] = [
    0x08, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x4A,
];
const CMD04_PACKET: [u8; 17] = [
    0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x49,
];

// CMD04 init sequence
const CMD04_INIT_SEQUENCE: [[u8; 17]; 18] = [
    CMD01_PACKET_A,
    CMD03_PACKET,
    CMD03_PACKET,
    CMD03_PACKET,
    CMD01_PACKET_A,
    CMD03_PACKET,
    CMD03_PACKET,
    CMD01_PACKET_A,
    CMD03_PACKET,
    CMD01_PACKET_B,
    CMD03_PACKET,
    CMD03_PACKET,
    CMD03_PACKET,
    CMD02_PACKET,
    CMD03_PACKET,
    CMD03_PACKET,
    CMD04_PACKET,
    CMD04_PACKET,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseStatus {
    pub battery_level: u8,
    pub is_charging: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PollingRate {
    Hz125 = 0x08,
    Hz250 = 0x04,
    Hz500 = 0x02,
    Hz1000 = 0x01,
    Hz2000 = 0x10,
    Hz4000 = 0x20,
    Hz8000 = 0x40,
}

impl PollingRate {
    pub fn as_hz(&self) -> u16 {
        match self {
            PollingRate::Hz125 => 125,
            PollingRate::Hz250 => 250,
            PollingRate::Hz500 => 500,
            PollingRate::Hz1000 => 1000,
            PollingRate::Hz2000 => 2000,
            PollingRate::Hz4000 => 4000,
            PollingRate::Hz8000 => 8000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiftOffDistance {
    Low,
    Medium,
    High,
}

impl LiftOffDistance {
    fn to_device_values(&self) -> (u8, u8) {
        match self {
            LiftOffDistance::Low => (0x03, 0x52),
            LiftOffDistance::Medium => (0x01, 0x54),
            LiftOffDistance::High => (0x02, 0x53),
        }
    }
}

fn send_prepare_sequence(device: &Device) -> Result<()> {
    let prepare_cmd = [CMD_PREPARE, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    device.send_command(&prepare_cmd, false)?;
    std::thread::sleep(Duration::from_millis(50));

    let ready_cmd = [CMD_READY, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    device.send_command(&ready_cmd, false)?;
    std::thread::sleep(Duration::from_millis(50));

    Ok(())
}

/// Get firmware version from USB descriptor
/// Return the firmware version read from the USB bcdDevice descriptor.
///
/// Captured at Device::open() from HidDeviceInfo.
pub fn get_firmware(device: &Device) -> Result<String> {
    Ok(device.firmware_version().to_string())
}

fn read_cmd(device: &Device, expected_cmd: u8, timeout_ms: i32) -> Result<Option<Vec<u8>>> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms as u64);

    while std::time::Instant::now() < deadline {
        match device.read_interrupt(250) {
            Ok(data) => {
                if data.len() < 7 || data[0] != 0x08 {
                    continue;
                }

                if data[1] != expected_cmd {
                    continue;
                }

                return Ok(Some(data));
            }
            Err(_) => continue,
        }
    }

    Ok(None)
}

pub fn get_mouse_battery(device: &Device) -> Result<MouseStatus> {
    device.drain_input(6);

    device.write_output(&CMD04_PACKET)?;

    if let Some(payload) = read_cmd(device, 0x04, 800)? {
        return Ok(MouseStatus {
            battery_level: payload[6],
            is_charging: payload[7] != 0,
        });
    }

    for pkt in &CMD04_INIT_SEQUENCE {
        device.write_output(pkt)?;
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    if let Some(payload) = read_cmd(device, 0x04, 2000)? {
        return Ok(MouseStatus {
            battery_level: payload[6],
            is_charging: payload[7] != 0,
        });
    }

    anyhow::bail!("Failed to read battery after init sequence")
}

pub fn query_current_stage(device: &Device) -> Result<u8> {
    let command = [
        CMD_QUERY_STAGE,
        0x00,
        0x00,
        0x04,
        0x01,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
    ];

    let response = device
        .send_command(&command, true)?
        .context("No stage query response")?;

    if response.len() < 7 {
        anyhow::bail!("Invalid stage response");
    }

    let device_stage = response[6];
    let user_stage = device_stage + 1;

    Ok(user_stage)
}

pub fn query_current_dpi(device: &Device) -> Result<u16> {
    let stage = query_current_stage(device)?;
    Ok(DPI_STAGES[(stage - 1) as usize].1)
}

fn configure_dpi_stage(device: &Device, user_stage: u8, dpi: u16) -> Result<()> {
    if !(1..=6).contains(&user_stage) {
        anyhow::bail!("Stage must be 1-6");
    }

    if !(DPI_MIN..=DPI_MAX).contains(&dpi) {
        anyhow::bail!("DPI must be between {} and {}", DPI_MIN, DPI_MAX);
    }

    let device_stage = user_stage - 1;
    let dpi_value = (dpi / 10) as u8;

    let command = [
        CMD_CONFIG,
        0x00,
        0x00,
        SUB_DPI_CONFIG,
        0x02,
        device_stage,
        dpi_value,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
    ];

    device.send_command(&command, false)?;
    Ok(())
}

fn switch_to_dpi_stage(device: &Device, user_stage: u8) -> Result<()> {
    if !(1..=6).contains(&user_stage) {
        anyhow::bail!("Stage must be 1-6");
    }

    let device_stage = user_stage - 1;
    let value = 0x55u8.wrapping_sub(device_stage);

    let command = [
        CMD_CONFIG,
        0x00,
        0x00,
        SUB_STAGE_SWITCH,
        0x02,
        device_stage,
        value,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
    ];

    device.send_command(&command, false)?;
    Ok(())
}

pub fn set_dpi(device: &Device, dpi: u16) -> Result<()> {
    let dpi = dpi.clamp(DPI_MIN, DPI_MAX);

    let mut closest_stage = 1u8;
    let mut closest_dpi = DPI_STAGES[0].1;
    let mut min_diff = (dpi as i32 - closest_dpi as i32).abs();

    for (idx, &(_, stage_dpi)) in DPI_STAGES.iter().enumerate() {
        let diff = (dpi as i32 - stage_dpi as i32).abs();
        if diff < min_diff {
            min_diff = diff;
            closest_stage = (idx + 1) as u8;
            closest_dpi = stage_dpi;
        }
    }

    send_prepare_sequence(device)?;
    configure_dpi_stage(device, closest_stage, closest_dpi)?;
    std::thread::sleep(Duration::from_millis(50));
    switch_to_dpi_stage(device, closest_stage)?;

    Ok(())
}

pub fn set_polling_rate(device: &Device, rate: PollingRate) -> Result<()> {
    let rate_byte = rate as u8;
    let value = 0x55u8.wrapping_sub(rate_byte);

    send_prepare_sequence(device)?;

    let command = [
        CMD_CONFIG,
        0x00,
        0x00,
        SUB_POLLING_RATE,
        0x02,
        rate_byte,
        value,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
    ];

    device.send_command(&command, false)?;
    Ok(())
}

pub fn set_lod(device: &Device, lod: LiftOffDistance) -> Result<()> {
    let (stage, value) = lod.to_device_values();

    send_prepare_sequence(device)?;

    let command = [
        CMD_CONFIG, 0x00, 0x00, SUB_LOD, 0x02, stage, value, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
    ];

    device.send_command(&command, false)?;
    Ok(())
}

pub fn set_debounce(device: &Device, milliseconds: u8) -> Result<()> {
    if milliseconds > 20 {
        anyhow::bail!("Debounce must be 0-20ms");
    }

    let value = 0x55u8.wrapping_sub(milliseconds);

    send_prepare_sequence(device)?;

    let command = [
        CMD_CONFIG,
        0x00,
        0x00,
        SUB_DEBOUNCE,
        0x02,
        milliseconds,
        value,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
    ];
    device.send_command(&command, false)?;
    Ok(())
}

fn set_toggle_feature(device: &Device, feature: u8, enabled: bool) -> Result<()> {
    let (stage, value) = if enabled { (0x01, 0x54) } else { (0x00, 0x55) };

    send_prepare_sequence(device)?;

    let command = [
        CMD_CONFIG, 0x00, 0x00, feature, 0x02, stage, value, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
    ];

    device.send_command(&command, false)?;
    Ok(())
}

pub fn get_device_info(device: &Device) -> DeviceInfo {
    DeviceInfo {
        firmware: get_firmware(device).ok(),
        current_dpi: query_current_dpi(device).ok(),
        battery: get_mouse_battery(device).ok(),
    }
}

pub struct DeviceInfo {
    pub firmware: Option<String>,
    pub current_dpi: Option<u16>,
    pub battery: Option<MouseStatus>,
}

pub fn set_angle_snap(device: &Device, enabled: bool) -> Result<()> {
    set_toggle_feature(device, SUB_ANGLE_SNAP, enabled)
}

pub fn set_ripple_control(device: &Device, enabled: bool) -> Result<()> {
    set_toggle_feature(device, SUB_RIPPLE, enabled)
}

pub fn set_turbo_mode(device: &Device, enabled: bool) -> Result<()> {
    set_toggle_feature(device, SUB_TURBO, enabled)
}

pub fn set_motion_sync(device: &Device, enabled: bool) -> Result<()> {
    set_toggle_feature(device, SUB_MOTION_SYNC, enabled)
}
