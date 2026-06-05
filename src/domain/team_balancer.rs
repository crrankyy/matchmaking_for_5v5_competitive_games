use crate::domain::player::Player;
use crate::domain::ticket::Ticket;

#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    pub team_a: Vec<Player>,
    pub team_b: Vec<Player>,
    pub cost: f64,
    pub delta: f64,
    pub variance: f64,
}

pub fn calculate_average(players: &[Player]) -> f64 {
    if players.is_empty() { return 0.0; }
    players.iter().map(|p| p.mmr).sum::<f64>() / players.len() as f64
}

pub fn calculate_variance(players: &[Player], mean: f64) -> f64 {
    if players.is_empty() { return 0.0; }
    let sum_sq: f64 = players.iter().map(|p| {
        let diff = p.mmr - mean;
        diff * diff
    }).sum();
    sum_sq / players.len() as f64
}

/// Validates if a group of tickets can be cleanly partitioned into two 5-player teams.
/// Uses a fast bitmask knapsack since n <= 10.
pub fn can_partition_5v5(tickets: &[Ticket]) -> bool {
    let n = tickets.len();
    for mask in 0..(1 << n) {
        let mut a_size = 0;
        for i in 0..n {
            if (mask & (1 << i)) != 0 {
                a_size += tickets[i].size();
            }
        }
        if a_size == 5 {
            return true;
        }
    }
    false
}

/// Takes a slice of Tickets summing to exactly 10 players and finds the optimal 5v5 partition.
pub fn balance_teams(tickets: &[Ticket], w: f64) -> Match {
    let mut best_match: Option<Match> = None;
    let mut min_cost = f64::MAX;
    let n = tickets.len();

    // Iterate all 2^N possible team assignments
    for mask in 0..(1 << n) {
        let mut a_size = 0;
        let mut b_size = 0;
        
        for i in 0..n {
            if (mask & (1 << i)) != 0 {
                a_size += tickets[i].size();
            } else {
                b_size += tickets[i].size();
            }
        }

        // Only evaluate valid 5v5 splits
        if a_size == 5 && b_size == 5 {
            let mut team_a = Vec::with_capacity(5);
            let mut team_b = Vec::with_capacity(5);

            for i in 0..n {
                if (mask & (1 << i)) != 0 {
                    team_a.extend(tickets[i].players.clone());
                } else {
                    team_b.extend(tickets[i].players.clone());
                }
            }

            let avg_a = calculate_average(&team_a);
            let avg_b = calculate_average(&team_b);
            let var_a = calculate_variance(&team_a, avg_a);
            let var_b = calculate_variance(&team_b, avg_b);
            
            let delta = (avg_a - avg_b).abs();
            let variance = var_a + var_b;
            let cost = delta + w * variance;

            if cost < min_cost {
                min_cost = cost;
                best_match = Some(Match { team_a, team_b, cost, delta, variance });
            }
        }
    }

    best_match.expect("Tickets could not be partitioned into 5v5 teams")
}
