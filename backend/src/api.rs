use serde::{Deserialize, Serialize};
use crate::db::RowId;

#[derive(Debug, Deserialize)]
pub struct Prediction {
    pub prediction: String,
    pub judges: Vec<String>,
    pub judge_share_ppm: u32,
    pub trading_end: i64,
    pub decision_period_sec: u32,
    pub judge_count: u32,
}
#[derive(Debug, Serialize)]
pub struct NewPredictionResponse {
    pub(crate) id: RowId,
}
