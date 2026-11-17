//! 工作流消费的跨域业务授权事实；账号查询直接使用 identity。

use std::future::Future;

use application_core::AuditActor;
#[cfg(test)]
use erp_core::AccountKind;
use persistence_core::Executor;

use super::ApprovalBindingFacts;
use super::object_facts::OrderTaskSource;
use crate::entity::document_registry::DocumentType;
use crate::error::{Error, Result};

/// 单任务来源检查沿用原用例的入口权限，不扩大为其他业务动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskReadAction {
    List,
    Detail,
}

impl TaskReadAction {
    /// 返回服务端固定的任务读取权限。
    /// # 参数
    /// 无。
    /// # 返回
    /// 列表来源资格或任务详情资格对应的权限码。
    /// # 错误
    /// 无。
    pub fn permission(self) -> &'static str {
        match self {
            Self::List => "work_item:list",
            Self::Detail => "work_item:detail",
        }
    }
}

/// 工作流消费的跨域业务来源取证；身份服务由用例直接注入。
pub trait WorkflowAuthorizationPort: Clone + Send + Sync + 'static {
    /// 当前审批授权被拒绝后检查正式分配授权是否仍待应用。
    /// # 参数
    /// actor 为已验证当前办理人；source 为实际订单；instance 为审批实例；executor 为原事务。
    /// # 返回
    /// 无待应用事件时成功；不授予任何功能或对象资格。
    /// # 错误
    /// 未装配、授权事件未就绪或基础设施失败时返回错误。
    fn approval_source_readiness(
        &self,
        _actor: &AuditActor,
        _source: &OrderTaskSource,
        _instance: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<()>> + Send {
        async { Err(Error::Internal("审批读取授权就绪检查未装配".into())) }
    }

    /// 在正式审批分配事务追加读取授权事件。
    /// # 参数
    /// writes 为已持久化的计划；executor 为原事务。
    /// # 返回
    /// 对应事件全部保存成功。
    /// # 错误
    /// 销售采购人工任务未装配事件提供者时拒绝。
    fn append_assignment_grants(
        &self,
        writes: &crate::service::approval::execution::PlannedWrites,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<()>> + Send {
        async move {
            if !writes.create_tasks.is_empty()
                && matches!(writes.instance.subject.subject_kind(), "sales_order" | "purchase_order")
            {
                return Err(Error::Internal("审批读取授权事件未装配".into()));
            }
            Ok(())
        }
    }

    /// 证明内部账号可读取指定供应商申请的当前对象范围。
    ///
    /// # 参数
    /// * `actor` - 当前内部账号，调用方已在同一执行器验证登录资格。
    /// * `request_id` - 精确供应商申请 ID，不从展示根对象推断。
    /// * `executor` - 调用方读取或业务事务执行器。
    /// # 返回
    /// detail 权限和当前 DataScope 均满足时返回 true；越界返回 false。
    /// # 错误
    /// 未装配、资格配置或基础设施失败时拒绝；具体任务责任不得替代对象授权。
    fn supplier_portal_request_readable(
        &self,
        _actor: &AuditActor,
        _request_id: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<bool>> + Send {
        async { Err(Error::Internal("供应商申请读取范围未装配".into())) }
    }

    /// 在当前申请对象范围内证明真实商品与供给业务确认资格。
    ///
    /// # 参数
    /// * `actor` - 同一执行器已验证为启用内部账号的当前确认人或转交候选。
    /// * `request_id` - 当前精确申请，商品/供给创建或复用资格由组合层读取真实事实判定。
    /// * `executor` - 任务创建、决定、转交或工作台读取使用的当前执行器。
    /// # 返回
    /// 当前所需业务动作权限与责任范围全部满足为 true；资格撤销或越界为 false。
    /// # 错误
    /// 未装配及基础设施错误保持失败关闭；此 Port 不复验已写入业务前的旧版本。
    fn supplier_portal_request_reviewable(
        &self,
        _actor: &AuditActor,
        _request_id: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<bool>> + Send {
        async { Err(Error::Internal("供应商申请业务确认资格未装配".into())) }
    }

    /// 按真实来源领域证明管理者可读取整个审批主体。
    ///
    /// # 参数
    /// * `actor` - 当前管理者。
    /// * `document_type` / `document_id` - 精确业务主体。
    /// * `executor` - 当前事务执行器。
    /// # 返回
    /// 真实来源全部可读时返回 true；部分财务分摊不等于整单可读。
    /// # 错误
    /// 未装配、来源缺失或授权基础设施失败时拒绝。
    fn approval_source_readable(
        &self,
        _actor: &AuditActor,
        _document_type: DocumentType,
        _document_id: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<bool>> + Send {
        async { Err(Error::Internal("审批来源读取授权未装配".into())) }
    }

    /// 在原提交事务内证明提交者可以读取本次锁定的合同。
    ///
    /// # 参数
    /// * `actor` - 当前提交者，不是审批材料的后续读取人。
    /// * `contract_id` - 本次销售提交锁定的合同身份。
    /// * `executor` - 业务提交沿用的事务执行器。
    /// # 返回
    /// 当前合同详情动作和真实客户来源边界均允许时返回 true。
    /// # 错误
    /// 未装配、授权配置或基础设施失败时拒绝；对象不可见时返回 false。
    /// # 关键业务约束
    /// 调用方仍须证明锁定修订及唯一 PDF 归属；此事实不授予审批人普通合同或文件读取权。
    fn approval_contract_readable(
        &self,
        _actor: &AuditActor,
        _contract_id: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<bool>> + Send {
        async { Err(Error::Internal("审批合同材料读取授权未装配".into())) }
    }

    /// 读取当前主体的存在性及订单绑定事实，不使用历史责任字段推断部门。
    ///
    /// # 参数
    /// * `document_type` - 单据类型。
    /// * `document_id` - 单据 ID。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回已确认主体存在性的订单来源、负责人和业务部门。
    ///
    /// # 错误
    /// 默认实现返回未装配的内部错误；对象缺失或读取失败时返回错误。
    fn approval_binding_facts(
        &self,
        _document_type: DocumentType,
        _document_id: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<ApprovalBindingFacts>> + Send {
        async { Err(Error::Internal("审批范围事实未装配".into())) }
    }

    /// 在原执行器内按任务读取入口独立重验关联业务对象范围。
    ///
    /// # 参数
    /// * `actor` - 当前操作人或待接收任务的有效账号。
    /// * `source` - 事务内业务实体证明的订单来源。
    /// * `action` - 服务端用例固定的任务读取入口。
    /// * `executor` - 原任务事务执行器。
    /// # 返回
    /// 对象当前可读时成功；任务责任不替代对象授权。
    /// # 错误
    /// 未装配、未接入、来源缺失或范围越界时失败关闭。
    fn require_order_task_read(
        &self,
        _actor: &AuditActor,
        _source: &OrderTaskSource,
        _action: TaskReadAction,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<()>> + Send {
        async { Err(Error::Internal("任务对象范围授权未装配".into())) }
    }
}

/// 测试未装配业务授权时的失败关闭行为。
#[cfg(test)]
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct FailClosedWorkflowAuthorizationPort;

#[cfg(test)]
impl WorkflowAuthorizationPort for FailClosedWorkflowAuthorizationPort {}

#[cfg(test)]
mod permission_tests {
    use persistence_core::NoTransaction;

    use super::*;

    #[tokio::test]
    async fn unwired_contract_material_authorization_fails_closed() {
        let actor = AuditActor::new("submitter".into(), "submitter".into(), AccountKind::Admin);
        let result = FailClosedWorkflowAuthorizationPort
            .approval_contract_readable(&actor, "contract-1", &mut NoTransaction)
            .await;
        assert!(matches!(result, Err(Error::Internal(message)) if message == "审批合同材料读取授权未装配"));
    }

    #[tokio::test]
    async fn unwired_supplier_portal_business_authorization_fails_closed() {
        let actor = AuditActor::new("reviewer".into(), "reviewer".into(), AccountKind::Admin);
        let result = FailClosedWorkflowAuthorizationPort
            .supplier_portal_request_reviewable(&actor, "request", &mut NoTransaction)
            .await;
        assert!(matches!(result, Err(Error::Internal(message)) if message == "供应商申请业务确认资格未装配"));
    }
}
