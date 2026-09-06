use std::collections::HashMap;

use bpm::graph::{assignee_ids, DefinitionGraph, NodeReplacementDraft};
use bpm::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId, ApprovalTransitionDefinitionId};
use bpm::model::{ApprovalNodeDefinition, ApprovalProcessDefinition};
use bpm::ParticipantId;
use database::repository::bpm::CasWriteOutcome;
use database::BpmExt;
use erp_identity::AccessControlExt;
use erp_identity::{AccountCore, Permission};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::errors::{Error, ErrorCode, Result};
use application_core::AuditActor;
use erp_identity::{subject, SharedRbacService};

use super::super::definition_dto::{
    DefinitionDetailView, DefinitionNodeRequest, ReplaceDefinitionNodesRequest,
};
use super::super::policy::{
    ApproverEligibilityPolicy, ProcessRequiredApprovalPolicy, SeparationOfDutiesPolicy,
    STATIC_APPROVE_PERMISSION,
};
use super::command::{
    applied_definition, definition_not_found, ensure_definition_admin_permission, ensure_lock, map_bpm_error,
    map_model_error, node_summary, now, parse_idempotency_key, policy_for_definition, replace_nodes_identity,
    replay_prepared_definition_receipt, write_definition_audit, write_receipt, DefinitionCommandResultRef,
    DefinitionResultExpectation, PreparedDefinitionIdentity,
};
use super::mapping::detail_view;
use super::ApprovalDefinitionService;

impl ApprovalDefinitionService {
    /// 整组替换草稿节点。
    ///
    /// # 参数
    /// * `request` - 节点替换请求
    /// * `actor` - 已认证操作人
    ///
    /// # 错误
    /// 锁版本冲突、节点不合法或账号校验失败时返回错误。
    pub async fn replace_definition_nodes(
        &self,
        mut request: ReplaceDefinitionNodesRequest,
        actor: &AuditActor,
    ) -> Result<DefinitionDetailView> {
        let key = parse_idempotency_key(&request.idempotency_key)?;
        request.nodes = prepare_definition_nodes(request.nodes)?;
        request.idempotency_key = key.as_str().to_string();
        let identity = replace_nodes_identity(
            key,
            &request.definition_id,
            request.expected_definition_lock_version,
            &request.nodes,
            actor.id(),
        )?;
        let graph = self.require_graph(&request.definition_id).await?;
        let policy = policy_for_definition(&graph.definition)?;
        self.ensure_definition_admin(actor, &policy).await?;
        if let Some(view) = self
            .replay_if_receipt(
                &identity.current,
                DefinitionResultExpectation::DefinitionId(request.definition_id.clone()),
            )
            .await?
        {
            return Ok(view);
        }
        ensure_draft_lock(&graph.definition, request.expected_definition_lock_version)?;
        self.commit_replace_nodes(graph, policy, request, actor, identity)
            .await
    }

    /// 在唯一事务中替换草稿图。
    ///
    /// # 错误
    /// 陈旧锁、节点非法或账号校验失败时返回错误。
    async fn commit_replace_nodes(
        &self,
        graph: DefinitionGraph,
        policy: ProcessRequiredApprovalPolicy,
        request: ReplaceDefinitionNodesRequest,
        actor: &AuditActor,
        identity: PreparedDefinitionIdentity,
    ) -> Result<DefinitionDetailView> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let transaction_actor = actor.clone();
        let transaction_policy = policy.clone();
        let transaction_identity = identity.clone();
        let expectation = DefinitionResultExpectation::DefinitionId(request.definition_id.clone());
        let outcome = db
            .client()
            .clone()
            .with_transaction(move |session| {
                Box::pin(async move {
                    replace_nodes_tx(
                        &db,
                        &rbac,
                        graph,
                        ReplaceNodesTxInput {
                            policy: transaction_policy,
                            request,
                            actor: &transaction_actor,
                            identity: &transaction_identity,
                        },
                        session,
                    )
                    .await
                })
            })
            .await;
        self.recover_definition_command(outcome, &policy, actor, identity, expectation)
            .await
    }
}

/// 替换草稿节点事务的政策、请求、命令身份与操作人。
///
/// # 用途
/// 打包 [`replace_nodes_tx`] 的非基础设施参数。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 收据回放优先于任何图替换写库。
struct ReplaceNodesTxInput<'a> {
    /// 单据审批政策。
    policy: ProcessRequiredApprovalPolicy,
    /// 节点替换请求。
    request: ReplaceDefinitionNodesRequest,
    /// 审计操作人。
    actor: &'a AuditActor,
    /// 已规范化的当前命令身份及精确旧格式候选。
    identity: &'a PreparedDefinitionIdentity,
}

/// 事务内替换草稿节点。
///
/// # 用途
/// 回放收据或校验并持久化新草稿图。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `graph` - 当前草稿图
/// * `input` - 政策、请求、摘要与操作人
/// * `session` - 事务会话
///
/// # 返回
/// 返回替换后的定义详情。
///
/// # 错误
/// 陈旧锁、节点非法、账号校验失败或写入失败时返回错误。
///
/// # 关键业务约束
/// 已有收据必须原样回放，不得重复 persist。
async fn replace_nodes_tx(
    db: &Database,
    rbac: &SharedRbacService,
    graph: DefinitionGraph,
    input: ReplaceNodesTxInput<'_>,
    session: &mut mongodb::ClientSession,
) -> Result<DefinitionDetailView> {
    let ReplaceNodesTxInput {
        policy,
        request,
        actor,
        identity,
    } = input;
    ensure_definition_admin_permission(db, rbac, actor, &policy, session).await?;
    if let Some(view) = replay_prepared_definition_receipt(
        db,
        identity,
        &DefinitionResultExpectation::DefinitionId(request.definition_id.clone()),
        session,
    )
    .await?
    {
        return Ok(view);
    }
    let reloaded = reload_draft_for_cas(
        db,
        &graph.definition,
        request.expected_definition_lock_version,
        session,
    )
    .await?;
    let prepared = prepare_replacement(db, rbac, &reloaded, &policy, &request.nodes, actor, session).await?;
    let result_ref = DefinitionCommandResultRef::from_graph(&prepared).encode();
    write_receipt(db, &identity.current, &result_ref, session).await?;
    apply_draft_graph(
        db,
        prepared,
        request.expected_definition_lock_version,
        actor,
        "approval_definition.replace_nodes",
        Some(node_summary(&reloaded.nodes)),
        session,
    )
    .await
}

/// 重新加载草稿并核对锁版本。
pub(super) async fn reload_draft_for_cas(
    db: &Database,
    original: &ApprovalProcessDefinition,
    expected: u64,
    session: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let graph = db
        .bpm_workflow()
        .load_definition_graph(
            &ApprovalProcessDefinitionId::new(original.base.id.clone()),
            session,
        )
        .await?
        .ok_or_else(definition_not_found)?;
    let ReplaceNodesWriteStep::PrepareAndReplaceGraph =
        allow_prepare_replacement(&graph.definition, expected)?;
    Ok(graph)
}

/// 按请求构造替换后的草稿图。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `graph` - 当前草稿图
/// * `policy` - 当前单据类型必须审批政策
/// * `requests` - 客户端整组节点请求
/// * `actor` - 已认证定义管理员
/// * `session` - 调用方事务执行器
///
/// # 返回
/// 返回已刷新审批人快照并由 BPM 重建的草稿图。
///
/// # 错误
/// 审批人、权限、节点请求或 BPM 图规则失败时返回错误。
///
/// # 关键业务约束
/// Service 只转换 DTO、加载账号并编排校验，节点身份与图规则由 BPM 决定。
async fn prepare_replacement(
    db: &Database,
    rbac: &SharedRbacService,
    graph: &DefinitionGraph,
    policy: &ProcessRequiredApprovalPolicy,
    requests: &[DefinitionNodeRequest],
    actor: &AuditActor,
    session: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let _ = actor;
    let drafts = node_replacement_drafts(requests)?;
    let planned = graph
        .plan_replacement_nodes(&drafts, now()?)
        .map_err(map_model_error)?;
    let assignee_ids = assignee_ids(&planned);
    let snapshots = load_assignee_snapshots(db, &assignee_ids, session).await?;
    let ReplaceAssigneesWriteStep::ApplySnapshotsAndReplaceGraph =
        allow_replace_after_assignees(validate_assignees(db, rbac, policy, &assignee_ids, session).await)?;
    let nodes = apply_snapshots(planned, &snapshots)?;
    rebuild_draft_graph(&graph.definition, nodes)
}

/// 以 CAS 写回草稿图并记录审计。
async fn apply_draft_graph(
    db: &Database,
    graph: DefinitionGraph,
    expected: u64,
    actor: &AuditActor,
    action: &str,
    before_summary: Option<String>,
    session: &mut dyn Executor,
) -> Result<DefinitionDetailView> {
    let applied = replace_graph(db, graph, expected, session).await?;
    write_definition_audit(
        db,
        actor,
        action,
        &applied,
        Some(expected),
        before_summary.as_deref(),
        session,
    )
    .await?;
    Ok(detail_view(&applied))
}

/// 调用仓储整组替换草稿图。
pub(super) async fn replace_graph(
    db: &Database,
    graph: DefinitionGraph,
    expected: u64,
    session: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let outcome = db
        .bpm_workflow()
        .replace_draft_graph(
            &graph.definition,
            &graph.nodes,
            &graph.transitions,
            expected,
            session,
        )
        .await?;
    Ok(DefinitionGraph {
        definition: allow_apply_replaced_definition(outcome)?,
        nodes: graph.nodes,
        transitions: graph.transitions,
    })
}

/// 将 Service 节点请求转换为 BPM 整组替换输入。
///
/// # 参数
/// * `requests` - 客户端提交的完整节点列表
///
/// # 返回
/// 返回携带调用方生成新身份与 BPM 参与人引用的替换输入。
///
/// # 错误
/// 审批人引用无效时返回校验错误。
///
/// # 关键业务约束
/// Service 只提供 ID，不判断节点顺序、已有身份归属或用途清理规则。
fn node_replacement_drafts(requests: &[DefinitionNodeRequest]) -> Result<Vec<NodeReplacementDraft>> {
    requests
        .iter()
        .map(|request| {
            let existing_node_id = request.node_id.as_deref().map(ApprovalNodeDefinitionId::new);
            NodeReplacementDraft::new(
                existing_node_id,
                ApprovalNodeDefinitionId::new(next_id()),
                next_id(),
                request.node_name.clone(),
                request.display_order,
                ParticipantId::new(request.assignee_user_id.clone()).map_err(map_bpm_error)?,
            )
            .map_err(map_model_error)
        })
        .collect()
}

/// 批量读取审批人并校验账号、静态权限与可静态判断的岗位分离。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `policy` - 当前单据类型必须审批政策
/// * `user_ids` - BPM 确定性提取的审批人 ID
/// * `session` - 调用方事务执行器
///
/// # 返回
/// 全部审批人通过定义期静态资格校验时返回 `Ok(())`。
///
/// # 错误
/// 账号缺失、后台有效性、静态权限或岗位分离失败时返回错误。
///
/// # 关键业务约束
/// 定义期不读取具体实例 DataScope，运行时对象访问资格由绑定与执行阶段重验。
pub(super) async fn validate_assignees(
    db: &Database,
    rbac: &SharedRbacService,
    policy: &ProcessRequiredApprovalPolicy,
    user_ids: &[String],
    session: &mut dyn Executor,
) -> Result<()> {
    let accounts = load_assignee_snapshots(db, user_ids, session).await?;
    for user_id in user_ids {
        let account = require_active_backoffice_assignee(accounts.get(user_id))?;
        ensure_static_eligibility(rbac, policy, account).await?;
    }
    validate_static_separation(policy.separation_of_duties_policy, user_ids)
}

/// 批量读取账号快照。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `user_ids` - 去重后的审批人账号 ID
/// * `session` - 调用方事务执行器
///
/// # 返回
/// 返回按账号 ID 索引的账号快照；空输入直接返回空映射。
///
/// # 错误
/// Repository 批量查询失败时返回错误。
///
/// # 关键业务约束
/// 查询条件由账号 Repository 的 `list_by_ids` 封装，禁止 Service 拼装 MongoDB 条件。
pub(super) async fn load_assignee_snapshots(
    db: &Database,
    user_ids: &[String],
    session: &mut dyn Executor,
) -> Result<HashMap<String, AccountCore>> {
    if user_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let accounts = db.accounts().list_by_ids(user_ids, session).await?;
    Ok(accounts
        .into_iter()
        .map(|account| (account.base.id.clone(), account))
        .collect())
}

/// 校验后台有效账号与静态审批权限，不伪造实例 DataScope。
///
/// # 参数
/// * `rbac` - 共享 RBAC 服务
/// * `policy` - 当前单据类型审批人资格政策
/// * `account` - Repository 返回的账号快照
///
/// # 返回
/// 账号当前有效且具有静态审批权限时返回 `Ok(())`。
///
/// # 错误
/// 后台有效性、权限常量解析或 RBAC 查询失败时返回错误。
///
/// # 关键业务约束
/// 账号类型与状态组合由实体判断；本方法只编排 RBAC 权限重验。
async fn ensure_static_eligibility(
    rbac: &SharedRbacService,
    policy: &ProcessRequiredApprovalPolicy,
    account: &AccountCore,
) -> Result<()> {
    match policy.approver_eligibility_policy {
        ApproverEligibilityPolicy::ActiveBackofficeWithDecidePermission => {}
    }
    require_active_backoffice_assignee(Some(account))?;
    let permission = Permission::parse(STATIC_APPROVE_PERMISSION)
        .map_err(|error| Error::Internal(format!("静态审批权限不变量损坏: {error}")))?;
    let allowed = rbac
        .enforce(&subject(account.kind, &account.base.id), &permission)
        .await?;
    ensure_static_decide_permission(allowed)
}

/// 定义期只能判断节点间岗位分离；提交人隔离留到运行时。
pub(super) fn validate_static_separation(
    policy: SeparationOfDutiesPolicy,
    _user_ids: &[String],
) -> Result<()> {
    match policy {
        SeparationOfDutiesPolicy::ForbidSubmitterAsApprover => Ok(()),
    }
}

/// 把账号显示名写入节点快照。
///
/// # 参数
/// * `nodes` - BPM 已规划的节点集合
/// * `accounts` - Repository 返回的账号 ID 到快照映射
///
/// # 返回
/// 返回由 BPM 节点方法刷新显示名后的节点集合。
///
/// # 错误
/// 账号快照缺失或 BPM 拒绝显示名时返回错误。
///
/// # 关键业务约束
/// Service 只把仓储快照与节点关联，不复制节点重建或名称规范化规则。
pub(super) fn apply_snapshots(
    nodes: Vec<ApprovalNodeDefinition>,
    accounts: &HashMap<String, AccountCore>,
) -> Result<Vec<ApprovalNodeDefinition>> {
    let at = now()?;
    let mut refreshed = Vec::with_capacity(nodes.len());
    for node in nodes {
        let account = accounts
            .get(node.assignee_participant_id.as_str())
            .ok_or_else(|| Error::ValidationError("指定审批人账号不存在、已停用或任职失效".to_string()))?;
        refreshed.push(
            node.with_assignee_label_snapshot(account.name.clone(), at)
                .map_err(map_model_error)?,
        );
    }
    Ok(refreshed)
}

/// 由节点快照重建草稿入口与线性连线。
///
/// # 参数
/// * `definition` - 需要保持身份与业务版本的草稿定义
/// * `nodes` - 已完成账号快照刷新的完整节点集合
///
/// # 返回
/// 返回 BPM 已校验的完整草稿图。
///
/// # 错误
/// 节点、入口或线性连线模型非法时返回校验错误。
///
/// # 关键业务约束
/// Service 只提供连线 ID 与时间，不实现节点顺序或图完整性规则。
pub(super) fn rebuild_draft_graph(
    definition: &ApprovalProcessDefinition,
    nodes: Vec<ApprovalNodeDefinition>,
) -> Result<DefinitionGraph> {
    let transition_ids = next_transition_ids(nodes.len());
    DefinitionGraph::rebuild_draft(definition, nodes, transition_ids, now()?).map_err(map_model_error)
}

/// 为线性定义图生成调用方负责的连线 ID。
///
/// # 参数
/// * `node_count` - 定义节点数量
///
/// # 返回
/// 返回每节点两条连线所需的 ID 集合。
///
/// # 错误
/// 无；节点数量合法性由 BPM 构图方法校验。
///
/// # 关键业务约束
/// 只生成身份，不在 Service 推导连线来源、事件或目标。
pub(super) fn next_transition_ids(node_count: usize) -> Vec<ApprovalTransitionDefinitionId> {
    (0..node_count.saturating_mul(2))
        .map(|_| ApprovalTransitionDefinitionId::new(next_id()))
        .collect()
}

/// 校验定义仍为草稿且锁版本匹配。
///
/// # 参数
/// * `definition` - 当前定义快照
/// * `expected` - 调用方期望的定义锁版本
///
/// # 返回
/// 定义可修改且锁版本一致时返回 `Ok(())`。
///
/// # 错误
/// 已发布、已退役或锁版本陈旧时返回冲突错误。
///
/// # 关键业务约束
/// 可变状态与锁版本规则由 BPM 定义实体提供，本层只映射稳定错误语义。
fn ensure_draft_lock(definition: &ApprovalProcessDefinition, expected: u64) -> Result<()> {
    definition
        .ensure_mutable()
        .map_err(|_| Error::from_approval_code(ErrorCode::ApprovalDefinitionNotDraft))?;
    ensure_lock(definition, expected)
}

/// 使用 DTO 清洗规则准备完整有序节点载荷。
pub(super) fn prepare_definition_nodes(
    nodes: Vec<DefinitionNodeRequest>,
) -> Result<Vec<DefinitionNodeRequest>> {
    let mut prepared = Vec::with_capacity(nodes.len());
    for node in nodes {
        prepared.push(node.prepare().map_err(map_model_error)?);
    }
    prepared.sort_by_key(|node| node.display_order);
    Ok(prepared)
}

/// 草稿替换在锁通过后的下一步。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplaceNodesWriteStep {
    /// 允许 prepare_replacement 与 apply_draft_graph。
    PrepareAndReplaceGraph,
}

/// 账号/静态权限校验通过后才允许写替换图。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplaceAssigneesWriteStep {
    /// 允许刷新账号快照并调用 BPM 重建草稿图。
    ApplySnapshotsAndReplaceGraph,
}

/// 替换路径在审批人校验失败时不得写图。
///
/// # 参数
/// * `assignees` - `validate_assignees` 的结果
///
/// # 返回
/// 校验通过时返回写图步骤。
///
/// # 错误
/// 透传账号不存在、不可登录、缺静态权限或岗位分离失败。
///
/// # 约束
/// 必须先于 `apply_snapshots` / `replace_graph` 调用。
fn allow_replace_after_assignees(assignees: Result<()>) -> Result<ReplaceAssigneesWriteStep> {
    assignees?;
    Ok(ReplaceAssigneesWriteStep::ApplySnapshotsAndReplaceGraph)
}

/// 陈旧锁立即失败，替换路径不得继续规划或写图。
///
/// # 错误
/// 非草稿或锁版本不匹配时返回冲突。
fn allow_prepare_replacement(
    definition: &ApprovalProcessDefinition,
    expected: u64,
) -> Result<ReplaceNodesWriteStep> {
    ensure_draft_lock(definition, expected)?;
    Ok(ReplaceNodesWriteStep::PrepareAndReplaceGraph)
}

/// CAS 冲突时不得把替换结果当成功写回。
///
/// # 错误
/// `VersionConflict` 映射为陈旧锁，其它失败原样返回。
fn allow_apply_replaced_definition(
    outcome: CasWriteOutcome<ApprovalProcessDefinition>,
) -> Result<ApprovalProcessDefinition> {
    applied_definition(outcome)
}

/// 收敛定义期审批人账号存在性与后台有效性。
///
/// # 参数
/// * `account` - 仓储按审批人 ID 返回的可选账号
///
/// # 返回
/// 返回可承担后台责任的有效账号引用。
///
/// # 错误
/// 账号缺失、已停用或不满足后台责任身份时返回校验错误。
///
/// # 关键业务约束
/// 账号类型与状态规则由 `AccountCore::is_active_backoffice` 唯一提供，本层只映射错误语义。
fn require_active_backoffice_assignee(account: Option<&AccountCore>) -> Result<&AccountCore> {
    account
        .filter(|item| item.is_active_backoffice())
        .ok_or_else(|| Error::ValidationError("指定审批人账号不存在、已停用或任职失效".to_string()))
}

/// 定义期静态 `approval_instance:decide` 闸门。
///
/// # 错误
/// 缺少静态审批权限时返回业务错误。
pub(super) fn ensure_static_decide_permission(has_decide: bool) -> Result<()> {
    if has_decide {
        return Ok(());
    }
    Err(Error::BusinessLogicError(
        "指定审批人不具备静态审批权限".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{draft_definition, production_source, source_fn};
    use super::*;
    use bpm::{ProcessKind, Timestamp};

    /// 陈旧锁或 VersionConflict 立即失败，不得继续规划或写图。
    #[test]
    fn stale_lock_has_no_partial_write() {
        let definition = draft_definition(ProcessKind::StockAdjustment, "n1");
        let stale = allow_prepare_replacement(&definition, 99).unwrap_err();
        assert_eq!(stale.code(), Some(ErrorCode::ApprovalDefinitionVersionConflict));
        let current = definition.definition_lock_version();
        assert!(matches!(
            allow_prepare_replacement(&definition, current),
            Ok(ReplaceNodesWriteStep::PrepareAndReplaceGraph)
        ));
        assert!(matches!(
            allow_apply_replaced_definition(CasWriteOutcome::VersionConflict(definition.clone())),
            Err(Error::Coded(ErrorCode::ApprovalDefinitionVersionConflict))
        ));
        let replace_tx = source_fn(
            production_source(),
            "async fn replace_nodes_tx",
            "async fn publish_tx",
        );
        let lock = replace_tx.find("reload_draft_for_cas").expect("CAS 重载");
        assert!(lock < replace_tx.find("prepare_replacement").expect("规划"));
        assert!(lock < replace_tx.find("apply_draft_graph").expect("写图"));
        let replace_graph_src = source_fn(production_source(), "async fn replace_graph", "fn plan_nodes");
        assert!(replace_graph_src.contains("allow_apply_replaced_definition"));
    }

    /// 已发布结构不可改。
    #[test]
    fn published_and_retired_are_immutable() {
        let mut definition = draft_definition(ProcessKind::StockAdjustment, "n1");
        definition
            .publish(
                ParticipantId::new("admin").unwrap(),
                Timestamp::from_unix_secs(2).unwrap(),
            )
            .unwrap();
        let error = ensure_draft_lock(&definition, definition.definition_lock_version()).unwrap_err();
        assert_eq!(error.code(), Some(ErrorCode::ApprovalDefinitionNotDraft));
        definition
            .retire(
                ParticipantId::new("admin").unwrap(),
                Timestamp::from_unix_secs(3).unwrap(),
            )
            .unwrap();
        assert!(ensure_draft_lock(&definition, 1).is_err());
    }

    /// 替换路径账号/权限失败不得写图。
    #[test]
    fn replace_assignee_failure_does_not_write_graph() {
        assert!(matches!(
            allow_replace_after_assignees(Ok(())),
            Ok(ReplaceAssigneesWriteStep::ApplySnapshotsAndReplaceGraph)
        ));
        assert!(matches!(
            allow_replace_after_assignees(Err(Error::ValidationError(
                "指定审批人账号不存在、已停用或任职失效".into()
            ))),
            Err(Error::ValidationError(message)) if message.contains("账号不存在")
        ));
        assert!(matches!(
            allow_replace_after_assignees(Err(Error::Forbidden("缺少静态审批权限".into()))),
            Err(Error::Forbidden(_))
        ));
        let prepare = source_fn(
            production_source(),
            "async fn prepare_replacement",
            "async fn prepare_publish_graph",
        );
        let gate = prepare
            .find("allow_replace_after_assignees")
            .expect("替换账号闸门");
        assert!(gate < prepare.find("apply_snapshots").expect("快照"));
        assert!(gate < prepare.find("rebuild_draft_graph").expect("写图规划"));
    }
}
