//! 客户验收草稿过账根事务；原状态机冲突语义不转换为收据回放。
use super::{
    completion::{complete_acceptance, CompletionKind},
    CustomerAcceptanceProcess,
};
use application_core::AuditActor;
use entities::fulfillment::CustomerAcceptance;
use erp_core::ids::CustomerAcceptanceId;
use persistence_core::Transactional;
use services::fulfillment::FulfillmentService;
use services::fulfillment::{
    prepare_customer_acceptance_task_command, CustomerAcceptanceView, PostCustomerAcceptanceRequest,
};
use services::Result;
use validator::Validate;
impl CustomerAcceptanceProcess {
    /// 过账客户验收（草稿 → 已过账；§8.2 第 5 条跨集合事务）。
    ///
    /// 客户验收签署为 `NO_APPROVAL`：过账只写履约分配与状态迁移，不得绑定
    /// 定义、启动审批实例或创建审批任务。
    ///
    /// 在同一事务内：锁定验收行与履约事实、校验逐行分配守恒（分配合计等于
    /// 通过数量）、校验每个履约事实的净验收数量不超过净成功履约数量、写
    /// `APPLY` 分配、迁移验收单状态、写审计。重复过账由状态守卫（仅草稿）
    /// 与状态机（`Draft → Posted`）防护。
    ///
    /// # 参数
    /// * `id` - 验收单主键
    /// * `req` - 过账请求（逐行分配）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后的验收单视图。
    ///
    /// # 错误
    /// * `NotFound` - 验收单/履约事实不存在
    /// * `ConflictError` - 状态不允许过账或重复过账
    /// * `ValidationError` - 分配不守恒或超上限
    /// * `OutcomeUnknown` - 提交结果无法确认
    #[tracing::instrument(
        name = "fulfillment.customer_acceptance_post",
        skip_all,
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "customer_acceptance_post"
        )
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
        let acceptance_id = CustomerAcceptanceId::new(id.to_string());
        let actor = actor.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let posted = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut acceptance =
                        FulfillmentService::load_customer_acceptance_for_post(&db, &acceptance_id, session)
                            .await?;
                    let task = prepare_customer_acceptance_task_command(
                        &db,
                        &acceptance.sales_order_id,
                        actor.id(),
                        req.work_item_id.as_deref(),
                        req.expected_task_version,
                        session,
                    )
                    .await?;
                    FulfillmentService::persist_customer_acceptance_post(&db, &mut acceptance, &req, session)
                        .await?;
                    complete_acceptance(&db, &acceptance, &actor, CompletionKind::Post { task }, session)
                        .await?;
                    Ok::<CustomerAcceptance, services::Error>(acceptance)
                })
            })
            .await?;
        Ok(posted.into())
    }
}
