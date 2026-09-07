//! 反向验收的幂等根事务；事实逆转后刷新销售、按剩余事实重开任务并写双审计。
use super::{
    completion::{complete_acceptance, CompletionKind},
    CustomerAcceptanceProcess,
};
use crate::Result;
use application_core::{AuditActor, CommandReceipt};
use erp_audit::CommandReceiptServiceExt;
use erp_core::ids::CustomerAcceptanceId;
use erp_fulfillment::dto::{CustomerAcceptanceView, ReverseCustomerAcceptanceRequest};
use erp_fulfillment::entity::fulfillment::CustomerAcceptance;
use erp_fulfillment::service::FulfillmentService;
use persistence_core::Transactional;
use validator::Validate;
impl CustomerAcceptanceProcess {
    /// 冲正客户验收（已过账 → 已冲正；§8.2 第 5 条反向分配事务）。
    ///
    /// 客户验收签署为 `NO_APPROVAL`：冲正只追加反向验收事实，不得启动审批
    /// 或创建任务。
    ///
    /// 误录时新增反向验收单：原验收行的通过/短少/拒收数量镜像复制，原
    /// `APPLY` 分配逐条生成 `REVERSE` 分配（引用原分配），新验收单立即过账，
    /// 原验收单登记反向引用并迁移到 `REVERSED`。冲正不覆盖原验收事实。
    ///
    /// # 参数
    /// * `id` - 待冲正验收单主键
    /// * `req` - 冲正请求（期望版本 + 原因）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建反向验收单的视图。
    ///
    /// # 错误
    /// * `NotFound` - 验收单不存在
    /// * `ConflictError` - 版本不符或状态不允许冲正
    /// * `OutcomeUnknown` - 提交结果无法确认
    #[tracing::instrument(
        name = "fulfillment.customer_acceptance_reverse",
        skip_all,
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "customer_acceptance_reverse"
        )
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
        if let Some(reverse_acceptance_id) = command_receipt.committed_resource_id(&self.db).await? {
            return Ok(self
                .domain
                .customer_acceptance_detail(&reverse_acceptance_id)
                .await?
                .acceptance);
        }
        let original_id = CustomerAcceptanceId::new(id.to_string());
        let actor = actor.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let command_receipt_for_tx = command_receipt.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let (original, reverse_acceptance) =
                        FulfillmentService::persist_customer_acceptance_reverse(
                            &db,
                            &original_id,
                            &req,
                            session,
                        )
                        .await?;
                    complete_acceptance(
                        &db,
                        &reverse_acceptance,
                        &actor,
                        CompletionKind::Reverse {
                            original_id: original.base.id,
                            receipt: command_receipt_for_tx,
                        },
                        session,
                    )
                    .await?;
                    Ok::<CustomerAcceptance, crate::Error>(reverse_acceptance)
                })
            })
            .await;
        match transaction_result {
            Ok(reversed) => Ok(reversed.into()),
            Err(error) => match command_receipt.committed_resource_id(&self.db).await? {
                Some(reverse_acceptance_id) => Ok(self
                    .domain
                    .customer_acceptance_detail(&reverse_acceptance_id)
                    .await?
                    .acceptance),
                None => Err(error),
            },
        }
    }
}
