use log::debug;
use reqwest::StatusCode;

pub fn map_any_err_and_code(e: anyhow::Error) -> (StatusCode, String) {
    debug!("Error: {:#}", e);
    (StatusCode::INTERNAL_SERVER_ERROR, format!("{:#}", e))
}
pub fn map_any_err(e: anyhow::Error) -> String {
    debug!("Error: {:#}", e);
    format!("{:#}", e)
}
