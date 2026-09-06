use mongodb::bson::{doc, Document};

pub async fn forbidden(db: mongodb::Database) {
    let filter = doc! { "id": "x" };
    db.collection::<Document>("customer_accounts")
        .find_one(filter)
        .await
        .ok();
}
