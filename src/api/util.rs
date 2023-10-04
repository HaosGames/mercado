use log::warn;
use reqwest::StatusCode;

pub fn map_any_err_and_code(e: anyhow::Error) -> (StatusCode, String) {
    warn!("Error: {:#}", e);
    (StatusCode::INTERNAL_SERVER_ERROR, format!("{}", e))
}
pub fn map_any_err(e: anyhow::Error) -> String {
    warn!("Error: {:#}", e);
    format!("{}", e)
}
