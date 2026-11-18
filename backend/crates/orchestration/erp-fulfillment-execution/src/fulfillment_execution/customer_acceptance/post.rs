//! 客户验收草稿过账根事务；原状态机冲突语义不转换为收据回放。
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::ids::CustomerAcceptanceId;
use erp_fulfillment::dto::{CustomerAcceptanceView, PostCustomerAcceptanceRequest};
use erp_fulfillment::entity::fulfillment::{CustomerAcceptance, CustomerAcceptanceUpdate};
use erp_fulfillment::service::FulfillmentService;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::fulfillment;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::CustomerAcceptanceProcess;
use super::completion::{CompletionKind, complete_acceptance};
use super::evidence::ensure_evidence;
use super::task::prepare_customer_acceptance_task_command;
use crate::Result;
use crate::workflow::FulfillmentWorkflow;
impl CustomerAcceptanceProcess {
    /// 过账客户验收（草稿 → 已过账；§8.2 第 5 条跨集合事务）。
    ///
    /// 客户验收签署为 `NO_APPROVAL`：过账只写履约分配与状态迁移，不得绑定定义、启动审批实例或创建审批任务。
    ///
    /// 在同一事务内：锁定验收行与履约事实、校验逐行分配守恒（分配合计等于通过数量）、校验每个履约事实的净验收数量不超过净成功履约数量、写 `APPLY` 分配、迁移验收单状态、写审计。
    /// 重复过账由状态守卫（仅草稿）与状态机（`Draft → Posted`）防护。
    ///
    /// `id` 为验收单主键；`req` 为过账请求（逐行分配）。返回过账后的验收单视图。`NotFound`：验收单/履约事实不存在。
    ///
    /// `ConflictError`：状态不允许过账或重复过账。`ValidationError`：分配不守恒或超上限。`OutcomeUnknown`：提交结果无法确认。
    #[tracing::instrument(
        name = "fulfillment.customer_acceptance_post",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "customer_acceptance_post")
    )]
    pub async fn post_customer_acceptance(
        &self,
        id: &str,
        req: PostCustomerAcceptanceRequest,
        actor: &AuditActor,
    ) -> Result<CustomerAcceptanceView> {
        req.validate()?;
        FulfillmentService::validate_customer_acceptance_task_context(
            req.work_item_id.as_deref(),
            req.expected_task_version,
        )?;

        let posting = AcceptancePosting {
            acceptance_id: CustomerAcceptanceId::new(id.to_string()),
            actor: actor.clone(),
            db: self.db.clone(),
            authorization: self.authorization.clone(),
            workflow: self.workflow.clone(),
            request: req,
        };
        let client = posting.db.client().clone();
        let posted = client
            .with_transaction(move |executor| Box::pin(async move { posting.apply(executor).await }))
            .await?;
        Ok(posted.into())
    }
}

/// 根事务开始前固定的验收过账上下文；所有步骤沿用入口提供的执行器。
struct AcceptancePosting {
    acceptance_id: CustomerAcceptanceId,
    actor: AuditActor,
    db: Database,
    authorization: Arc<AuthorizationService>,
    workflow: Arc<dyn FulfillmentWorkflow>,
    request: PostCustomerAcceptanceRequest,
}

impl AcceptancePosting {
    // 按原顺序核对凭证和任务，再写入验收、销售进度及原成功审计。
    async fn apply(self, executor: &mut dyn Executor) -> Result<CustomerAcceptance> {
        let mut acceptance = self.prepare(executor).await?;
        let task = prepare_customer_acceptance_task_command(
            &self.db,
            &self.authorization,
            &acceptance.sales_order_id,
            self.actor.id(),
            self.request.work_item_id.as_deref(),
            self.request.expected_task_version,
            executor,
        )
        .await?;

        FulfillmentService::persist_customer_acceptance_post(
            &self.db,
            &mut acceptance,
            &self.request,
            executor,
        )
        .await?;

        complete_acceptance(
            &self.db,
            &self.authorization,
            &acceptance,
            &self.actor,
            CompletionKind::Post { task },
            executor,
        )
        .await?;
        Ok(acceptance)
    }
    // 固定过账功能资格先于状态、版本和凭证读取。
    async fn prepare(&self, executor: &mut dyn Executor) -> Result<CustomerAcceptance> {
        self.authorization
            .require(
                &self.actor,
                &fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_POST,
                AccessTarget::Operation,
                executor,
            )
            .await?;

        let mut acceptance =
            FulfillmentService::load_customer_acceptance_for_post(&self.db, &self.acceptance_id, executor)
                .await?;
        acceptance.update(CustomerAcceptanceUpdate {
            evidence_attachment_id: self.request.evidence_attachment_id.clone(),
            ..Default::default()
        })?;

        ensure_evidence(
            &self.db,
            self.workflow.as_ref(),
            acceptance.require_evidence()?,
            &self.actor,
            executor,
        )
        .await?;
        Ok(acceptance)
    }
}
