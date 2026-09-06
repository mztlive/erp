use axum::Json;
use mongodb::Database;

pub struct CustomerAccount;

pub async fn handler(db: Database) -> Json<String> {
    Json("no".into())
}
