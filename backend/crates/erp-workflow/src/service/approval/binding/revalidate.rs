use std::collections::HashMap;

use application_core::AuditActor;
use bpm::model::types::ModelError;
use mongodb::Database;
use persistence_core::Executor;

use super::super::business_adapter::{
    BindingRevalidationContext, adapter_object_read_decision_with, adapter_spec_of,
    ensure_separation_of_duties,
};
use super::super::policy::{ApproverEligibilityPolicy, ProcessRequiredApprovalPolicy};
use super::super::scope::approval_document_read_scope_with_executor;
use super::process_not_configured;
use super::upgrade::map_model_error;
use crate::error::{Error, Result};
use crate::ports::{ApprovalObjectReadPort, OrderTaskSource, WorkflowAccountFact, WorkflowAuthorizationPort};
use crate::repository::bpm::DefinitionGraph;
use crate::service::approval::approval_decide_scope_with_executor;

/// 复用 BPM 图原语重验发布结构；不得把 Executor 传入 BPM。
///
/// # 参数
/// * `graph` - Repository 一次性加载的发布定义图
///
/// # 返回
/// 状态为已发布且线性图完整时返回 `Ok(())`。
///
/// # 错误
/// 状态不是已发布时映射为未配置；图结构损坏时返回稳定校验错误。
///
/// # 关键业务约束
/// Service 不得复制节点顺序、入口或连线算法；仓储过滤不能替代 BPM 确认。
pub(super) fn revalidate_published_graph(graph: &DefinitionGraph) -> Result<()> {
    graph.validate_published_linear().map_err(|error| match error {
        ModelError::InvalidStatus(_) => process_not_configured(),
        other => map_model_error(other),
    })
}

/// Adapter 重验指定用户、权限、对象范围、读取权与岗位分离。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `policy` - 当前单据类型必须审批政策
/// * `context` - 当前单据组织与创建人事实
/// * `graph` - 已由 BPM 校验的定义图
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 全部定义审批人通过静态与对象访问重验时返回 `Ok(())`。
///
/// # 错误
/// 岗位分离、账号、权限、数据范围或对象读取权失败时返回错误。
///
/// # 关键业务约束
/// 审批人集合由 BPM 图确定性提取，Service 只编排外部资格判断。
pub(super) async fn revalidate_binding_graph(
    _db: &Database,
    rbac: &impl WorkflowAuthorizationPort,
    object_read: &dyn ApprovalObjectReadPort,
    policy: &ProcessRequiredApprovalPolicy,
    context: &BindingRevalidationContext,
    graph: &DefinitionGraph,
    executor: &mut dyn Executor,
) -> Result<()> {
    let spec = adapter_spec_of(policy.document_type)?;
    let assignee_ids = graph.assignee_ids();
    ensure_separation_of_duties(policy.separation_of_duties_policy, &context.creator_id, &assignee_ids)?;
    let accounts = load_assignee_accounts(rbac, &assignee_ids, executor).await?;
    for user_id in &assignee_ids {
        let account = require_ready_assignee(accounts.get(user_id))?;
        let assignee = AuditActor::new(account.id.clone(), account.login_account.clone(), account.kind);
        let object = context.scope_object(policy.document_type);
        if OrderTaskSource::approval_kind(policy.document_type).is_some()
            && (object.business_org_unit_id.as_deref().is_none_or(str::is_empty)
                || object.order_source.is_none()
                || object.owner_user_id.is_empty())
        {
            return Err(Error::ValidationError("订单审批绑定缺少内部业务部门".into()));
        }
        let decide = approval_decide_scope_with_executor(rbac, &assignee, executor).await?;
        let read =
            approval_document_read_scope_with_executor(rbac, &assignee, policy.document_type, executor)
                .await?;
        if !decide.covers_object(&object) || !read.covers_object(&object) {
            return Err(Error::ValidationError("指定审批人缺少动作权限或当前对象范围".into()));
        }
        if object.order_source.is_some() && !rbac.binding_order_readable(&assignee, &object, executor).await?
        {
            return Err(Error::ValidationError("指定审批人不能读取当前原订单".into()));
        }
        if adapter_object_read_decision_with(&spec, context, user_id, object_read)? != Some(true) {
            return Err(Error::ValidationError("指定审批人不能读取当前业务对象".into()));
        }
    }
    Ok(())
}

/// 一次批量读取定义内全部审批人账号事实。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `assignee_ids` - BPM 按确定顺序提取的审批人 ID
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 返回按账号 ID 索引的未软删除账号事实；缺失 ID 不会补齐。
///
/// # 错误
/// Repository 批量查询失败时返回错误。
///
/// # 关键业务约束
/// Repository 不保证 `$in` 结果顺序；Service 必须继续按 `assignee_ids`
/// 逐用户查表与重验 RBAC，保留首错与精确错误语义。
async fn load_assignee_accounts(
    rbac: &impl WorkflowAuthorizationPort,
    assignee_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, WorkflowAccountFact>> {
    Ok(rbac
        .load_accounts(assignee_ids, executor)
        .await?
        .into_iter()
        .map(|account| (account.id.clone(), account))
        .collect())
}

/// 将账号实体的后台有效性判断映射为绑定校验错误。
///
/// # 参数
/// * `account` - Repository 返回的审批人账号
///
/// # 返回
/// 账号可承担后台责任时返回 `Ok(())`。
///
/// # 错误
/// 账号已停用或身份不满足后台责任时返回校验错误。
///
/// # 关键业务约束
/// 类型与状态组合规则只由 `WorkflowAccountFact::is_active_backoffice` 提供。
fn ensure_assignee_ready(account: &WorkflowAccountFact) -> Result<()> {
    match ApproverEligibilityPolicy::ActiveBackofficeWithDecidePermission {
        ApproverEligibilityPolicy::ActiveBackofficeWithDecidePermission => {},
    }
    if account.is_active_backoffice() {
        return Ok(());
    }
    Err(assignee_unavailable_error())
}

/// 按当前审批人 ID 从批量快照映射中取回可承担责任的账号。
///
/// # 参数
/// * `account` - 批量映射中按当前审批人 ID 查得的可选账号
///
/// # 返回
/// 账号存在且当前可承担后台审批责任时返回其引用。
///
/// # 错误
/// 账号缺失、已停用或任职失效时返回合同固定的校验错误。
///
/// # 关键业务约束
/// 调用方必须按 BPM 审批人顺序逐个调用，不得依赖 `HashMap` 迭代顺序。
pub(super) fn require_ready_assignee(account: Option<&WorkflowAccountFact>) -> Result<&WorkflowAccountFact> {
    let account = account.ok_or_else(assignee_unavailable_error)?;
    ensure_assignee_ready(account)?;
    Ok(account)
}

/// 构造审批人账号不可用的合同固定错误。
///
/// # 返回
/// 返回同时覆盖缺失、停用与任职失效的校验错误。
///
/// # 错误
/// 无；本方法只构造 Service 错误值。
fn assignee_unavailable_error() -> Error {
    Error::ValidationError("指定审批人账号不存在、已停用或任职失效".to_string())
}
