use erp_identity::AccessControlExt;
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::Executor;

use super::super::TimelineEvent;
use crate::Result;
pub(in crate::sales_timeline) async fn names(
    db: &Database,
    events: &mut [TimelineEvent],
    executor: &mut dyn Executor,
) -> Result<()> {
    let ids = events.iter().filter_map(|event| event.actor_id.clone()).collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(());
    }
    let accounts = db.accounts().find_many(doc! { "id": { "$in": ids } }, executor).await?;
    for event in events {
        if let Some(id) = event.actor_id.as_deref() {
            event.actor_name = accounts
                .iter()
                .find(|account| account.base.id == id)
                .map(|account| account.name.clone())
                .unwrap_or_else(|| "未记录姓名".into());
        }
    }
    Ok(())
}
