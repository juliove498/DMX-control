//! Cross-platform "keep the system awake" guard. Holds a power
//! assertion for the lifetime of the value; drops the assertion when
//! the value is dropped. Lives in app state so the assertion stays
//! alive as long as the Tauri app is running.
//!
//! macOS: spawns `caffeinate` as a child process. Apple's own tool —
//! it manages the IOPMAssertion lifecycle for us and is bulletproof
//! against arbitrary entitlement issues a release-signed app might
//! have. Killed explicitly on drop.
//!
//! Other platforms: no-op for now. A live show on Windows/Linux can
//! still set the OS sleep policy manually; we'll wire native power
//! requests there if someone asks for it.

#[cfg(target_os = "macos")]
mod imp {
    use std::process::{Child, Command, Stdio};

    pub struct SleepInhibitor {
        child: Option<Child>,
    }

    impl SleepInhibitor {
        /// Block idle + disk sleep so the output thread keeps streaming
        /// DMX even after the operator stops touching the laptop.
        /// `-d` (display) is intentionally absent — the operator can
        /// dim their screen during a quiet moment and the rig will
        /// keep running.
        pub fn new() -> Self {
            let child = Command::new("/usr/bin/caffeinate")
                .args(["-i", "-m"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| {
                    tracing::warn!(error = %e, "could not spawn caffeinate; system sleep is NOT inhibited");
                })
                .ok();
            if child.is_some() {
                tracing::info!("system sleep inhibited via caffeinate -i -m");
            }
            Self { child }
        }
    }

    impl Drop for SleepInhibitor {
        fn drop(&mut self) {
            if let Some(mut child) = self.child.take() {
                // SIGKILL on the child releases the power assertion
                // immediately. `wait` reaps the zombie so the process
                // table stays clean — caffeinate exits in <10 ms so
                // we don't block shutdown noticeably.
                let _ = child.kill();
                let _ = child.wait();
                tracing::info!("system sleep inhibitor released");
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    #[derive(Default)]
    pub struct SleepInhibitor;
    impl SleepInhibitor {
        pub fn new() -> Self {
            // No-op stub. Wire OS-specific power requests here if/when
            // the app ships on Windows or Linux.
            Self
        }
    }
}

pub use imp::SleepInhibitor;
