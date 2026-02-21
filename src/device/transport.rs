use anyhow::{Context, Result};
use hidapi::{HidApi, HidDevice as RawHidDevice};
use std::path::{Path, PathBuf};
use std::time::Duration;

const VID: u16 = 0x3710;
const PID_WIRED: u16 = 0x3414;
const PID_8K_DONGLE: u16 = 0x5406;

const INTERFACE: i32 = 1;
const REPORT_ID: u8 = 0x08;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConnectionMode {
    Wired,
    Wireless,
}

pub struct Device {
    device: RawHidDevice,
    mode: ConnectionMode,
    path: PathBuf,
    // Firmware version string decoded from the USB bcdDevice descriptor,
    // captured during enumeration when HidDeviceInfo is still available.
    // Format: "{major}.{minor:02}", e.g. "2.25" from bcdDevice 0x0225.
    firmware_version: String,
}

impl Device {
    pub fn open() -> Result<Self> {
        let api = HidApi::new().context("Failed to initialize HID API")?;

        let mut selected = None;

        for dev in api.device_list() {
            if dev.vendor_id() == VID
                && (dev.product_id() == PID_WIRED || dev.product_id() == PID_8K_DONGLE)
                && dev.interface_number() == INTERFACE
            {
                selected = Some(dev);
                break;
            }
        }

        let info = selected.context("Pulsar interface 1 not found")?;
        let mode = if info.product_id() == PID_WIRED {
            ConnectionMode::Wired
        } else {
            ConnectionMode::Wireless
        };

        // Capture firmware version from the USB bcdDevice descriptor now.
        // This field is only available on HidDeviceInfo, not on an open HidDevice.
        let firmware_version = bcd_version(info.release_number());

        let device = info
            .open_device(&api)
            .context("Failed to open HID device")?;

        let path = PathBuf::from(info.path().to_string_lossy().to_string());

        Ok(Self {
            device,
            mode,
            path,
            firmware_version,
        })
    }

    // Firmware version decoded from the USB "bcdDevice" descriptor.
    // e.g. "2.25" for a device reporting "bcdDevice 2.25" in lsusb output.
    pub fn firmware_version(&self) -> &str {
        &self.firmware_version
    }

    pub fn write_output(&self, packet: &[u8; 17]) -> Result<()> {
        self.send_raw_packet(packet)
    }

    pub fn drain_input(&self, attempts: usize) {
        for _ in 0..attempts {
            let mut buf = [0u8; 64];
            let _ = self.device.read_timeout(&mut buf, 1);
        }
    }

    pub fn send_command(
        &self,
        command_data: &[u8],
        read_response: bool,
    ) -> Result<Option<Vec<u8>>> {
        let mut packet = [0u8; 17];
        packet[0] = REPORT_ID;

        let len = command_data.len().min(15);
        packet[1..=len].copy_from_slice(&command_data[..len]);

        let sum: u8 = packet[..16].iter().fold(0u8, |acc, &x| acc.wrapping_add(x));
        packet[16] = 0x55u8.wrapping_sub(sum);

        self.send_raw_packet(&packet)?;

        if read_response {
            std::thread::sleep(Duration::from_millis(50));
            Ok(Some(self.read_interrupt(1000)?))
        } else {
            Ok(None)
        }
    }

    pub fn send_raw_packet(&self, packet: &[u8; 17]) -> Result<()> {
        let mut report = [0u8; 65];
        report[0] = packet[0];
        report[1..17].copy_from_slice(&packet[1..]);
        self.device.write(&report).context("HID write failed")?;
        Ok(())
    }

    pub fn read_interrupt(&self, timeout_ms: i32) -> Result<Vec<u8>> {
        let mut buf = [0u8; 64];

        let len = self
            .device
            .read_timeout(&mut buf, timeout_ms)
            .context("HID read failed")?;

        if len == 0 {
            anyhow::bail!("Read timeout");
        }

        Ok(buf[..len].to_vec())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_wired(&self) -> bool {
        self.mode == ConnectionMode::Wired
    }

    pub fn is_wireless(&self) -> bool {
        self.mode == ConnectionMode::Wireless
    }
}

// Decode a USB "bcdDevice" value into a human-readable version string.
//
// The USB spec encodes "bcdDevice" in binary: the high byte is
// the major version and the low byte is the minor version, each nibble being
// a decimal digit.
//
// Examples:
// "0x0225" -> "2.25"
// "0x0100" -> "1.00"
// "0x0310" -> "3.10"
fn bcd_version(bcd: u16) -> String {
    let major_bcd = (bcd >> 8) as u8;
    let minor_bcd = (bcd & 0xFF) as u8;
    let major = (major_bcd >> 4) * 10 + (major_bcd & 0x0F);
    let minor = (minor_bcd >> 4) * 10 + (minor_bcd & 0x0F);
    format!("{}.{:02}", major, minor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bcd_version_decodes_correctly() {
        assert_eq!(bcd_version(0x0225), "2.25"); // Pulsar 8K Dongle
        assert_eq!(bcd_version(0x0100), "1.00");
        assert_eq!(bcd_version(0x0310), "3.10");
        assert_eq!(bcd_version(0x0000), "0.00");
        assert_eq!(bcd_version(0x9999), "99.99");
    }
}
