use serde::{Deserialize, Serialize};
use serialport::{SerialPortType, UsbPortInfo};
use ts_rs::TS;

/// FTDI vendor id; covers the chips Enttec uses (FT232, FT245, etc.).
const FTDI_VID: u16 = 0x0403;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../bindings/")]
pub struct SerialPortInfo {
    pub name: String,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    /// True if VID matches FTDI; Enttec USB Pro is the typical match here.
    pub looks_like_enttec: bool,
}

pub fn list_serial_ports() -> Vec<SerialPortInfo> {
    let Ok(ports) = serialport::available_ports() else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(ports.len());
    for p in ports {
        let mut info = SerialPortInfo {
            name: p.port_name,
            vid: None,
            pid: None,
            manufacturer: None,
            product: None,
            looks_like_enttec: false,
        };
        if let SerialPortType::UsbPort(UsbPortInfo {
            vid,
            pid,
            manufacturer,
            product,
            ..
        }) = p.port_type
        {
            info.vid = Some(vid);
            info.pid = Some(pid);
            info.manufacturer = manufacturer;
            info.product = product;
            info.looks_like_enttec = vid == FTDI_VID;
        }
        out.push(info);
    }
    out
}
