//! 已完成实例读取授权后的销售审批链条查询。

use persistence_core::NoTransaction;

use super::super::runtime_route::RuntimeApprovalRoute;
use super::ApprovalRuntimeService;
use super::read_auth::RuntimeReadSubject;
use crate::entity::document_registry::DocumentType;
use crate::error::{Error, Result};
use crate::repository::BpmExt;

impl<A: crate::ports::WorkflowAuthorizationPort> ApprovalRuntimeService<A> {
    /// 为销售类审批首页读取完整冻结路线。
    ///
    /// # 参数
    /// * `subject` - 已通过实例读取授权的主体。
    ///
    /// # 返回
    /// 销售单、卡券销售单和销售变更单返回本轮完整链条，其它类型返回空。
    ///
    /// # 错误
    /// 冻结运行图缺失、不一致或数据库查询失败时返回错误。
    pub(super) async fn sales_approval_route(
        &self,
        subject: &RuntimeReadSubject,
    ) -> Result<Option<RuntimeApprovalRoute>> {
        if !matches!(
            subject.document_type,
            DocumentType::SalesOrder | DocumentType::VoucherSalesOrder | DocumentType::SalesChangeOrder
        ) {
            return Ok(None);
        }
        let repository = self.db.bpm_workflow();
        let graph = repository
            .load_instance_graph(&subject.instance, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ConflictError("本次审批路线不存在，请刷新后重试".into()))?;
        let executions = repository
            .latest_node_executions(
                &subject.instance.typed_id(),
                subject.instance.current_round_no,
                &mut NoTransaction,
            )
            .await?;
        RuntimeApprovalRoute::from_runtime(&subject.instance, &graph, &executions).map(Some)
    }
}
