use std::collections::HashMap;

use application_core::AuditActor;
use bpm::ParticipantId;
use bpm::graph::{DefinitionGraph, NodeReplacementDraft, assignee_ids};
use bpm::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId, ApprovalTransitionDefinitionId};
use bpm::model::{ApprovalNodeDefinition, ApprovalParticipant, ApprovalProcessDefinition};
use erp_core::AccountKind;
use erp_identity::{AccountSummary, SharedRbacService};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::super::assignee_source::AssigneeSource;
use super::super::definition_dto::{
    DefinitionDetailView, DefinitionNodeRequest, ReplaceDefinitionNodesRequest,
};
use super::super::policy::ProcessRequiredApprovalPolicy;
use super::ApprovalDefinitionService;
use super::command::{
    DefinitionCommandResultRef, DefinitionResultExpectation, PreparedDefinitionIdentity, applied_definition,
    definition_not_found, ensure_definition_admin_permission, ensure_lock, map_bpm_error, map_model_error,
    node_summary, now, parse_idempotency_key, policy_for_definition, replace_nodes_identity,
    replay_prepared_definition_receipt, write_definition_audit, write_receipt,
};
use super::mapping::detail_with_copies;
use crate::entity::approval_copy::ApprovalCopyRule;
use crate::entity::approval_qualification::ApprovalQualification;
use crate::error::{Error, ErrorCode, Result};
use crate::repository::BpmExt;
use crate::repository::approval_copy::ApprovalCopyExt;
#[cfg(test)]
use crate::repository::bpm::CasWriteOutcome;
use crate::service::approval::copies::{save_config, validate_recipients};
use crate::service::approval::scope::approver_qualification;

impl ApprovalDefinitionService {
    /// 整组替换草稿节点。
    ///
    /// # 参数
    /// * `request` - 节点替换请求
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回替换后的草稿详情；同载荷回放返回已有结果。
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
        request.copy_rules = request
            .copy_rules
            .map(|rules| ApprovalCopyRule::prepare(rules, request.nodes.len()))
            .transpose()?;
        request.idempotency_key = key.as_str().to_string();
        let identity = replace_nodes_identity(
            key,
            &request.definition_id,
            request.expected_definition_lock_version,
            &request.nodes,
            request.copy_rules.as_deref(),
            actor.id(),
        )?;
        let graph = self.require_graph(&request.definition_id).await?;
        let policy = policy_for_definition(&graph.definition)?;
        self.ensure_definition_admin(actor, &policy).await?;
        if let Some(view) = self
            .replay_prepared_if_receipt(
                &identity,
                DefinitionResultExpectation::DefinitionId(request.definition_id.clone()),
            )
            .await?
        {
            return Ok(view);
        }
        ensure_draft_lock(&graph.definition, request.expected_definition_lock_version)?;
        self.commit_replace_nodes(graph, policy, request, actor, identity).await
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

        let identity_service = self.identity.clone();
        let audit = std::sync::Arc::clone(&self.audit);
        let transaction_actor = actor.clone();
        let transaction_policy = policy.clone();
        let transaction_identity = identity.clone();
        let expectation = DefinitionResultExpectation::DefinitionId(request.definition_id.clone());
        let outcome = db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    replace_nodes_tx(
                        &identity_service,
                        &db,
                        graph,
                        ReplaceNodesTxInput {
                            policy: transaction_policy,
                            request,
                            actor: &transaction_actor,
                            identity: &transaction_identity,
                            audit: audit.as_ref(),
                        },
                        executor,
                    )
                    .await
                })
            })
            .await;
        self.recover_definition_command(outcome, &policy, actor, identity, expectation).await
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
    /// 注入的审计端口。
    audit: &'a dyn crate::ports::WorkflowAuditPort,
}

/// 事务内替换草稿节点。
///
/// # 用途
/// 回放收据或校验并持久化新草稿图。
///
/// # 参数
/// * `identity_service` - 同一应用注入的共享身份与权限服务。
/// * `db` - 数据库
/// * `graph` - 当前草稿图
/// * `input` - 政策、请求、摘要与操作人
/// * `executor` - 执行器
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
    identity_service: &SharedRbacService,
    db: &Database,
    graph: DefinitionGraph,
    input: ReplaceNodesTxInput<'_>,
    executor: &mut dyn Executor,
) -> Result<DefinitionDetailView> {
    let ReplaceNodesTxInput { policy, request, actor, identity, audit } = input;
    ensure_definition_admin_permission(db, identity_service, actor, &policy, executor).await?;
    if let Some(view) = replay_prepared_definition_receipt(
        db,
        identity,
        &DefinitionResultExpectation::DefinitionId(request.definition_id.clone()),
        executor,
    )
    .await?
    {
        return Ok(view);
    }
    let reloaded =
        reload_draft_for_cas(db, &graph.definition, request.expected_definition_lock_version, executor)
            .await?;
    let prepared = prepare_replacement(db, identity_service, &reloaded, &request.nodes, executor).await?;
    let repository = db.approval_copies();
    let rules = match request.copy_rules {
        Some(rules) => rules,
        None => preserve_copy_rules(
            repository.config(&request.definition_id, executor).await?.map_or_else(Vec::new, |c| c.rules),
            &reloaded.nodes,
            &prepared.nodes,
        )?,
    };
    validate_recipients(identity_service, &rules, executor).await?;
    let result_ref = DefinitionCommandResultRef::from_graph(&prepared).encode();
    write_receipt(db, &identity.current, &result_ref, executor).await?;
    save_config(identity_service, db, &prepared, rules, executor).await?;
    apply_draft_graph(
        db,
        audit,
        prepared,
        request.expected_definition_lock_version,
        actor,
        "approval_definition.replace_nodes",
        Some(node_summary(&reloaded.nodes)),
        executor,
    )
    .await
}

/// 旧客户端省略配置时按节点身份保留规则，禁止随顺序变更抄送到其他节点。
fn preserve_copy_rules(
    mut rules: Vec<ApprovalCopyRule>,
    previous: &[ApprovalNodeDefinition],
    next: &[ApprovalNodeDefinition],
) -> Result<Vec<ApprovalCopyRule>> {
    for rule in &mut rules {
        let Some(order) = rule.node_order else { continue };
        let old_node = previous.iter().find(|node| node.display_order == order);
        let new_node = old_node.and_then(|old| next.iter().find(|node| node.base.id == old.base.id));
        rule.node_order = Some(
            new_node
                .ok_or_else(|| Error::ValidationError("删除抄送关联节点时须同时更新抄送规则".into()))?
                .display_order,
        );
    }
    ApprovalCopyRule::prepare(rules, next.len())
}

/// 重新加载草稿并核对锁版本。
///
/// # 参数
/// * `db` - 数据库
/// * `original` - 事务外读到的定义
/// * `expected` - 期望的定义锁版本
/// * `session` - 调用方执行器
///
/// # 返回
/// 定义仍可修改且锁版本一致时返回当前图。
///
/// # 错误
/// 定义缺失、不是草稿或锁版本陈旧时返回错误；仓储读取失败时返回对应错误。
pub(super) async fn reload_draft_for_cas(
    db: &Database,
    original: &ApprovalProcessDefinition,
    expected: u64,
    session: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let graph = db
        .bpm_workflow()
        .load_definition_graph(&ApprovalProcessDefinitionId::new(original.base.id.clone()), session)
        .await?
        .ok_or_else(definition_not_found)?;
    ensure_draft_lock(&graph.definition, expected)?;
    Ok(graph)
}

/// 按请求构造替换后的草稿图。
///
/// # 参数
/// * `identity_service` - 同一应用注入的共享身份与权限服务。
/// * `graph` - 当前草稿图
/// * `requests` - 客户端整组节点请求
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
    identity_service: &SharedRbacService,
    graph: &DefinitionGraph,
    requests: &[DefinitionNodeRequest],
    session: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let drafts = node_replacement_drafts(requests)?;
    let planned = graph.plan_replacement_nodes(&drafts, now()?).map_err(map_model_error)?;
    let snapshots = validate_assignees(db, identity_service, &assignee_ids(&planned), session).await?;
    let nodes = apply_snapshots(planned, &snapshots)?;
    rebuild_draft_graph(&graph.definition, nodes)
}

/// 以 CAS 写回草稿图并记录审计。
// 草稿落盘 8 参数：db/审计/图/版本/操作人/动作/摘要/执行器随事务一体传递，拆包增加错序风险；告警逐项压制。
#[allow(clippy::too_many_arguments)]
async fn apply_draft_graph(
    db: &Database,
    audit: &dyn crate::ports::WorkflowAuditPort,
    graph: DefinitionGraph,
    expected: u64,
    actor: &AuditActor,
    action: &str,
    before_summary: Option<String>,
    session: &mut dyn Executor,
) -> Result<DefinitionDetailView> {
    let applied = replace_graph(db, graph, expected, session).await?;
    write_definition_audit(
        audit,
        actor,
        action,
        &applied,
        Some(expected),
        before_summary.as_deref(),
        session,
    )
    .await?;
    detail_with_copies(db, &applied, session).await
}

/// 调用仓储整组替换草稿图。
///
/// # 参数
/// * `db` - 数据库
/// * `graph` - 待写回的草稿图
/// * `expected` - 期望的定义锁版本
/// * `session` - 调用方执行器
///
/// # 返回
/// 返回 CAS 成功后的定义图；节点与连线沿用入参。
///
/// # 错误
/// 仓储写入失败，或 CAS 结果不是成功应用时返回错误。
pub(super) async fn replace_graph(
    db: &Database,
    graph: DefinitionGraph,
    expected: u64,
    session: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let outcome = db
        .bpm_workflow()
        .replace_draft_graph(&graph.definition, &graph.nodes, &graph.transitions, expected, session)
        .await?;
    Ok(DefinitionGraph {
        definition: applied_definition(outcome)?,
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
            let mut draft = NodeReplacementDraft::new(
                existing_node_id,
                ApprovalNodeDefinitionId::new(next_id()),
                next_id(),
                request.node_name.clone(),
                request.display_order,
                if request.assignee_source.rule().is_some() {
                    None
                } else {
                    Some(ParticipantId::new(request.assignee_user_id.clone()).map_err(map_bpm_error)?)
                },
            )
            .map_err(map_model_error)?;
            draft.assignee_rule = request.assignee_source.rule().map(str::to_owned);
            draft.additional_assignees = request
                .additional_assignee_user_ids
                .iter()
                .map(|id| {
                    Ok(ApprovalParticipant {
                        participant_id: ParticipantId::new(id.clone()).map_err(map_bpm_error)?,
                        name: "pending".to_string(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(draft)
        })
        .collect()
}
/// 在同一执行器内校验审批人并返回该次取证的账号摘要。
/// # 参数
/// identity_service 为共享身份服务，user_ids 为 BPM 提取的有序账号，executor 为原事务。
/// # 返回
/// 已核验账号和同角色读写资格的显示资料，不再次查询账号目录。
/// # 错误
/// 账号失效返回校验错误，权限不足返回业务错误；基础设施失败原样传播。
pub(super) async fn validate_assignees(
    db: &Database,
    identity_service: &SharedRbacService,
    user_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, AccountSummary>> {
    let mut accounts = HashMap::new();
    for user_id in user_ids {
        let actor = AuditActor::new(user_id.clone(), user_id.clone(), AccountKind::Admin);
        let account = match approver_qualification(db, identity_service, &actor, executor).await? {
            ApprovalQualification::Inactive => {
                return Err(Error::ValidationError("指定审批人账号不存在、已停用或任职失效".into()));
            },
            ApprovalQualification::MissingPermissions => {
                return Err(Error::BusinessLogicError("指定审批人不具备静态审批权限".into()));
            },
            ApprovalQualification::Eligible(account) => account,
        };
        accounts.insert(user_id.clone(), account);
    }
    Ok(accounts)
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
    accounts: &HashMap<String, AccountSummary>,
) -> Result<Vec<ApprovalNodeDefinition>> {
    let at = now()?;
    let mut refreshed = Vec::with_capacity(nodes.len());
    for node in nodes {
        if node.assignee_rule.is_some() {
            let source = AssigneeSource::from_rule(node.assignee_rule.as_deref())?;
            let mut node = node;
            node.assignee_label_snapshot = source.label().into();
            refreshed.push(node);
            continue;
        }
        let account = accounts
            .get(node.assignee().map_err(map_model_error)?.as_str())
            .ok_or_else(|| Error::ValidationError("指定审批人账号不存在、已停用或任职失效".to_string()))?;
        let additional = node
            .additional_assignees
            .iter()
            .map(|person| {
                let account = accounts
                    .get(person.participant_id.as_str())
                    .ok_or_else(|| Error::ValidationError("指定审批人账号不存在、已停用或任职失效".into()))?;
                ApprovalParticipant::new(person.participant_id.clone(), account.display_name.clone())
                    .map_err(map_model_error)
            })
            .collect::<Result<Vec<_>>>()?;
        refreshed.push(
            node.with_assignee_label_snapshot(account.display_name.clone(), at)
                .map_err(map_model_error)?
                .with_additional_assignees(additional)
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
    (0..node_count.saturating_mul(2)).map(|_| ApprovalTransitionDefinitionId::new(next_id())).collect()
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
///
/// # 参数
/// * `nodes` - 客户端节点请求
///
/// # 返回
/// 返回按 `display_order` 排序的已清洗节点。
///
/// # 错误
/// 单个节点 `prepare` 失败时映射为模型错误。
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

#[cfg(test)]
mod tests {
    use bpm::model::NewNodeDefinition;
    use bpm::{ProcessKind, Timestamp};

    use super::super::test_support::draft_definition;
    use super::*;
    use crate::entity::approval_copy::CopyTrigger;

    #[test]
    fn snapshots_refresh_all_assignees_and_reject_missing_or_invalid_names() {
        let mut graph = super::super::test_support::two_node_publish_graph();
        graph.nodes[0] = graph.nodes[0]
            .clone()
            .with_additional_assignees(vec![
                ApprovalParticipant::new(ParticipantId::new("u3").unwrap(), "旧或签人").unwrap(),
            ])
            .unwrap();
        let mut accounts = [("u3", "或签人"), ("u2", "复核人"), ("u1", "初审人")]
            .into_iter()
            .map(|(id, name)| {
                (id.to_owned(), AccountSummary::new(id, AccountKind::Admin, true).with_display_name(name))
            })
            .collect::<HashMap<_, _>>();
        let refreshed = apply_snapshots(graph.nodes.clone(), &accounts).unwrap();
        assert_eq!(refreshed[0].assignee_label_snapshot, "初审人");
        assert_eq!(refreshed[0].additional_assignees[0].name, "或签人");
        assert_eq!(refreshed[1].assignee_label_snapshot, "复核人");
        accounts.remove("u3");
        assert!(matches!(apply_snapshots(graph.nodes.clone(), &accounts),
            Err(Error::ValidationError(message)) if message == "指定审批人账号不存在、已停用或任职失效"));
        accounts.insert("u3".into(), AccountSummary::new("u3", AccountKind::Admin, true));
        assert!(apply_snapshots(graph.nodes, &accounts).is_err());
    }

    #[test]
    fn omitted_copy_rules_follow_node_identity_and_reject_deleted_nodes() {
        let node = |id: &str, display_order| {
            ApprovalNodeDefinition::new(NewNodeDefinition {
                id: ApprovalNodeDefinitionId::new(id),
                process_definition_id: ApprovalProcessDefinitionId::new("definition"),
                node_key: id.into(),
                node_name: id.into(),
                node_purpose: None,
                display_order,
                assignee_participant_id: Some(ParticipantId::new("approver").unwrap()),
                assignee_label_snapshot: "审批人".into(),
                at: Timestamp::from_unix_secs(1).unwrap(),
            })
            .unwrap()
        };
        let old = vec![node("first", 1), node("second", 2)];
        let reordered = vec![node("second", 1), node("first", 2)];
        let rule = ApprovalCopyRule {
            trigger: CopyTrigger::NodeApproved,
            node_order: Some(1),
            recipient_user_ids: vec!["reader".into()],
        };
        let preserved = preserve_copy_rules(vec![rule.clone()], &old, &reordered).unwrap();
        assert_eq!(preserved[0].node_order, Some(2));
        assert!(preserve_copy_rules(vec![rule], &old, &[node("second", 1)]).is_err());
    }

    /// 陈旧锁或 VersionConflict 立即失败，不得继续规划或写图。
    #[test]
    fn stale_lock_has_no_partial_write() {
        let definition = draft_definition(ProcessKind::StockAdjustment, "n1");
        let stale = ensure_draft_lock(&definition, 99).unwrap_err();
        assert_eq!(stale.code(), Some(ErrorCode::ApprovalDefinitionVersionConflict));
        let current = definition.definition_lock_version();
        assert!(matches!(ensure_draft_lock(&definition, current), Ok(())));
        assert!(matches!(
            applied_definition(CasWriteOutcome::VersionConflict(definition.clone())),
            Err(Error::Coded(ErrorCode::ApprovalDefinitionVersionConflict))
        ));
    }

    /// 已发布结构不可改。
    #[test]
    fn published_and_retired_are_immutable() {
        let mut definition = draft_definition(ProcessKind::StockAdjustment, "n1");
        definition
            .publish(ParticipantId::new("admin").unwrap(), Timestamp::from_unix_secs(2).unwrap())
            .unwrap();
        let error = ensure_draft_lock(&definition, definition.definition_lock_version()).unwrap_err();
        assert_eq!(error.code(), Some(ErrorCode::ApprovalDefinitionNotDraft));
        definition
            .retire(ParticipantId::new("admin").unwrap(), Timestamp::from_unix_secs(3).unwrap())
            .unwrap();
        assert!(ensure_draft_lock(&definition, 1).is_err());
    }
}
