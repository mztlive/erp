use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_read_models::sales_center::order::dto::SalesOrderDetailView;
use erp_sales::dto::sales_order::CancelSalesOrderApprovalRequest;
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::prepare_cancel;
use erp_workflow::service::document_registry::find_approval_binding;
use persistence_core::NoTransaction;
use validator::Validate;

use super::super::SalesOrderCommandProcess;
use super::super::adapter::{
    execute_sales_order_domain_action, require_frozen_binding, sales_approval_ports,
};
use super::super::cancel_approval::{
    SalesOrderCancelPersistInput, build_sales_order_cancel_input, load_cancel_runtime,
    persist_sales_order_cancel,
};
use super::submit::latest_submission_no;
use crate::{Error, Result};

impl SalesOrderCommandProcess {
    /// 撤回审批中的销售单，回到可修正草稿。
    ///
    /// `GoodsService` 与 `Voucher` 均先按主体加载 RUNNING/BLOCKED 实例并调用
    /// 统一 `prepare_cancel`，关闭开放任务后再执行 `cancel_action`。
    /// 已 `APPROVED` 必须拒绝。
    ///
    /// # 参数
    /// * `id` - 销售单主键
    /// * `req` - 撤回请求（原因必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回撤回后的销售单详情。
    ///
    /// # 错误
    /// 非审批中、已最终通过、原因缺失或并发冲突时返回错误。
    #[tracing::instrument(
        name = "sales_order.cancel_approval",
        skip_all,
        fields(layer = "service", domain = "sales_order", operation = "cancel_approval")
    )]
    pub async fn cancel_approval_submission(
        &self,
        id: &str,
        req: CancelSalesOrderApprovalRequest,
        actor: &AuditActor,
    ) -> Result<SalesOrderDetailView> {
        req.validate()?;
        let access = self.command_access(actor, "cancel_approval")?;
        let authorized_order = access.current(id, &mut NoTransaction).await?;
        let mut order = authorized_order;
        let expected_order_version = order.base.version;
        if !order.matches_version(req.expected_version) {
            return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()));
        }
        let ports = sales_approval_ports(order.business_type)?;
        let binding =
            find_approval_binding(&self.db, id, &mut NoTransaction).await.map_err(crate::Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let subject = crate::order_to_cash::subject_ref_for_sales_business(order.business_type, id)
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        let subject_version = latest_submission_no(&self.db, id).await?;
        let runtime = load_cancel_runtime(&self.db, &binding, &subject, subject_version).await?;
        let now = Instant::now();
        let idempotency_key = normalize_idempotency_key(&req.idempotency_key)?;
        let input =
            build_sales_order_cancel_input(&runtime, &req.reason, actor.id(), &idempotency_key, None, now)?;
        let prepared = prepare_cancel(input)?;
        execute_sales_order_domain_action(&mut order, ports.cancel_action, actor.id())?;
        let audit =
            actor.clone().resource_log("sales_order.cancel_approval", "sales_order", id.to_string())?;
        persist_sales_order_cancel(
            &self.db,
            access,
            expected_order_version,
            SalesOrderCancelPersistInput {
                order,
                prepared,
                open_tasks: runtime.open_tasks,
                actor_id: actor.id().to_string(),
                reason: req.reason.clone(),
                now,
                audit,
            },
        )
        .await?;
        self.read_model().sales_order_detail(id, None).await.map_err(crate::Error::from)
    }
}
