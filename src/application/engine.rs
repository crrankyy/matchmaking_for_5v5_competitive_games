use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use crate::domain::ticket::Ticket;
use crate::domain::team_balancer::{balance_teams, Match};
use crate::application::queue_manager::QueueManager;
use crate::application::telemetry::Telemetry;

pub enum EngineMsg {
    Queue(Ticket),
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub bucket_size: f64,
    pub expansion_interval_secs: f64,
    pub tick_interval_secs: u64,
    pub team_balance_weight: f64,
}

pub struct MatchmakingEngine {
    config: EngineConfig,
    receiver: Receiver<EngineMsg>,
    match_sender: Sender<Match>,
    telemetry: Arc<Telemetry>,
}

impl MatchmakingEngine {
    pub fn new(
        config: EngineConfig,
        receiver: Receiver<EngineMsg>,
        match_sender: Sender<Match>,
        telemetry: Arc<Telemetry>,
    ) -> Self {
        Self {
            config,
            receiver,
            match_sender,
            telemetry,
        }
    }

    pub fn run(self) {
        let mut queue_manager = QueueManager::new(
            self.config.bucket_size,
            self.config.expansion_interval_secs,
        );

        let tick_duration = Duration::from_secs(self.config.tick_interval_secs);
        let mut next_tick = Instant::now() + tick_duration;

        loop {
            let now_instant = Instant::now();
            let timeout = if next_tick > now_instant {
                next_tick.duration_since(now_instant)
            } else {
                Duration::from_secs(0)
            };

            match self.receiver.recv_timeout(timeout) {
                Ok(msg) => {
                    self.process_msg(msg, &mut queue_manager);
                    while let Ok(msg) = self.receiver.try_recv() {
                        self.process_msg(msg, &mut queue_manager);
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => { }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }

            let current_time = Instant::now();
            if current_time >= next_tick {
                let now_ms = current_time.elapsed().as_millis() as u64 
                             + std::time::UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;

                let tick_start = Instant::now();
                let matched_groups = queue_manager.tick(now_ms);
                let tick_micros = tick_start.elapsed().as_micros() as u64;
                
                self.telemetry.total_tick_micros.fetch_add(tick_micros, Ordering::Relaxed);
                self.telemetry.tick_count.fetch_add(1, Ordering::Relaxed);

                for group in matched_groups {
                    let matched_teams = balance_teams(&group, self.config.team_balance_weight);

                    let mut wait_times = Vec::new();
                    let mut max_match_radius = 0;
                    for ticket in &group {
                        let r = ticket.current_radius(now_ms, self.config.expansion_interval_secs);
                        if r > max_match_radius { max_match_radius = r; }
                        let wait = now_ms.saturating_sub(ticket.entered_at_ms);
                        for _ in 0..ticket.size() {
                            wait_times.push(wait);
                        }
                    }

                    if !wait_times.is_empty() {
                        self.telemetry.record_match_formed(wait_times);
                    }
                    if let Ok(mut cost_lock) = self.telemetry.cumulative_match_cost.lock() {
                        *cost_lock += matched_teams.cost;
                    }
                    if let Ok(mut tension_lock) = self.telemetry.tension_stats.lock() {
                        tension_lock.record(matched_teams.delta, matched_teams.variance, max_match_radius as u64);
                    }
                    let _ = self.match_sender.send(matched_teams);
                }
                
                next_tick = Instant::now() + tick_duration;
            }

            let now_ms_for_demographics = Instant::now().elapsed().as_millis() as u64 
                             + std::time::UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
            self.telemetry.active_players.store(queue_manager.len(), Ordering::Relaxed);
            let stats = queue_manager.demographics(now_ms_for_demographics);
            self.telemetry.active_solos.store(stats.solos, Ordering::Relaxed);
            self.telemetry.active_duos.store(stats.duos, Ordering::Relaxed);
            self.telemetry.active_trios.store(stats.trios, Ordering::Relaxed);
            self.telemetry.oldest_ticket_age_ms.store(stats.max_age_ms, Ordering::Relaxed);
            self.telemetry.active_buckets.store(stats.active_buckets, Ordering::Relaxed);
            if let Ok(mut min_lock) = self.telemetry.min_active_mmr.lock() { *min_lock = stats.min_mmr; }
            if let Ok(mut max_lock) = self.telemetry.max_active_mmr.lock() { *max_lock = stats.max_mmr; }
        }
    }
    
    fn process_msg(&self, msg: EngineMsg, qm: &mut QueueManager) {
        match msg {
            EngineMsg::Queue(t) => qm.insert(t),
        }
    }
}
