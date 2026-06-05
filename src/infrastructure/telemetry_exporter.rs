use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use crate::application::telemetry::Telemetry;

pub struct TelemetryExporter {
    telemetry: Arc<Telemetry>,
    interval: Duration,
    shutdown: Arc<AtomicBool>,
}

impl TelemetryExporter {
    pub fn new(telemetry: Arc<Telemetry>, interval: Duration, shutdown: Arc<AtomicBool>) -> Self {
        Self { telemetry, interval, shutdown }
    }

    pub fn start(self) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            let mut last_queued = 0;
            let mut last_matches = 0;

            while !self.shutdown.load(Ordering::Relaxed) {
                thread::sleep(self.interval);

                let queued = self.telemetry.players_queued.load(Ordering::Relaxed);
                let matches = self.telemetry.matches_formed.load(Ordering::Relaxed);
                

                let dq = queued.saturating_sub(last_queued);
                let dm = matches.saturating_sub(last_matches);

                if dq > 0 || dm > 0 {
                    println!(
                        "[Telemetry] Rate -> +{} queued, +{} matches | Active: {}",
                        dq, dm, self.telemetry.active_players.load(Ordering::Relaxed)
                    );
                }
                last_queued = queued;
                last_matches = matches;
            }
        })
    }
}
