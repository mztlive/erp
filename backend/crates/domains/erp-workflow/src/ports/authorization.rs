//! 工作流消费的跨域业务授权事实；账号查询直接使用 identity。

use std::collections::{BTreeSet, HashSet};
use std::future::Future;

use application_core::AuditActor;
#[cfg(test)]
use erp_core::AccountKind;
use erp_identity::SharedRbacService;
use persistence_core::Executor;

use super::object_facts::OrderTaskSource;
use super::{WorkflowScopeObject, WorkflowScopeObjects};
use crate::entity::document_registry::DocumentType;
use crate::error::{Error, Result};

/// 工作流消费的业务来源取证与共享身份设施。
pub trait WorkflowAuthorizationPort: Clone + Send + Sync + 'static {
    /// 取得共享 identity 服务；通用权限与策略事务直接使用 identity API。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回组合根装配的共享 RBAC 服务。
    /// # 错误
    /// 未装配时拒绝，不构造空权限或降级身份。
    fn rbac(&self) -> Result<&SharedRbacService> {
        Err(Error::Internal("身份服务未装配".into()))
    }

    /// 当前审批办理在原事务检查完整单据读取资格。
    /// # 参数
    /// actor 为已验证当前办理人；source 为实际订单；instance 为审批实例；executor 为原事务。
    /// # 返回
    /// 当前入口和来源授权成立时成功。
    /// # 错误
    /// 未装配、授权未就绪或来源不可读时拒绝。
    fn require_approval_source(
        &self,
        _actor: &AuditActor,
        _source: &OrderTaskSource,
        _instance: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<()>> + Send {
        async { Err(Error::Internal("审批单据读取授权未装配".into())) }
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

    /// 绑定前按当前或拟创建订单事实独立核对详情范围；未装配时失败关闭。
    ///
    /// # 参数
    /// * `actor` - 当前操作人。
    /// * `object` - 当前或拟创建的订单范围事实。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 详情范围允许时返回 `true`，不允许时返回 `false`。
    ///
    /// # 错误
    /// 默认实现返回未装配的内部错误；实现无法完成核验时返回错误。
    fn binding_order_readable(
        &self,
        _actor: &AuditActor,
        _object: &WorkflowScopeObject,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<bool>> + Send {
        async { Err(Error::Internal("订单绑定读取范围未装配".into())) }
    }

    /// 有界批量读取当前审批对象事实；缺失对象不进入结果，配置失败不得吞掉。
    ///
    /// # 参数
    /// * `keys` - 单据类型与单据 ID。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回已找到的当前审批对象事实。
    ///
    /// # 错误
    /// 默认实现返回未装配的内部错误；配置失败时返回错误，不得当成空结果。
    fn approval_scope_objects(
        &self,
        _keys: &HashSet<(DocumentType, String)>,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<WorkflowScopeObjects>> + Send {
        async { Err(Error::Internal("审批批量范围事实未装配".into())) }
    }

    /// 从当前强业务实体映射审批范围维度，不使用历史责任字段推断部门。
    ///
    /// # 参数
    /// * `document_type` - 单据类型。
    /// * `document_id` - 单据 ID。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回当前强业务实体上的范围维度。
    ///
    /// # 错误
    /// 默认实现返回未装配的内部错误；对象缺失或读取失败时返回错误。
    fn approval_scope_object(
        &self,
        _document_type: DocumentType,
        _document_id: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<WorkflowScopeObject>> + Send {
        async { Err(Error::Internal("审批范围事实未装配".into())) }
    }

    /// 在审批原事务内重验 S2 订单对象当前详情权限。
    ///
    /// # 参数
    /// * `actor` - 当前审批人。
    /// * `document_type` - 审批快照固定的订单类型。
    /// * `document_id` - 业务单据主键，变更单由 adapter 沿原单解析。
    /// * `executor` - 审批决定、恢复或读取的执行器。
    /// # 返回
    /// 当前对象可读时为 true；业务越界返回 false，供审批引擎记录受阻。
    /// # 错误
    /// 未装配和基础设施失败不得转换成成功。
    fn order_approval_readable(
        &self,
        _actor: &AuditActor,
        _document_type: DocumentType,
        _document_id: &str,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<bool>> + Send {
        async { Err(Error::Internal("订单审批范围授权未装配".into())) }
    }

    /// 批量返回当前详情范围允许的关联订单，用于任务列表、统计及详情。
    ///
    /// # 参数
    /// * `actor` - 服务端账号身份。
    /// * `sources` - 从业务实体外键得到的去重订单来源。
    /// * `executor` - 调用方执行器。
    /// # 返回
    /// 返回已授权来源的子集；空授权必须保持空集。
    /// # 错误
    /// 未装配、未知范围版本及基础设施错误失败关闭。
    fn readable_order_sources(
        &self,
        _actor: &AuditActor,
        _sources: &BTreeSet<OrderTaskSource>,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<BTreeSet<OrderTaskSource>>> + Send {
        async { Err(Error::Internal("任务对象范围授权未装配".into())) }
    }

    /// 在任务命令原事务内独立重验关联业务对象的详情范围。
    ///
    /// # 参数
    /// * `actor` - 当前操作人或待接收任务的有效账号。
    /// * `source` - 事务内业务实体证明的订单来源。
    /// * `executor` - 原任务事务执行器。
    /// # 返回
    /// 对象当前可读时成功；任务责任不替代对象授权。
    /// # 错误
    /// 未装配、未接入、来源缺失或范围越界时失败关闭。
    fn require_order_task_read(
        &self,
        _actor: &AuditActor,
        _source: &OrderTaskSource,
        _executor: &mut dyn Executor,
    ) -> impl Future<Output = Result<()>> + Send {
        async { Err(Error::Internal("任务对象范围授权未装配".into())) }
    }
}

/// Fail-closed authorization port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedWorkflowAuthorizationPort;

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
