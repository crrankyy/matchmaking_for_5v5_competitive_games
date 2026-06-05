use crate::domain::player::Player;

#[derive(Debug, Clone, PartialEq)]
pub struct Ticket {
    pub id: String,
    pub players: Vec<Player>,
    pub entered_at_ms: u64,
    pub base_bucket: i32,
}

impl Ticket {
    pub fn new(id: String, players: Vec<Player>, entered_at_ms: u64, bucket_size: f64) -> Self {
        let avg_mmr = if players.is_empty() { 0.0 } else {
            players.iter().map(|p| p.mmr).sum::<f64>() / players.len() as f64
        };
        let base_bucket = (avg_mmr / bucket_size).floor() as i32;
        Self {
            id,
            players,
            entered_at_ms,
            base_bucket,
        }
    }

    pub fn size(&self) -> usize {
        self.players.len()
    }

    pub fn current_radius(&self, now_ms: u64, expansion_interval_secs: f64) -> i32 {
        if now_ms <= self.entered_at_ms {
            return 0;
        }
        let elapsed_secs = (now_ms - self.entered_at_ms) as f64 / 1000.0;
        (elapsed_secs / expansion_interval_secs).floor() as i32
    }

    pub fn mutual_consent(&self, other: &Self, now_ms: u64, expansion_interval_secs: f64) -> bool {
        let r_self = self.current_radius(now_ms, expansion_interval_secs);
        let r_other = other.current_radius(now_ms, expansion_interval_secs);
        let bucket_diff = (self.base_bucket - other.base_bucket).abs();
        bucket_diff <= r_self && bucket_diff <= r_other
    }
}
