//! 授权队列公开用例；持久化逻辑由身份仓储拥有。
use application_core::AuditActor;
use persistence_core::{NoTransaction, Transactional};

use super::{GrantEventService, GrantRecompute, ResourceGrantService};
use crate::Result;

impl ResourceGrantService {
    /// 管理员重算授权时，在同一事务内检查管理资格，并按维护范围和排他游标 `after` 重排一页正式事件。
    /// `now` 为秒级可领取时间；返回本页末尾游标，空页返回 `None`。
    /// 越权、查询失败或事务写入冲突时整页失败；提交结果未知时不重跑整页事务。
    pub async fn recompute_managed(
        &self,
        actor: &AuditActor,
        selection: GrantRecompute,
        after: Option<String>,
        now: u64,
    ) -> Result<Option<String>> {
        let this = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.authorize_manager(&actor, executor).await?;
                    GrantEventService::new(this.db.clone())
                        .requeue_with_executor(&selection, after.as_deref(), now, executor)
                        .await
                })
            })
            .await
    }
}

impl GrantEventService {
    /// 后台定期重验时，按类型化范围和排他主键游标 `after` 扫描正式事件，只重新排队已发布的事件。
    /// 后续按事件证据重算，不从当前责任人推断历史授权。
    /// 每页扫描最多 100 条，`now` 为秒级可领取时间；返回本页末尾游标，空页表示本轮结束。
    /// 查询或更新失败时返回错误；已重排的事件可安全重放。
    pub async fn requeue(
        &self,
        filter: &GrantRecompute,
        after: Option<&str>,
        now: u64,
    ) -> Result<Option<String>> {
        self.requeue_with_executor(filter, after, now, &mut NoTransaction).await
    }
}

#[cfg(test)]
mod tests {
    use mongodb::Client;

    use super::*;
    use crate::Error;
    use crate::entity::resource_access::event::{GrantEvent, GrantEventEvidence, GrantEventPayload};

    // 验证队列在访问数据库前拒绝空领取令牌，以及不在快照事务中的完成请求。
    #[tokio::test]
    async fn queue_rejects_empty_claim_and_completion_without_snapshot() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("grant_queue_test");
        let queue = GrantEventService::new(db);
        assert!(matches!(
            queue.claim_event("", 1).await,
            Err(Error::ValidationError(message)) if message == "领取令牌不能为空"
        ));

        let event = GrantEvent::new(GrantEventEvidence {
            payload: GrantEventPayload::InvoiceTaskReassigned {
                work_item_id: "task".into(),
                command_receipt_id: "command".into(),
            },
            recipient: "operator".into(),
            fact_version: 1,
            occurred_at: 1,
        })
        .unwrap();
        assert!(matches!(
            queue.complete_event(event, &mut NoTransaction).await,
            Err(Error::Internal(message)) if message == "事件完成必须使用事务"
        ));
    }
}
