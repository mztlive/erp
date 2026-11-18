use mongodb::Database;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, mongo_ops};

use crate::dto::INBOX_PAGE_SIZE;
use crate::{Error, InboxQuery, Notification, NotificationData, Result};

pub(crate) const COLLECTION: &str = "notifications";

fn notifications(db: &Database) -> Repository<'_, Notification> {
    Repository::new(db, COLLECTION)
}

pub(crate) async fn receive(
    db: &Database,
    content: NotificationData,
    executor: &mut dyn Executor,
) -> Result<()> {
    let repo = notifications(db);
    let message = Notification::new(content);
    match mongo_ops::insert_one(&repo.collection(), &message, executor).await {
        Ok(()) => Ok(()),
        Err(persistence_core::Error::DuplicateKey(_)) => {
            let existing = repo
                .find_one(
                    doc! { "recipient_id": &message.content.recipient_id,
                    "dedup_key": &message.content.dedup_key },
                    executor,
                )
                .await?
                .ok_or(Error::Conflict)?;
            existing.replay(&message.content)
        },
        Err(error) => Err(error.into()),
    }
}

pub(crate) async fn inbox(
    db: &Database,
    recipient: &str,
    query: &InboxQuery,
    executor: &mut dyn Executor,
) -> Result<Vec<Notification>> {
    let filter = query.filter(recipient)?;
    Ok(mongo_ops::find_many(
        &notifications(db).collection(),
        filter,
        FindOptions::builder()
            .sort(doc! { "created_at": -1, "id": -1 })
            .limit(INBOX_PAGE_SIZE as i64 + 1)
            .build(),
        executor,
    )
    .await?)
}

pub(crate) async fn unread(db: &Database, recipient: &str, executor: &mut dyn Executor) -> Result<u64> {
    Ok(mongo_ops::count_documents(
        &notifications(db).collection(),
        doc! { "recipient_id": recipient, "read_at": null },
        executor,
    )
    .await?)
}

pub(crate) async fn mark_read(
    db: &Database,
    recipient: &str,
    id: &str,
    at: i64,
    executor: &mut dyn Executor,
) -> Result<()> {
    let result = mongo_ops::update_one(
        &notifications(db).collection(),
        doc! { "id": id, "recipient_id": recipient, "read_at": null },
        doc! { "$set": { "read_at": at } },
        false,
        executor,
    )
    .await?;
    if result.matched_count == 0
        && notifications(db).find_one(doc! { "id": id, "recipient_id": recipient }, executor).await?.is_none()
    {
        return Err(Error::NotFound);
    }
    Ok(())
}
