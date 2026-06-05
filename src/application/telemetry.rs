use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, AtomicU64, Ordering};

#[derive(Debug)]
pub struct TensionStats {
    pub sum_delta: f64,
    pub min_delta: f64,
    pub max_delta: f64,
    pub sum_variance: f64,
    pub min_variance: f64,
    pub max_variance: f64,
    pub sum_radius: u64,
    pub min_radius: u64,
    pub max_radius: u64,
}

impl TensionStats {
    pub fn new() -> Self {
        Self {
            sum_delta: 0.0,
            min_delta: f64::MAX,
            max_delta: f64::MIN,
            sum_variance: 0.0,
            min_variance: f64::MAX,
            max_variance: f64::MIN,
            sum_radius: 0,
            min_radius: u64::MAX,
            max_radius: 0,
        }
    }
    
    pub fn record(&mut self, delta: f64, variance: f64, radius: u64) {
        self.sum_delta += delta;
        if delta < self.min_delta { self.min_delta = delta; }
        if delta > self.max_delta { self.max_delta = delta; }
        
        self.sum_variance += variance;
        if variance < self.min_variance { self.min_variance = variance; }
        if variance > self.max_variance { self.max_variance = variance; }
        
        self.sum_radius += radius;
        if radius < self.min_radius { self.min_radius = radius; }
        if radius > self.max_radius { self.max_radius = radius; }
    }
}

#[derive(Debug)]
pub struct Telemetry {
    pub players_queued: AtomicUsize,
    pub matches_formed: AtomicUsize,
    pub active_players: AtomicUsize,
    pub active_solos: AtomicUsize,
    pub active_duos: AtomicUsize,
    pub active_trios: AtomicUsize,
    pub active_buckets: AtomicUsize,
    pub min_active_mmr: Mutex<f64>,
    pub max_active_mmr: Mutex<f64>,
    pub total_wait_time_ms: AtomicU64,
    pub cumulative_match_cost: Mutex<f64>,
    pub tension_stats: Mutex<TensionStats>,
    pub total_tick_micros: AtomicU64,
    pub tick_count: AtomicUsize,
    pub oldest_ticket_age_ms: AtomicU64,
    pub wait_times_ms: Mutex<Vec<u64>>,
}

impl Telemetry {
    pub fn new() -> Self {
        Self {
            players_queued: AtomicUsize::new(0),
            matches_formed: AtomicUsize::new(0),
            active_players: AtomicUsize::new(0),
            active_solos: AtomicUsize::new(0),
            active_duos: AtomicUsize::new(0),
            active_trios: AtomicUsize::new(0),
            active_buckets: AtomicUsize::new(0),
            min_active_mmr: Mutex::new(f64::MAX),
            max_active_mmr: Mutex::new(f64::MIN),
            total_wait_time_ms: AtomicU64::new(0),
            cumulative_match_cost: Mutex::new(0.0),
            tension_stats: Mutex::new(TensionStats::new()),
            total_tick_micros: AtomicU64::new(0),
            tick_count: AtomicUsize::new(0),
            oldest_ticket_age_ms: AtomicU64::new(0),
            wait_times_ms: Mutex::new(Vec::new()),
        }
    }

    pub fn record_players_queued(&self, count: usize) {
        self.players_queued.fetch_add(count, Ordering::Relaxed);
    }
    
    pub fn record_match_formed(&self, wait_times: Vec<u64>) {
        self.matches_formed.fetch_add(1, Ordering::Relaxed);
        let mut sum = 0;
        for w in &wait_times { sum += *w; }
        self.total_wait_time_ms.fetch_add(sum, Ordering::Relaxed);
        let mut lock = self.wait_times_ms.lock().unwrap();
        lock.extend(wait_times);
    }
    
    pub fn print_percentiles(&self) {
        let mut lock = self.wait_times_ms.lock().unwrap();
        if lock.is_empty() { return; }
        
        lock.sort_unstable();
        let n = lock.len() as f64;
        
        let p50 = lock[(n * 0.50).floor() as usize];
        let p90 = lock[(n * 0.90).floor() as usize];
        let p99 = lock[(n * 0.99).floor() as usize];
        let max = lock.last().unwrap();
        
        println!("Wait Time Percentiles -> p50: {}ms | p90: {}ms | p99: {}ms | max: {}ms", p50, p90, p99, max);
    }
}
