use application_core::AuditActor;
use bpm::graph::{
    CopiedNodeIdentity, DefinitionGraph, NewPopulatedDraftParams, assignee_ids, copy_nodes_for_definition,
};
use bpm::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId};
use bpm::model::ApprovalProcessDefinition;
use entity_core::BaseModel;
use erp_identity::{AuthorizationService, SharedRbacService};
use erp_permissions::approval_process;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::super::definition_dto::{CreateDefinitionDraftRequest, DefinitionDetailView, DraftSource};
use super::super::policy::{ProcessRequiredApprovalPolicy, require_process_required};
use super::ApprovalDefinitionService;
use super::command::{
    DefinitionCommandResultRef, DefinitionResultExpectation, PreparedDefinitionIdentity,
    create_draft_identity, map_model_error, now, parse_idempotency_key, participant,
    replay_prepared_definition_receipt, require_definition_access, write_definition_audit, write_receipt,
};
use super::mapping::detail_with_copies;
use super::replace::{next_transition_ids, validate_assignees};
use crate::error::{Error, ErrorCode, Result};
use crate::repository::BpmExt;
use crate::repository::approval_copy::ApprovalCopyExt;
use crate::service::approval::copies::validate_recipients;

impl ApprovalDefinitionService {
    /// 创建定义草稿。
    /// 返回新建草稿或同载荷回放的详情。
    ///
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
        self.commit_create_draft(&policy, &name, request, actor, identity).await
    }

    // 在唯一事务中创建草稿。
    // 已有活动草稿、缺发布源或写入失败时返回错误。
    async fn commit_create_draft(
        &self,
        policy: &ProcessRequiredApprovalPolicy,
        name: &str,
        request: CreateDefinitionDraftRequest,
        actor: &AuditActor,
        identity: PreparedDefinitionIdentity,
    ) -> Result<DefinitionDetailView> {
        let authorization = self.authorization.clone();
        let db = self.db.clone();
        let identity_service = self.identity.clone();
        let audit = std::sync::Arc::clone(&self.audit);
        let transaction_actor = actor.clone();
        let transaction_policy = policy.clone();
        let name = name.to_string();
        let transaction_identity = identity.clone();
        let client = db.client().clone();
        let outcome = client.with_transaction(move |executor| {
            Box::pin(async move {
                create_draft_tx(
                    &authorization,
                    &identity_service,
                    &db,
                    CreateDraftTxInput {
                        policy: &transaction_policy,
                        name: &name,
                        request: &request,
                        actor: &transaction_actor,
                        identity: &transaction_identity,
                        audit: audit.as_ref(),
                    },
                    executor,
                )
                .await
            })
        });

        self.recover_definition_command(
            outcome.await,
            &approval_process::CREATE_DEFINITION_DRAFT,
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
    /// 注入的审计端口。
    audit: &'a dyn crate::ports::WorkflowAuditPort,
}

// 事务内创建草稿。
// 已有收据必须原样回放，不得重复 persist。
//
// 回放收据或构造并持久化新草稿。
//
// 返回草稿详情。
//
// 已有活动草稿、缺发布源或写入失败时返回错误。
//
// `identity_service`：同一应用注入的共享身份与权限服务；`input`：政策、请求、摘要与操作人。
async fn create_draft_tx(
    authorization: &AuthorizationService,
    identity_service: &SharedRbacService,
    db: &Database,
    input: CreateDraftTxInput<'_>,
    executor: &mut dyn Executor,
) -> Result<DefinitionDetailView> {
    let CreateDraftTxInput { policy, request, actor, identity, audit, .. } = input;
    require_definition_access(
        authorization,
        actor,
        &approval_process::CREATE_DEFINITION_DRAFT,
        policy,
        executor,
    )
    .await?;

    if let Some(view) = replay_prepared_definition_receipt(
        db,
        identity,
        &DefinitionResultExpectation::ProcessKind(policy.process_kind),
        executor,
    )
    .await?
    {
        return Ok(view);
    }

    ensure_no_active_draft(db.bpm_workflow().find_active_draft(policy.process_kind, executor).await?)?;
    let graph = build_new_draft(authorization, identity_service, db, &input, executor).await?;

    let result_ref = DefinitionCommandResultRef::from_graph(&graph).encode();
    write_receipt(db, &identity.current, &result_ref, executor).await?;
    persist_new_draft(db, &graph, executor).await?;

    if request.draft_source == DraftSource::CurrentPublished {
        let source = db
            .bpm_workflow()
            .find_published_by_process_kind(policy.process_kind, executor)
            .await?
            .ok_or_else(second_draft_error)?;
        if let Some(mut config) = db.approval_copies().config(&source.base.id, executor).await? {
            validate_recipients(identity_service, &config.rules, executor).await?;
            config.base = BaseModel::new(graph.definition.base.id.clone());
            db.approval_copies().save_config(&config, executor).await?;
        }
    }

    write_definition_audit(audit, actor, "approval_definition.create_draft", &graph, None, None, executor)
        .await?;
    detail_with_copies(db, &graph, executor).await
}

// 按命令选择空草稿或当前发布图的副本，供创建草稿事务统一保存。
async fn build_new_draft(
    authorization: &AuthorizationService,
    identity_service: &SharedRbacService,
    db: &Database,
    input: &CreateDraftTxInput<'_>,
    session: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let version = next_definition_version(db, input.policy.process_kind, session).await?;
    match input.request.draft_source {
        DraftSource::Empty => empty_draft(input.policy, input.name, version, input.actor),
        DraftSource::CurrentPublished => {
            copy_published_draft(authorization, identity_service, db, input, version, session).await
        },
    }
}

// 从发起新设计的命令创建尚无节点的定义草稿，留待后续配置节点与连线。
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

// 从当前发布定义复制节点到新草稿。
// Service 只生成新 ID 与查询账号，节点复制、用途清理、入口和连线规则全部由 BPM 提供。
//
// 返回 BPM 构造并校验的新草稿图。
//
// 缺少发布源、审批人失效或 BPM 构图失败时返回错误。
//
// `identity_service`：同一应用注入的共享身份与权限服务；`input`：当前单据政策、已规范化名称及认证操作人。
// `version`：新草稿业务版本；`session`：调用方事务执行器。
async fn copy_published_draft(
    authorization: &AuthorizationService,
    identity_service: &SharedRbacService,
    db: &Database,
    input: &CreateDraftTxInput<'_>,
    version: u32,
    session: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let CreateDraftTxInput { policy, name, actor, .. } = *input;
    let source = require_current_published(
        db.bpm_workflow().load_published_definition_graph(policy.process_kind, session).await?,
    )?;
    source.validate_published_linear().map_err(map_model_error)?;

    let definition_id = ApprovalProcessDefinitionId::new(next_id());
    let at = now()?;
    let identities = next_copied_node_identities(source.nodes.len());
    let nodes = copy_nodes_for_definition(&source.nodes, definition_id.clone(), &identities, at)
        .map_err(map_model_error)?;
    validate_assignees(authorization, db, identity_service, &assignee_ids(&nodes), session).await?;
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

// 保存本次创建的草稿及冻结节点和连线；复用创建事务，不能只写定义头。
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

// 为复制节点生成调用方负责的新身份。
// BPM 禁止自行生成 ID，因此身份生成保留在 Service 编排边界。
//
// 返回与源节点一一对应的新节点 ID 与不可预测节点键。
fn next_copied_node_identities(count: usize) -> Vec<CopiedNodeIdentity> {
    (0..count)
        .map(|_| CopiedNodeIdentity {
            node_id: ApprovalNodeDefinitionId::new(next_id()),
            node_key: next_id(),
        })
        .collect()
}

// 读取最高持久化版本并计算下一业务版本。
// Repository 只提供最高版本事实；递增、溢出与事务边界仍分别由 BPM 模型和 Service 持有。
//
// 返回历史最高版本之后的单调递增业务版本。
//
// Repository 查询或 BPM 版本溢出校验失败时返回错误。
//
// `session`：调用方事务执行器。
async fn next_definition_version(
    db: &Database,
    process_kind: bpm::ProcessKind,
    session: &mut dyn Executor,
) -> Result<u32> {
    let current = db.bpm_workflow().latest_definition_version(process_kind, session).await?.unwrap_or(0);
    ApprovalProcessDefinition::next_version_after(current).map_err(map_model_error)
}

// 已有活动草稿则冲突，不得进入 persist_new_draft / write_receipt。
// 已存在活动草稿时返回冲突。
fn ensure_no_active_draft<T>(existing_active_draft: Option<T>) -> Result<()> {
    if existing_active_draft.is_some() {
        return Err(second_draft_error());
    }
    Ok(())
}

// `CURRENT_PUBLISHED` 必须命中当前发布定义，缺失则失败关闭。
// 当前没有已发布定义时返回校验错误。
fn require_current_published<T>(published: Option<T>) -> Result<T> {
    published.ok_or_else(|| Error::from_approval_code(ErrorCode::ApprovalDraftSourceNotAvailable))
}

// 已有活动草稿时返回稳定的定义冲突，提示先处理旧草稿再新建。
fn second_draft_error() -> Error {
    Error::ConflictError("该单据类型已有活动草稿".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // 验证：已有活动草稿时不得 persist_new_draft / write_receipt。
    #[test]
    fn second_active_draft_is_conflict() {
        assert!(matches!(
            ensure_no_active_draft(Some("draft-1")),
            Err(Error::ConflictError(message)) if message.contains("活动草稿")
        ));
        assert!(matches!(ensure_no_active_draft::<&str>(None), Ok(())));
    }

    // 验证：draft_source=CURRENT_PUBLISHED 缺发布源必须失败关闭。
    #[test]
    fn current_published_requires_existing_definition() {
        assert!(matches!(
            require_current_published::<&str>(None),
            Err(Error::Coded(ErrorCode::ApprovalDraftSourceNotAvailable))
        ));
        assert_eq!(require_current_published(Some("def-pub")).unwrap(), "def-pub");
    }
}

#[cfg(test)]
mod authorization_tests {
    use persistence_core::testing::SnapshotExecutor;

    use super::super::test_support::unavailable_service;
    use super::*;
    use crate::entity::document_registry::DocumentType;

    #[tokio::test]
    async fn create_rejects_before_receipt_replay_and_persistence() {
        let (service, actor) = unavailable_service().await;
        let policy = require_process_required(DocumentType::SalesOrder).unwrap();
        let request = CreateDefinitionDraftRequest {
            document_type: DocumentType::SalesOrder,
            name: "审批流程".into(),
            draft_source: DraftSource::Empty,
            idempotency_key: "definition-create".into(),
        };
        let identity = create_draft_identity(
            parse_idempotency_key(&request.idempotency_key).unwrap(),
            request.document_type,
            &request.name,
            request.draft_source,
            actor.id(),
        )
        .unwrap();

        let error = create_draft_tx(
            &service.authorization,
            &service.identity,
            &service.db,
            CreateDraftTxInput {
                policy: &policy,
                name: &request.name,
                request: &request,
                actor: &actor,
                identity: &identity,
                audit: service.audit.as_ref(),
            },
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }
}
