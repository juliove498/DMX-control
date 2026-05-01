//! Live MIDI hub: holds the active input/output connections and exposes
//! a small API for commands. The `midir` connections must be kept alive
//! to keep the receive callback running, so `MidiHub` parks them in
//! Option fields and drops them on disconnect.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};

use super::{MidiDeviceInfo, MidiMessage, MidiStatus};

pub const MIDI_EVENT: &str = "midi:message";

/// In-process router for MIDI input. Surfaces (e.g. the Launchpad
/// controller) install a router so they can react to pad presses without
/// the round-trip through the Tauri event bus. Shared via `Arc` so the
/// midir input thread can clone it out from under the hub lock and call
/// it without holding any other lock.
pub type InputRouter = Arc<dyn Fn(&MidiMessage) + Send + Sync + 'static>;

pub type SharedMidi = Arc<Mutex<MidiHub>>;

pub struct MidiHub {
    /// Held to keep the input callback alive; we don't read from it.
    input_conn: Option<MidiInputConnection<()>>,
    output_conn: Option<MidiOutputConnection>,
    connected_name: Option<String>,
    last_event: Option<MidiMessage>,
    input_router: Option<InputRouter>,
}

impl Default for MidiHub {
    fn default() -> Self {
        Self {
            input_conn: None,
            output_conn: None,
            connected_name: None,
            last_event: None,
            input_router: None,
        }
    }
}

pub fn shared_midi() -> SharedMidi {
    Arc::new(Mutex::new(MidiHub::default()))
}

/// Enumerate every device the OS sees on either side. Names are merged so
/// a device that shows up on both input and output appears once with both
/// flags set.
pub fn list_devices() -> Vec<MidiDeviceInfo> {
    let mut by_name: std::collections::BTreeMap<String, MidiDeviceInfo> = Default::default();
    if let Ok(input) = MidiInput::new("dmx-control-list-in") {
        for port in input.ports() {
            if let Ok(name) = input.port_name(&port) {
                by_name
                    .entry(name.clone())
                    .or_insert(MidiDeviceInfo {
                        name,
                        has_input: false,
                        has_output: false,
                    })
                    .has_input = true;
            }
        }
    }
    if let Ok(output) = MidiOutput::new("dmx-control-list-out") {
        for port in output.ports() {
            if let Ok(name) = output.port_name(&port) {
                by_name
                    .entry(name.clone())
                    .or_insert(MidiDeviceInfo {
                        name,
                        has_input: false,
                        has_output: false,
                    })
                    .has_output = true;
            }
        }
    }
    by_name.into_values().collect()
}

impl MidiHub {
    pub fn status(&self) -> MidiStatus {
        MidiStatus {
            connected: self.connected_name.clone(),
            has_output: self.output_conn.is_some(),
            last_event: self.last_event.clone(),
        }
    }

    pub fn disconnect(&mut self) {
        self.input_conn = None;
        self.output_conn = None;
        self.connected_name = None;
        // Routers are tied to the active surface; clearing them on
        // disconnect prevents pad presses on a future re-connect from
        // dispatching to a stale handler.
        self.input_router = None;
    }

    pub fn set_input_router(&mut self, router: Option<InputRouter>) {
        self.input_router = router;
    }

    /// Open input and output ports of the given device by name. Some
    /// devices expose only one side; we surface that via `has_output`
    /// in the status without making it an error.
    pub fn connect(
        &mut self,
        name: &str,
        app: AppHandle,
        shared: SharedMidi,
    ) -> Result<(), String> {
        // Drop the previous connections cleanly before opening new ones —
        // some platforms are picky about a port being held twice.
        self.disconnect();

        // ---- Input ------------------------------------------------------
        let mut midi_in =
            MidiInput::new("dmx-control-in").map_err(|e| format!("MidiInput::new: {e}"))?;
        midi_in.ignore(midir::Ignore::None);
        let in_port = midi_in
            .ports()
            .into_iter()
            .find(|p| midi_in.port_name(p).map(|n| n == name).unwrap_or(false));

        if let Some(in_port) = in_port {
            let app_for_cb = app.clone();
            let shared_for_cb = shared.clone();
            let in_conn = midi_in
                .connect(
                    &in_port,
                    "dmx-control-in",
                    move |_t, msg, _| {
                        let parsed = parse_message(msg);
                        // Stash the most recent event so a polling client
                        // (status query) can read it without subscribing.
                        // Clone the router out under the lock and call it
                        // afterwards — calling user code while holding the
                        // hub lock would deadlock anything that re-enters.
                        let router = {
                            let mut g = shared_for_cb.lock();
                            g.last_event = Some(parsed.clone());
                            g.input_router.clone()
                        };
                        // Push-style stream for live UIs.
                        let _ = app_for_cb.emit(MIDI_EVENT, &parsed);
                        if let Some(router) = router {
                            router(&parsed);
                        }
                    },
                    (),
                )
                .map_err(|e| format!("input connect: {e}"))?;
            self.input_conn = Some(in_conn);
        }

        // ---- Output -----------------------------------------------------
        let midi_out =
            MidiOutput::new("dmx-control-out").map_err(|e| format!("MidiOutput::new: {e}"))?;
        let out_port = midi_out
            .ports()
            .into_iter()
            .find(|p| midi_out.port_name(p).map(|n| n == name).unwrap_or(false));
        if let Some(out_port) = out_port {
            self.output_conn = Some(
                midi_out
                    .connect(&out_port, "dmx-control-out")
                    .map_err(|e| format!("output connect: {e}"))?,
            );
        }

        if self.input_conn.is_none() && self.output_conn.is_none() {
            return Err(format!(
                "no input or output port found with name {name}"
            ));
        }

        self.connected_name = Some(name.to_string());
        Ok(())
    }

    pub fn send_raw(&mut self, bytes: &[u8]) -> Result<(), String> {
        let conn = self
            .output_conn
            .as_mut()
            .ok_or_else(|| "no output connection".to_string())?;
        conn.send(bytes).map_err(|e| format!("midi send: {e}"))
    }
}

fn parse_message(msg: &[u8]) -> MidiMessage {
    let status = msg.first().copied().unwrap_or(0);
    let data1 = msg.get(1).copied();
    let data2 = msg.get(2).copied();
    let label = describe(msg);
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    MidiMessage {
        status,
        data1,
        data2,
        label,
        timestamp_ms,
    }
}

fn describe(msg: &[u8]) -> String {
    if msg.is_empty() {
        return "<empty>".into();
    }
    let status = msg[0];
    let high = status & 0xF0;
    let ch = (status & 0x0F) + 1;
    let d1 = msg.get(1).copied().unwrap_or(0);
    let d2 = msg.get(2).copied().unwrap_or(0);
    match high {
        0x80 => format!("note_off ch={ch} note={d1} vel={d2}"),
        0x90 if d2 == 0 => format!("note_off ch={ch} note={d1}"),
        0x90 => format!("note_on ch={ch} note={d1} vel={d2}"),
        0xA0 => format!("aftertouch ch={ch} note={d1} val={d2}"),
        0xB0 => format!("cc ch={ch} cc={d1} val={d2}"),
        0xC0 => format!("program ch={ch} prog={d1}"),
        0xD0 => format!("pressure ch={ch} val={d1}"),
        0xE0 => format!("pitch ch={ch} lsb={d1} msb={d2}"),
        0xF0 => format!("sysex ({} bytes)", msg.len()),
        _ => format!("raw {:02x?}", msg),
    }
}
