use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use rand::prelude::*;
use rand_distr::{Normal, Exp};

use crate::domain::player::Player;
use crate::domain::ticket::Ticket;
use crate::application::engine::{EngineConfig, MatchmakingEngine, EngineMsg};
use crate::application::telemetry::Telemetry;
use crate::domain::team_balancer::Match;

pub struct SimulationContext {
    pub engine_handle: thread::JoinHandle<()>,
    pub match_receiver: mpsc::Receiver<Match>,
}

pub fn spawn_simulation(
    num_players: usize,
    num_producers: usize,
    telemetry: Arc<Telemetry>,
    shutdown_flag: Arc<AtomicBool>,
) -> SimulationContext {
    let (msg_sender, msg_receiver) = mpsc::channel();
    let (match_sender, match_receiver) = mpsc::channel();

    let engine_config = EngineConfig {
        bucket_size: 100.0,
        expansion_interval_secs: 0.5, 
        tick_interval_secs: 1, 
        team_balance_weight: 0.5,
    };

    let engine = MatchmakingEngine::new(
        engine_config,
        msg_receiver,
        match_sender,
        telemetry.clone(),
    );
    let engine_handle = thread::spawn(move || { engine.run(); });

    let mut producer_handles = Vec::new();
    let players_per_producer = num_players / num_producers;

    for p_id in 0..num_producers {
        let sender = msg_sender.clone();
        let telemetry_clone = telemetry.clone();

        let handle = thread::spawn(move || {
            let mut rng = thread_rng();
            let normal_dist = Normal::<f64>::new(1500.0, 300.0).unwrap();
            let exp_dist = Exp::<f64>::new(5.0).unwrap(); 
            
            let mut players_generated = 0;
            let mut party_index = 0;

            while players_generated < players_per_producer {
                let roll: f64 = rng.gen();
                let mut party_size = if roll < 0.70 { 1 } else if roll < 0.90 { 2 } else { 3 };
                
                if players_generated + party_size > players_per_producer {
                    party_size = players_per_producer - players_generated;
                }
                if party_size == 0 {
                    break;
                }
                
                let mut party_players = Vec::new();
                for j in 0..party_size {
                    let id = format!("P_{}_{}_{}", p_id, party_index, j);
                    let mmr = normal_dist.sample(&mut rng).clamp(100.0, 3000.0);
                    party_players.push(Player::new(id, mmr));
                }

                let party_id = format!("Party_{}_{}", p_id, party_index);
                let now_ms = std::time::UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
                let ticket = Ticket::new(party_id.clone(), party_players, now_ms, 100.0);
                telemetry_clone.record_players_queued(party_size);
                
                if sender.send(EngineMsg::Queue(ticket)).is_err() { break; }

                let sleep_ms = (exp_dist.sample(&mut rng) * 100.0) as u64;
                thread::sleep(Duration::from_millis(sleep_ms.clamp(1, 10)));
                
                players_generated += party_size;
                party_index += 1;
            }
        });
        producer_handles.push(handle);
    }

    let telemetry_clone = telemetry.clone();
    thread::spawn(move || {
        for handle in producer_handles { let _ = handle.join(); }

        let mut last_active = telemetry_clone.active_players.load(Ordering::Relaxed);
        let mut unchanged_checks = 0;
        loop {
            thread::sleep(Duration::from_millis(500));
            let active = telemetry_clone.active_players.load(Ordering::Relaxed);
            if active == 0 {
                break;
            }
            if active != last_active {
                last_active = active;
                unchanged_checks = 0;
            } else {
                unchanged_checks += 1;
                if active < 10 || unchanged_checks >= 30 {
                    break;
                }
            }
        }

        shutdown_flag.store(true, Ordering::Relaxed);
        drop(msg_sender); 
    });

    SimulationContext {
        engine_handle,
        match_receiver,
    }
}
