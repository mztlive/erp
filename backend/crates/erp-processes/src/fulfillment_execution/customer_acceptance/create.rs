//! 客户验收草稿创建根事务：注册、履约头行写入、审计。
use super::{registration::register_created_customer_acceptance_document, CustomerAcceptanceProcess};
use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::ids::CustomerAcceptanceId;
use erp_fulfillment::dto::{CreateCustomerAcceptanceRequest, CustomerAcceptanceView};
use erp_fulfillment::entity::fulfillment::{CustomerAcceptance, CustomerAcceptanceLine};
use erp_fulfillment::repository::FulfillmentExt;
use erp_identity::SharedRbacService;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Transactional;
use services::Result;
use validator::Validate;
impl CustomerAcceptanceProcess {
    /// 创建客户验收单（草稿，跨集合：表头 + 行 + 审计）。
    ///
    /// 同一事务注册 `BusinessDocument` 并调用统一绑定端口。客户验收为
    /// `NO_APPROVAL`：返回空绑定，不查询已发布定义，不启动审批实例，
    /// 不创建审批任务。创建阶段不写验收分配；分配在过账时按行守恒与履约
    /// 事实上限校验后写入。
    ///
    /// # 参数
    /// * `req` - 创建请求（表头 + 行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建验收单的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - 单号重复（唯一索引透出）
    /// * `RepositoryError` - 数据库写入失败
    #[tracing::instrument(
        name = "fulfillment.customer_acceptance_create",
        skip_all,
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "customer_acceptance_create"
        )
    )]
    pub async fn create_customer_acceptance(
        &self,
        req: CreateCustomerAcceptanceRequest,
        actor: &AuditActor,
    ) -> Result<CustomerAcceptanceView> {
        req.validate()?;
        let id = CustomerAcceptanceId::new(next_id());
        let acceptance_no =
            erp_fulfillment::service::document_number::next_customer_acceptance_no(&self.db).await?;
        let (acceptance, lines) =
            erp_fulfillment::service::customer_acceptance::prepare_customer_acceptance_draft(
                id,
                acceptance_no,
                req,
            )?;
        persist_created_customer_acceptance(
            &self.db,
            &self.rbac,
            std::sync::Arc::clone(&self.object_read),
            acceptance.clone(),
            lines,
            actor.clone(),
        )
        .await?;
        Ok(acceptance.into())
    }
}
/// 在创建事务内写入客户验收草稿并登记无绑定单据。
///
/// # 错误
/// 绑定、注册或验收单写入失败时返回错误，调用方必须视作整体回滚。
async fn persist_created_customer_acceptance(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
    acceptance: CustomerAcceptance,
    lines: Vec<CustomerAcceptanceLine>,
    actor: AuditActor,
) -> Result<()> {
    let audit = actor.clone().resource_log(
        "customer_acceptance.create",
        "customer_acceptance",
        acceptance.base.id.clone(),
    )?;
    let db = db.clone();
    let rbac = rbac.clone();
    let object_read = object_read.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                register_created_customer_acceptance_document(
                    &db,
                    &rbac,
                    object_read.as_ref(),
                    &acceptance,
                    &actor,
                    session,
                )
                .await?;
                db.fulfillment()
                    .create_customer_acceptance_with_lines(&acceptance, &lines, session)
                    .await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), services::Error>(())
            })
        })
        .await
}
