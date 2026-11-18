//! 反向验收的幂等根事务；事实逆转后刷新销售、按剩余事实重开任务并写独立回执。
use std::sync::Arc;

use application_core::{AuditActor, CommandReceipt};
use erp_core::ids::CustomerAcceptanceId;
use erp_fulfillment::dto::{CustomerAcceptanceView, ReverseCustomerAcceptanceRequest};
use erp_fulfillment::entity::fulfillment::CustomerAcceptance;
use erp_fulfillment::service::FulfillmentService;
use erp_identity::AuthorizationService;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::CustomerAcceptanceProcess;
use super::completion::{CompletionKind, complete_acceptance};
use super::replay::{ReplayReader, recover, reversed};
use crate::{Error, Result};
impl CustomerAcceptanceProcess {
    /// 冲正客户验收（已过账 → 已冲正；§8.2 第 5 条反向分配事务）。
    ///
    /// 客户验收签署为 `NO_APPROVAL`：冲正只追加反向验收事实，不得启动审批或创建任务。
    ///
    /// 误录时新增反向验收单：原验收行的通过/短少/拒收数量镜像复制，原 `APPLY` 分配逐条生成 `REVERSE` 分配（引用原分配），新验收单立即过账，原验收单登记反向引用并迁移到 `REVERSED`。
    /// 冲正不覆盖原验收事实。
    ///
    /// `id` 为待冲正验收单主键；`req` 为冲正请求（期望版本 + 原因）。返回新建反向验收单的视图。`NotFound`：验收单不存在。
    ///
    /// `ConflictError`：版本不符或状态不允许冲正。`OutcomeUnknown`：提交结果无法确认。
    #[tracing::instrument(
        name = "fulfillment.customer_acceptance_reverse",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "customer_acceptance_reverse")
    )]
    pub async fn reverse_customer_acceptance(
        &self,
        id: &str,
        req: ReverseCustomerAcceptanceRequest,
        actor: &AuditActor,
    ) -> Result<CustomerAcceptanceView> {
        req.validate()?;
        let command_receipt = CommandReceipt::from_resource_parts(
            "customer-acceptance-reverse-",
            actor.id(),
            "customer_acceptance.reverse",
            "customer_acceptance",
            id,
            &req.idempotency_key,
            [req.expected_version.to_string(), req.reason_text.clone()],
        )?;
        if let Some(view) = self.reversed_result(id, actor, &command_receipt).await? {
            return Ok(view);
        }

        let original_actor = actor.clone();
        let original_id = CustomerAcceptanceId::new(id.to_string());
        let actor = actor.clone();
        let authorization = ReverseAuthorization { authorization: self.authorization.clone() };
        let db = self.db.clone();
        let client = db.client().clone();
        let command_receipt_for_tx = command_receipt.clone();
        let transaction_result = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    commit_reverse(
                        &db,
                        authorization,
                        &original_id,
                        &req,
                        &actor,
                        command_receipt_for_tx,
                        executor,
                    )
                    .await
                })
            })
            .await;

        match transaction_result {
            Ok(reversed) => Ok(reversed.into()),
            Err(error) => self.recover_reverse(error, &command_receipt, id, &original_actor).await,
        }
    }

    // 恢复只查询当前授权快照中的已提交结果，不重放冲正写入。
    async fn recover_reverse(
        &self,
        error: Error,
        receipt: &CommandReceipt,
        id: &str,
        actor: &AuditActor,
    ) -> Result<CustomerAcceptanceView> {
        recover(error, self.reversed_result(id, actor, receipt)).await
    }

    // 原功能资格、真实原单和回执结果共用同一恢复事务。
    async fn reversed_result(
        &self,
        id: &str,
        actor: &AuditActor,
        receipt: &CommandReceipt,
    ) -> Result<Option<CustomerAcceptanceView>> {
        let reader = ReplayReader { db: self.db.clone(), authorization: self.authorization.clone() };
        let (id, actor, receipt) = (id.to_owned(), actor.clone(), receipt.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    Ok::<_, Error>(reversed(&reader, &id, &actor, &receipt, executor).await?.map(Into::into))
                })
            })
            .await
    }
}

struct ReverseAuthorization {
    authorization: Arc<AuthorizationService>,
}

// 重验授权和回执后，沿用当前事务依次写反向事实与后续任务。
async fn commit_reverse(
    db: &Database,
    authorization: ReverseAuthorization,
    original_id: &CustomerAcceptanceId,
    req: &ReverseCustomerAcceptanceRequest,
    actor: &AuditActor,
    receipt: CommandReceipt,
    executor: &mut dyn Executor,
) -> Result<CustomerAcceptance> {
    let reader = ReplayReader { db: db.clone(), authorization: authorization.authorization.clone() };
    if let Some(persisted) = reversed(&reader, original_id.as_ref(), actor, &receipt, executor).await? {
        return Ok(persisted);
    }

    let (original, reverse_acceptance) =
        FulfillmentService::persist_customer_acceptance_reverse(db, original_id, req, executor).await?;

    complete_acceptance(
        db,
        &authorization.authorization,
        &reverse_acceptance,
        actor,
        CompletionKind::Reverse { original_id: original.base.id, receipt: Box::new(receipt) },
        executor,
    )
    .await?;
    Ok::<CustomerAcceptance, Error>(reverse_acceptance)
}
