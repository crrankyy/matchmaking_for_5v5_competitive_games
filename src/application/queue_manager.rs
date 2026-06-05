use std::collections::{BTreeMap, VecDeque};
use crate::domain::ticket::Ticket;
use crate::domain::team_balancer::can_partition_5v5;
pub struct QueueDemographics {
    pub solos: usize,
    pub duos: usize,
    pub trios: usize,
    pub max_age_ms: u64,
    pub active_buckets: usize,
    pub min_mmr: f64,
    pub max_mmr: f64,
}

pub struct QueueManager {
    pub queues: BTreeMap<i32, VecDeque<Ticket>>,
    pub bucket_size: f64,
    pub expansion_interval_secs: f64,
}

impl QueueManager {
    pub fn new(bucket_size: f64, expansion_interval_secs: f64) -> Self {
        Self {
            queues: BTreeMap::new(),
            bucket_size,
            expansion_interval_secs,
        }
    }

    pub fn insert(&mut self, ticket: Ticket) {
        self.queues
            .entry(ticket.base_bucket)
            .or_insert_with(VecDeque::new)
            .push_back(ticket);
    }

    pub fn len(&self) -> usize {
        self.queues.values().map(|q| q.iter().map(|t| t.size()).sum::<usize>()).sum()
    }

    pub fn demographics(&self, now_ms: u64) -> QueueDemographics {
        let mut solos = 0;
        let mut duos = 0;
        let mut trios = 0;
        let mut max_age = 0;
        let active_buckets = self.queues.len();
        let mut min_mmr = f64::MAX;
        let mut max_mmr = f64::MIN;
        
        for queue in self.queues.values() {
            for ticket in queue {
                match ticket.size() {
                    1 => solos += 1,
                    2 => duos += 1,
                    3 => trios += 1,
                    _ => {}
                }
                let age = now_ms.saturating_sub(ticket.entered_at_ms);
                if age > max_age { max_age = age; }
                for player in &ticket.players {
                    if player.mmr < min_mmr { min_mmr = player.mmr; }
                    if player.mmr > max_mmr { max_mmr = player.mmr; }
                }
            }
        }
        QueueDemographics { solos, duos, trios, max_age_ms: max_age, active_buckets, min_mmr, max_mmr }
    }

    pub fn tick(&mut self, now_ms: u64) -> Vec<Vec<Ticket>> {
        let mut matches = Vec::new();
        let bucket_ids: Vec<i32> = self.queues.keys().cloned().collect();

        for bucket_id in bucket_ids {
            loop {
                if self.queues.get(&bucket_id).map_or(true, |q| q.is_empty()) {
                    break;
                }

                let anchor_ticket = self.queues.get(&bucket_id).unwrap().front().unwrap().clone();
                let radius = anchor_ticket.current_radius(now_ms, self.expansion_interval_secs);

                let mut candidates = Vec::new();
                for adj_bucket_id in (bucket_id - radius)..=(bucket_id + radius) {
                    if let Some(queue) = self.queues.get(&adj_bucket_id) {
                        for ticket in queue.iter() {
                            if ticket.id == anchor_ticket.id { continue; }
                            if anchor_ticket.mutual_consent(ticket, now_ms, self.expansion_interval_secs) {
                                candidates.push(ticket.clone());
                            }
                        }
                    }
                }

                candidates.sort_by_key(|t| t.entered_at_ms);

                let mut current_group = vec![anchor_ticket.clone()];
                if Self::find_match_dfs(
                    &candidates, 
                    &mut current_group, 
                    anchor_ticket.size(), 
                    0, 
                    now_ms, 
                    self.expansion_interval_secs
                ) {
                    for chosen in &current_group {
                        if let Some(queue) = self.queues.get_mut(&chosen.base_bucket) {
                            queue.retain(|t| t.id != chosen.id);
                        }
                    }
                    matches.push(current_group);
                } else {
                    // No more matches can be formed for this anchor right now.
                    break;
                }
            }
        }

        self.queues.retain(|_, queue| !queue.is_empty());
        matches
    }
    
    fn find_match_dfs(
        candidates: &[Ticket],
        current_group: &mut Vec<Ticket>,
        current_size: usize,
        start_idx: usize,
        now_ms: u64,
        expansion_interval_secs: f64
    ) -> bool {
        if current_size == 10 {
            return can_partition_5v5(current_group);
        }
        for i in start_idx..candidates.len() {
            let cand = &candidates[i];
            if current_size + cand.size() > 10 { continue; }
            
            let compatible = current_group.iter().all(|existing| {
                existing.mutual_consent(cand, now_ms, expansion_interval_secs)
            });
            
            if compatible {
                current_group.push(cand.clone());
                if Self::find_match_dfs(candidates, current_group, current_size + cand.size(), i + 1, now_ms, expansion_interval_secs) {
                    return true;
                }
                current_group.pop();
            }
        }
        false
    }
}
