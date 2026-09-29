use chrono::Utc;

pub fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
