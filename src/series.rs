use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Series {
    pub id: i64,
    pub owner: i64,
    pub owner_name: String,
    pub title: String,
    pub description: String,
    pub decorated_description_html: String,
    pub thumbnail_url: String,
    pub is_listed: bool,
    pub created_at: String,
    pub updated_at: String,
    pub items: Vec<String>,
}
