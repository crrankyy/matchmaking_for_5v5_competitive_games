#[derive(Debug, Clone, PartialEq)]
pub struct Player {
    pub id: String,
    pub mmr: f64,
}

impl Player {
    pub fn new(id: String, mmr: f64) -> Self {
        Self { id, mmr }
    }
}
