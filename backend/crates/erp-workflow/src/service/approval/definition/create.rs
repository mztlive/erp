use application_core::AuditActor;
use bpm::graph::{
    CopiedNodeIdentity, DefinitionGraph, NewPopulatedDraftParams, assignee_ids, copy_nodes_for_definition,
};
use bpm::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId};
use bpm::model::ApprovalProcessDefinition;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::super::definition_dto::{CreateDefinitionDraftRequest, DefinitionDetailView, DraftSource};
use super::super::policy::{ProcessRequiredApprovalPolicy, require_process_required};
use super::ApprovalDefinitionService;
use super::command::{
    DefinitionCommandResultRef, DefinitionResultExpectation, PreparedDefinitionIdentity,
    create_draft_identity, ensure_definition_admin_permission, map_model_error, now, parse_idempotency_key,
    participant, replay_prepared_definition_receipt, write_definition_audit, write_receipt,
};
use super::mapping::detail_view;
use super::replace::{next_transition_ids, validate_assignees};
use crate::error::{Error, ErrorCode, Result};
use crate::repository::BpmExt;

impl<A: crate::ports::WorkflowAuthorizationPort> ApprovalDefinitionService<A> {
    /// 创建定义草稿。
    ///
    /// # 参数
    /// * `request` - 写请求
    /// * `actor` - 已认证操作人
    ///
    /// # 错误
    /// 政策、权限、幂等冲突或已有活动草稿时返回错误。
    pub async fn create_definition_draft(
        &self,
        mut request: CreateDefinitionDraftRequest,
        actor: &AuditActor,
    ) -> Result<DefinitionDetailView> {
        let key = parse_idempotency_key(&request.idempotency_key)?;
        let policy = require_process_required(request.document_type)?;
        let name =
            ApprovalProcessDefinition::normalize_name(request.name.clone()).map_err(map_model_error)?;
        request.name.clone_from(&name);
        request.idempotency_key = key.as_str().to_string();
        let identity =
            create_draft_identity(key, request.document_type, &name, request.draft_source, actor.id())?;
        self.ensure_definition_admin(actor, &policy).await?;
        if let Some(view) = self
            .replay_prepared_if_receipt(
                &identity,
                DefinitionResultExpectation::ProcessKind(policy.process_kind),
            )
            .await?
        {
            return Ok(view);
        }
        self.commit_create_draft(&policy, &name, request, actor, identity).await
    }

    /// 在唯一事务中创建草稿。
    ///
    /// # 错误
    /// 已有活动草稿、缺发布源或写入失败时返回错误。
    async fn commit_create_draft(
        &self,
        policy: &ProcessRequiredApprovalPolicy,
        name: &str,
        request: CreateDefinitionDraftRequest,
        actor: &AuditActor,
        identity: PreparedDefinitionIdentity,
    ) -> Result<DefinitionDetailView> {
        let db = self.db.clone();
        let rbac = self.auth.clone();
        let audit = std::sync::Arc::clone(&self.audit);
        let transaction_actor = actor.clone();
        let transaction_policy = policy.clone();
        let name = name.to_string();
        let transaction_identity = identity.clone();
        let client = db.client().clone();
        let outcome = client.with_transaction(move |session| {
            Box::pin(async move {
                create_draft_tx(
                    &db,
                    &rbac,
                    CreateDraftTxInput {
                        policy: &transaction_policy,
                        name: &name,
                        request: &request,
                        actor: &transaction_actor,
                        identity: &transaction_identity,
                        audit: audit.as_ref(),
                    },
                    session,
                )
                .await
            })
        });
        self.recover_definition_command(
            outcome.await,
            policy,
            actor,
            identity,
            DefinitionResultExpectation::ProcessKind(policy.process_kind),
        )
        .await
    }
}

/// 创建草稿事务的政策、请求、命令身份与操作人。
///
/// # 用途
/// 将草稿创建命令上下文字段打包。
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
/// 同一流程种类只允许一个活动草稿。
struct CreateDraftTxInput<'a> {
    /// 单据审批政策。
    policy: &'a ProcessRequiredApprovalPolicy,
    /// 已规范化的定义名称。
    name: &'a str,
    /// 创建草稿请求。
    request: &'a CreateDefinitionDraftRequest,
    /// 审计操作人。
    actor: &'a AuditActor,
    /// 已规范化的当前命令身份及精确旧格式候选。
    identity: &'a PreparedDefinitionIdentity,
    /// Injected audit port.
    audit: &'a dyn crate::ports::WorkflowAuditPort,
}

/// 事务内创建草稿。
///
/// # 用途
/// 回放收据或构造并持久化新草稿。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `input` - 政策、请求、摘要与操作人
/// * `session` - 事务会话
///
/// # 返回
/// 返回草稿详情。
///
/// # 错误
/// 已有活动草稿、缺发布源或写入失败时返回错误。
///
/// # 关键业务约束
/// 已有收据必须原样回放，不得重复 persist。
async fn create_draft_tx(
    db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    input: CreateDraftTxInput<'_>,
    session: &mut mongodb::ClientSession,
) -> Result<DefinitionDetailView> {
    let CreateDraftTxInput { policy, name, request, actor, identity, audit } = input;
    ensure_definition_admin_permission(rbac, actor, policy, session).await?;
    if let Some(view) = replay_prepared_definition_receipt(
        db,
        identity,
        &DefinitionResultExpectation::ProcessKind(policy.process_kind),
        session,
    )
    .await?
    {
        return Ok(view);
    }
    let CreateDraftWriteStep::PersistNewDraftAndReceipt =
        decide_create_draft_write(db.bpm_workflow().find_active_draft(policy.process_kind, session).await?)?;
    let graph = build_new_draft(db, rbac, policy, name, request.draft_source, actor, session).await?;
    let result_ref = DefinitionCommandResultRef::from_graph(&graph).encode();
    write_receipt(db, &identity.current, &result_ref, session).await?;
    persist_new_draft(db, &graph, session).await?;
    write_definition_audit(audit, actor, "approval_definition.create_draft", &graph, None, None, session)
        .await?;
    Ok(detail_view(&graph))
}

/// 构造空草稿或从当前发布复制。
async fn build_new_draft(
    db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    policy: &ProcessRequiredApprovalPolicy,
    name: &str,
    draft_source: DraftSource,
    actor: &AuditActor,
    session: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let version = next_definition_version(db, policy.process_kind, session).await?;
    match draft_source {
        DraftSource::Empty => empty_draft(policy, name, version, actor),
        DraftSource::CurrentPublished => {
            copy_published_draft(db, rbac, policy, name, version, actor, session).await
        },
    }
}

/// 创建无节点草稿。
fn empty_draft(
    policy: &ProcessRequiredApprovalPolicy,
    name: &str,
    version: u32,
    actor: &AuditActor,
) -> Result<DefinitionGraph> {
    let definition = ApprovalProcessDefinition::new_draft(
        ApprovalProcessDefinitionId::new(next_id()),
        policy.process_kind,
        version,
        name,
        next_id(),
        participant(actor)?,
        now()?,
    )
    .map_err(map_model_error)?;
    Ok(DefinitionGraph { definition, nodes: Vec::new(), transitions: Vec::new() })
}

/// 从当前发布定义复制节点到新草稿。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `policy` - 当前单据类型必须审批政策
/// * `name` - 已由 BPM 规范化的草稿名称
/// * `version` - 新草稿业务版本
/// * `actor` - 已认证定义管理员
/// * `session` - 调用方事务执行器
///
/// # 返回
/// 返回 BPM 构造并校验的新草稿图。
///
/// # 错误
/// 缺少发布源、审批人失效或 BPM 构图失败时返回错误。
///
/// # 关键业务约束
/// Service 只生成新 ID 与查询账号，节点复制、用途清理、入口和连线规则全部由 BPM 提供。
async fn copy_published_draft(
    db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    policy: &ProcessRequiredApprovalPolicy,
    name: &str,
    version: u32,
    actor: &AuditActor,
    session: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let source = require_current_published(
        db.bpm_workflow().load_published_definition_graph(policy.process_kind, session).await?,
    )?;
    source.validate_published_linear().map_err(map_model_error)?;
    let definition_id = ApprovalProcessDefinitionId::new(next_id());
    let at = now()?;
    let identities = next_copied_node_identities(source.nodes.len());
    let nodes = copy_nodes_for_definition(&source.nodes, definition_id.clone(), &identities, at)
        .map_err(map_model_error)?;
    validate_assignees(db, rbac, policy, &assignee_ids(&nodes), session).await?;
    let transition_ids = next_transition_ids(nodes.len());
    DefinitionGraph::new_populated_draft(NewPopulatedDraftParams {
        definition_id,
        process_kind: policy.process_kind,
        definition_version: version,
        name: name.into(),
        created_by: participant(actor)?,
        nodes,
        transition_ids,
        at,
    })
    .map_err(map_model_error)
}

/// 持久化新建草稿及其图。
async fn persist_new_draft(db: &Database, graph: &DefinitionGraph, session: &mut dyn Executor) -> Result<()> {
    db.approval_process_definitions().create(&graph.definition, session).await?;
    for node in &graph.nodes {
        db.approval_node_definitions().create(node, session).await?;
    }
    for transition in &graph.transitions {
        db.approval_transition_definitions().create(transition, session).await?;
    }
    Ok(())
}

/// 为复制节点生成调用方负责的新身份。
///
/// # 参数
/// * `count` - 需要复制的节点数量
///
/// # 返回
/// 返回与源节点一一对应的新节点 ID 与不可预测节点键。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// BPM 禁止自行生成 ID，因此身份生成保留在 Service 编排边界。
fn next_copied_node_identities(count: usize) -> Vec<CopiedNodeIdentity> {
    (0..count)
        .map(|_| CopiedNodeIdentity {
            node_id: ApprovalNodeDefinitionId::new(next_id()),
            node_key: next_id(),
        })
        .collect()
}

/// 读取最高持久化版本并计算下一业务版本。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `process_kind` - BPM 流程种类
/// * `session` - 调用方事务执行器
///
/// # 返回
/// 返回历史最高版本之后的单调递增业务版本。
///
/// # 错误
/// Repository 查询或 BPM 版本溢出校验失败时返回错误。
///
/// # 关键业务约束
/// Repository 只提供最高版本事实；递增、溢出与事务边界仍分别由 BPM 模型和 Service 持有。
async fn next_definition_version(
    db: &Database,
    process_kind: bpm::ProcessKind,
    session: &mut dyn Executor,
) -> Result<u32> {
    let current = db.bpm_workflow().latest_definition_version(process_kind, session).await?.unwrap_or(0);
    ApprovalProcessDefinition::next_version_after(current).map_err(map_model_error)
}

/// 创建草稿写库步骤。仅在无活动草稿时允许持久化。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CreateDraftWriteStep {
    /// 允许 persist_new_draft 与 write_receipt。
    PersistNewDraftAndReceipt,
}

/// 已有活动草稿则冲突，不得进入 persist_new_draft / write_receipt。
///
/// # 错误
/// 已存在活动草稿时返回冲突。
fn decide_create_draft_write<T>(existing_active_draft: Option<T>) -> Result<CreateDraftWriteStep> {
    if existing_active_draft.is_some() {
        return Err(second_draft_error());
    }
    Ok(CreateDraftWriteStep::PersistNewDraftAndReceipt)
}

/// `CURRENT_PUBLISHED` 必须命中当前发布定义，缺失则失败关闭。
///
/// # 错误
/// 当前没有已发布定义时返回校验错误。
fn require_current_published<T>(published: Option<T>) -> Result<T> {
    published.ok_or_else(|| Error::from_approval_code(ErrorCode::ApprovalDraftSourceNotAvailable))
}

/// 第二活动草稿错误。
fn second_draft_error() -> Error {
    Error::ConflictError("该单据类型已有活动草稿".to_string())
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{production_source, source_fn};
    use super::*;

    /// 已有活动草稿时不得 persist_new_draft / write_receipt。
    #[test]
    fn second_active_draft_is_conflict() {
        assert!(matches!(
            decide_create_draft_write(Some("draft-1")),
            Err(Error::ConflictError(message)) if message.contains("活动草稿")
        ));
        assert!(matches!(
            decide_create_draft_write::<&str>(None),
            Ok(CreateDraftWriteStep::PersistNewDraftAndReceipt)
        ));
        let create_tx =
            source_fn(production_source(), "async fn create_draft_tx", "async fn replace_nodes_tx");
        let gate = create_tx.find("decide_create_draft_write").expect("创建闸门");
        let receipt = create_tx.find("write_receipt").expect("receipt");
        assert!(gate < receipt);
        assert!(receipt < create_tx.find("persist_new_draft").expect("persist"));
    }

    /// draft_source=CURRENT_PUBLISHED 缺发布源必须失败关闭。
    #[test]
    fn current_published_requires_existing_definition() {
        assert!(matches!(
            require_current_published::<&str>(None),
            Err(Error::Coded(ErrorCode::ApprovalDraftSourceNotAvailable))
        ));
        assert_eq!(require_current_published(Some("def-pub")).unwrap(), "def-pub");
        let copy_src =
            source_fn(production_source(), "async fn copy_published_draft", "async fn persist_new_draft");
        assert!(copy_src.contains("require_current_published"));
        assert!(copy_src.contains("load_published_definition_graph"));
        assert!(!copy_src.contains("load_definition_graph"));
        assert!(copy_src.contains("validate_published_linear"));
    }
}
