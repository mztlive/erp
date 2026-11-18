//! 授权队列公开用例；持久化逻辑由身份仓储拥有。
use application_core::AuditActor;
use persistence_core::{NoTransaction, Transactional};

use super::{GrantEventService, GrantRecompute, ResourceGrantService};
use crate::Result;

impl ResourceGrantService {
    /// 管理员在同一事务校验资格并重排一页正式事件。
    /// 本页末尾游标；空页返回 None。
    ///
    /// 越权、查询或写入冲突时整页拒绝；未知提交结果不自动重试。
    ///
    /// actor 为管理员，selection 为维护范围，after 为排他游标，now 为当前秒数。
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
    /// 对正式事件按主键有界重算，禁止从当前责任人猜测历史授权。
    /// 本页末尾游标；空页表示一轮结束。每次最多 100 条。
    ///
    /// 查询或更新失败返回错误；已重新排队事件可安全重放。
    ///
    /// filter 为类型化选择条件，after 为主键游标，now 为重新可领取时间。
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
