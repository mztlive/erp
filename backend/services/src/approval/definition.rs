//! 审批流程定义管理：草稿、节点替换、发布、退役与版本查询。
//!
//! 图算法只编排 `bpm::graph` 已交付原语，不在本模块复制或另立定义源。

use std::{collections::HashMap, time::Duration};

use bpm::graph::{
    assignee_ids, copy_nodes_for_definition, CopiedNodeIdentity, DefinitionGraph, NodeReplacementDraft,
};
use bpm::ids::{
    ApprovalCommandReceiptId, ApprovalNodeDefinitionId, ApprovalProcessDefinitionId,
    ApprovalTransitionDefinitionId,
};
use bpm::model::types::{ApprovalCommandKind, ApprovalDefinitionStatus, ModelError};
use bpm::model::{
    ApprovalCommandIdentity, ApprovalCommandReceipt, ApprovalNodeDefinition, ApprovalProcessDefinition,
    CanonicalCommandPayload, CommandPayloadField, IdempotencyKey,
};
use bpm::{ParticipantId, Timestamp};
use chrono::Utc;
use database::repository::bpm::{CasWriteOutcome, APPROVAL_COMMAND_RECEIPT_IDEMPOTENCY_INDEX};
use database::{AccessControlExt, BpmExt, Executor, NoTransaction, Transactional};
use entities::document_registry::DocumentType;
use entities::{AccountCore, Permission};
use id_generator::next_id;
use mongodb::Database;
use sha2::{Digest, Sha256};

use crate::audit::AuditActor;
use crate::errors::{Error, ErrorCode, Result};
use crate::iam::{subject, SharedRbacService};

use super::definition_dto::{
    ApprovalRequirementView, CreateDefinitionDraftRequest, DefinitionAllowedAction, DefinitionCatalogItem,
    DefinitionConfigurationStatus, DefinitionDetailView, DefinitionNodeRequest, DefinitionNodeView,
    DefinitionVersionItem, DraftSource, PublishDefinitionRequest, ReplaceDefinitionNodesRequest,
    RetireDefinitionRequest,
};
use super::policy::{
    ensure_actions_registered, policy_of, require_process_required, validate_required_purposes,
    ApprovalRequirement, ApproverEligibilityPolicy, DocumentApprovalPolicy, ProcessRequiredApprovalPolicy,
    SeparationOfDutiesPolicy, ALL_DOCUMENT_TYPES, STATIC_APPROVE_PERMISSION,
};
use super::process_kind::{document_type_of, process_kind_of};

use super::scope::definition_management_visibility_with_executor;
pub use super::scope::{definition_management_visibility, DefinitionManagementVisibility};

const CREATE_DRAFT_COMMAND_DOMAIN: &str = "approval.definition.create-draft";
const REPLACE_NODES_COMMAND_DOMAIN: &str = "approval.definition.replace-nodes";
const PUBLISH_DEFINITION_COMMAND_DOMAIN: &str = "approval.definition.publish";
const RETIRE_DEFINITION_COMMAND_DOMAIN: &str = "approval.definition.retire";
const DEFINITION_RESULT_REF_PREFIX: &str = "definition-v1:";
const DEFINITION_COMMAND_RECOVERY_ATTEMPTS: usize = 8;

/// 当前 v3 命令身份与唯一允许查询的旧格式候选。
#[derive(Debug, Clone, PartialEq, Eq)]
struct PreparedDefinitionIdentity {
    current: ApprovalCommandIdentity,
    legacy: Option<LegacyDefinitionIdentity>,
}

/// 历史定义命令的精确 scope/digest 配对与结果证明方式。
#[derive(Debug, Clone, PartialEq, Eq)]
struct LegacyDefinitionIdentity {
    command_kind: ApprovalCommandKind,
    scope_id: String,
    payload_digest: String,
    proof: LegacyDefinitionProof,
}

/// 旧收据只有能由结果中的不可变事实证明完整载荷时才可回放。
#[derive(Debug, Clone, PartialEq, Eq)]
enum LegacyDefinitionProof {
    /// 创建来源不是不可变结果事实，必须失败关闭。
    Unprovable,
    /// 发布结果可由定义身份、发布人、状态和锁版本证明。
    Published {
        definition_id: String,
        expected_lock: u64,
        actor_id: String,
    },
    /// 退役结果可由定义身份、退役人、状态和锁版本证明。
    Retired {
        definition_id: String,
        expected_lock: u64,
        actor_id: String,
    },
}

/// v3 收据中版本化、可校验的定义结果引用。
#[derive(Debug, Clone, PartialEq, Eq)]
struct DefinitionCommandResultRef {
    definition_id: String,
    definition_lock_version: u64,
}

/// 回放时必须证明结果仍属于命令原始资源。
#[derive(Debug, Clone, PartialEq, Eq)]
enum DefinitionResultExpectation {
    ProcessKind(bpm::ProcessKind),
    DefinitionId(String),
}

/// 审批流程定义管理服务。
pub struct ApprovalDefinitionService {
    db: Database,
    rbac: SharedRbacService,
}

impl ApprovalDefinitionService {
    /// 创建定义管理服务。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    /// * `rbac` - 共享 RBAC 服务
    ///
    /// # 返回
    /// 返回尚未接线 HTTP 的应用端口。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 返回定义管理使用的数据库。
    ///
    /// # 返回
    /// 返回 MongoDB 句柄。
    pub(crate) fn db(&self) -> &Database {
        &self.db
    }

    /// 返回固定 20 行非敏感目录。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `visibility` - 类型级可见范围
    ///
    /// # 错误
    /// 政策或仓储读取失败时返回错误。
    pub async fn definition_catalog(
        &self,
        actor: &AuditActor,
        visibility: &DefinitionManagementVisibility,
    ) -> Result<Vec<DefinitionCatalogItem>> {
        let visibility = enforce_visibility(&self.db, &self.rbac, actor, visibility).await?;
        let mut items = Vec::with_capacity(ALL_DOCUMENT_TYPES.len());
        for document_type in ALL_DOCUMENT_TYPES {
            items.push(self.catalog_item(document_type, &visibility).await?);
        }
        Ok(items)
    }

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
        let identity = create_draft_identity(
            key,
            request.document_type,
            &name,
            request.draft_source,
            actor.id(),
        )?;
        self.ensure_definition_admin(actor, &policy).await?;
        if let Some(view) = self
            .replay_if_receipt(
                &identity.current,
                DefinitionResultExpectation::ProcessKind(policy.process_kind),
            )
            .await?
        {
            return Ok(view);
        }
        if let Some(view) = self.replay_legacy_if_receipt(&identity).await? {
            return Ok(view);
        }
        self.commit_create_draft(&policy, &name, request, actor, identity)
            .await
    }

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

    /// 发布草稿为当前唯一已发布版本。
    ///
    /// # 参数
    /// * `request` - 发布请求
    /// * `actor` - 已认证操作人
    ///
    /// # 错误
    /// 图、用途、账号或权限校验失败时零写入返回错误。
    pub async fn publish_definition(
        &self,
        mut request: PublishDefinitionRequest,
        actor: &AuditActor,
    ) -> Result<DefinitionDetailView> {
        let key = parse_idempotency_key(&request.idempotency_key)?;
        request.idempotency_key = key.as_str().to_string();
        let identity = lock_command_identity(
            key,
            ApprovalCommandKind::PublishDefinition,
            PUBLISH_DEFINITION_COMMAND_DOMAIN,
            &request.definition_id,
            request.expected_definition_lock_version,
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
        if let Some(view) = self.replay_legacy_if_receipt(&identity).await? {
            return Ok(view);
        }
        self.commit_publish(graph, policy, request, actor, identity).await
    }

    /// 退役当前已发布定义。
    ///
    /// # 参数
    /// * `request` - 退役请求
    /// * `actor` - 已认证操作人
    ///
    /// # 错误
    /// 目标不是当前发布版本或锁冲突时返回错误。
    pub async fn retire_definition(
        &self,
        mut request: RetireDefinitionRequest,
        actor: &AuditActor,
    ) -> Result<DefinitionDetailView> {
        let key = parse_idempotency_key(&request.idempotency_key)?;
        request.idempotency_key = key.as_str().to_string();
        let identity = lock_command_identity(
            key,
            ApprovalCommandKind::RetireDefinition,
            RETIRE_DEFINITION_COMMAND_DOMAIN,
            &request.definition_id,
            request.expected_definition_lock_version,
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
        if let Some(view) = self.replay_legacy_if_receipt(&identity).await? {
            return Ok(view);
        }
        self.commit_retire(graph, policy, request, actor, identity).await
    }

    /// 列出某单据类型的定义版本。
    ///
    /// # 参数
    /// * `document_type` - 固定单据类型
    /// * `actor` - 已认证操作人
    /// * `visibility` - 类型级可见范围
    ///
    /// # 错误
    /// 无读取权或类型无需审批时返回错误。
    pub async fn definition_versions(
        &self,
        document_type: DocumentType,
        actor: &AuditActor,
        visibility: &DefinitionManagementVisibility,
    ) -> Result<Vec<DefinitionVersionItem>> {
        let visibility = enforce_visibility(&self.db, &self.rbac, actor, visibility).await?;
        require_process_required(document_type)?;
        ensure_can_read_detail(&visibility, document_type)?;
        let versions = self
            .db
            .bpm_workflow()
            .list_definition_versions(process_kind_of(document_type), &mut NoTransaction)
            .await?;
        Ok(versions.iter().map(version_item).collect())
    }

    /// 返回定义详情。
    ///
    /// # 参数
    /// * `definition_id` - 定义主键
    /// * `actor` - 已认证操作人
    /// * `visibility` - 类型级可见范围
    ///
    /// # 错误
    /// 不存在或无权读取时返回不泄露存在性的错误。
    pub async fn definition_detail(
        &self,
        definition_id: &str,
        actor: &AuditActor,
        visibility: &DefinitionManagementVisibility,
    ) -> Result<DefinitionDetailView> {
        let visibility = enforce_visibility(&self.db, &self.rbac, actor, visibility).await?;
        let graph = self
            .db
            .bpm_workflow()
            .load_definition_graph(
                &ApprovalProcessDefinitionId::new(definition_id.to_string()),
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(definition_not_found)?;
        let document_type = document_type_of(graph.definition.process_kind);
        ensure_can_read_detail(&visibility, document_type)?;
        Ok(detail_view(&graph))
    }

    /// 校验操作人具备该类型定义管理权。
    ///
    /// # 错误
    /// 缺少类型级权限时返回禁止。
    pub(crate) async fn ensure_definition_admin(
        &self,
        actor: &AuditActor,
        policy: &ProcessRequiredApprovalPolicy,
    ) -> Result<()> {
        ensure_definition_admin_permission(&self.db, &self.rbac, actor, policy, &mut NoTransaction).await
    }

    /// 组装单行目录。
    ///
    /// # 错误
    /// 政策或仓储读取失败时返回错误。
    async fn catalog_item(
        &self,
        document_type: DocumentType,
        visibility: &DefinitionManagementVisibility,
    ) -> Result<DefinitionCatalogItem> {
        let policy = policy_of(document_type)?;
        let (published_version, draft_version) = self.catalog_versions(&policy).await?;
        Ok(DefinitionCatalogItem {
            document_type,
            document_type_label: document_type.label().to_string(),
            approval_requirement: requirement_view(policy.requirement()),
            published_version,
            draft_version,
            configuration_status: configuration_status(
                policy.requirement(),
                published_version,
                draft_version,
            ),
            allowed_actions: allowed_actions(
                policy.requirement(),
                visibility.can_define(document_type),
                published_version,
                draft_version,
            ),
        })
    }

    /// 读取目录所需的发布与草稿版本。
    ///
    /// # 错误
    /// 仓储读取失败时返回错误。
    async fn catalog_versions(&self, policy: &DocumentApprovalPolicy) -> Result<(Option<u32>, Option<u32>)> {
        if !matches!(policy, DocumentApprovalPolicy::ProcessRequired(_)) {
            return Ok((None, None));
        }
        let published = self
            .db
            .bpm_workflow()
            .find_published_by_process_kind(policy.process_kind(), &mut NoTransaction)
            .await?;
        let draft = self
            .db
            .bpm_workflow()
            .find_active_draft(policy.process_kind(), &mut NoTransaction)
            .await?;
        Ok((
            published.map(|item| item.definition_version),
            draft.map(|item| item.definition_version),
        ))
    }

    /// 读取定义图，缺失时失败关闭。
    ///
    /// # 错误
    /// 定义不存在时返回未找到。
    async fn require_graph(&self, definition_id: &str) -> Result<DefinitionGraph> {
        self.db
            .bpm_workflow()
            .load_definition_graph(
                &ApprovalProcessDefinitionId::new(definition_id.to_string()),
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(definition_not_found)
    }

    /// 同键同载荷回读详情。
    ///
    /// # 错误
    /// 异载荷、结果引用漂移或结果资源不匹配时返回稳定冲突。
    async fn replay_if_receipt(
        &self,
        identity: &ApprovalCommandIdentity,
        expectation: DefinitionResultExpectation,
    ) -> Result<Option<DefinitionDetailView>> {
        replay_current_receipt(&self.db, identity, &expectation, &mut NoTransaction).await
    }

    /// 只查询命令声明的精确旧 scope/digest 候选。
    ///
    /// # 错误
    /// 旧结果不能证明完整请求载荷时固定返回幂等载荷冲突。
    async fn replay_legacy_if_receipt(
        &self,
        identity: &PreparedDefinitionIdentity,
    ) -> Result<Option<DefinitionDetailView>> {
        let Some(legacy) = identity.legacy.as_ref() else {
            return Ok(None);
        };
        replay_legacy_receipt(
            &self.db,
            identity.current.idempotency_key(),
            legacy,
            &mut NoTransaction,
        )
        .await
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
        let rbac = self.rbac.clone();
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
                        transaction_policy,
                        request,
                        &transaction_actor,
                        &transaction_identity,
                        session,
                    )
                    .await
                })
            })
            .await;
        self.recover_definition_command(outcome, &policy, actor, identity, expectation)
            .await
    }

    /// 在唯一事务中发布草稿并退役旧版本。
    ///
    /// # 错误
    /// 任一校验失败时整体回滚。
    async fn commit_publish(
        &self,
        graph: DefinitionGraph,
        policy: ProcessRequiredApprovalPolicy,
        request: PublishDefinitionRequest,
        actor: &AuditActor,
        identity: PreparedDefinitionIdentity,
    ) -> Result<DefinitionDetailView> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let transaction_actor = actor.clone();
        let transaction_policy = policy.clone();
        let transaction_identity = identity.clone();
        let expectation = DefinitionResultExpectation::DefinitionId(request.definition_id.clone());
        let client = db.client().clone();
        let outcome = client.with_transaction(move |session| {
            Box::pin(async move {
                publish_tx(
                    &db,
                    &rbac,
                    graph,
                    PublishTxInput {
                        policy: transaction_policy,
                        request,
                        actor: &transaction_actor,
                        identity: &transaction_identity,
                    },
                    session,
                )
                .await
            })
        });
        self.recover_definition_command(outcome.await, &policy, actor, identity, expectation)
            .await
    }

    /// 在唯一事务中退役当前发布版本。
    ///
    /// # 错误
    /// 目标不是当前发布版本或写入失败时返回错误。
    async fn commit_retire(
        &self,
        graph: DefinitionGraph,
        policy: ProcessRequiredApprovalPolicy,
        request: RetireDefinitionRequest,
        actor: &AuditActor,
        identity: PreparedDefinitionIdentity,
    ) -> Result<DefinitionDetailView> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let transaction_actor = actor.clone();
        let transaction_policy = policy.clone();
        let transaction_identity = identity.clone();
        let expectation = DefinitionResultExpectation::DefinitionId(request.definition_id.clone());
        let client = db.client().clone();
        let outcome = client.with_transaction(move |session| {
            Box::pin(async move {
                retire_tx(
                    &db,
                    &rbac,
                    graph,
                    transaction_policy,
                    request,
                    &transaction_actor,
                    &transaction_identity,
                    session,
                )
                .await
            })
        });
        self.recover_definition_command(outcome.await, &policy, actor, identity, expectation)
            .await
    }

    /// 收据竞争、瞬态事务或提交结果未知后，在新会话有限回读胜者。
    ///
    /// # 错误
    /// 找不到胜者时返回原错误；回放前当前定义管理权失效时返回禁止。
    async fn recover_definition_command(
        &self,
        outcome: Result<DefinitionDetailView>,
        policy: &ProcessRequiredApprovalPolicy,
        actor: &AuditActor,
        identity: PreparedDefinitionIdentity,
        expectation: DefinitionResultExpectation,
    ) -> Result<DefinitionDetailView> {
        let Err(error) = outcome else {
            return outcome;
        };
        if !definition_command_may_have_committed(&error) {
            return Err(error);
        }
        for attempt in 0..DEFINITION_COMMAND_RECOVERY_ATTEMPTS {
            let db = self.db.clone();
            let rbac = self.rbac.clone();
            let actor = actor.clone();
            let policy = policy.clone();
            let identity = identity.clone();
            let expectation = expectation.clone();
            let recovered = self
                .db
                .client()
                .with_transaction(move |session| {
                    Box::pin(async move {
                        ensure_definition_admin_permission(&db, &rbac, &actor, &policy, session).await?;
                        replay_prepared_definition_receipt(&db, &identity, &expectation, session).await
                    })
                })
                .await;
            match recovered {
                Ok(Some(view)) => return Ok(view),
                Ok(None) => {}
                Err(recovery_error) if definition_command_may_have_committed(&recovery_error) => {}
                Err(recovery_error) => return Err(recovery_error),
            }
            if attempt + 1 < DEFINITION_COMMAND_RECOVERY_ATTEMPTS {
                tokio::time::sleep(definition_recovery_delay(attempt)).await;
            }
        }
        Err(error)
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
    rbac: &SharedRbacService,
    input: CreateDraftTxInput<'_>,
    session: &mut mongodb::ClientSession,
) -> Result<DefinitionDetailView> {
    let CreateDraftTxInput {
        policy,
        name,
        request,
        actor,
        identity,
    } = input;
    ensure_definition_admin_permission(db, rbac, actor, policy, session).await?;
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
    let CreateDraftWriteStep::PersistNewDraftAndReceipt = decide_create_draft_write(
        db.bpm_workflow()
            .find_active_draft(policy.process_kind, session)
            .await?,
    )?;
    let graph = build_new_draft(db, rbac, policy, name, request.draft_source, actor, session).await?;
    let result_ref = DefinitionCommandResultRef::from_graph(&graph).encode();
    write_receipt(db, &identity.current, &result_ref, session).await?;
    persist_new_draft(db, &graph, session).await?;
    write_definition_audit(
        db,
        actor,
        "approval_definition.create_draft",
        &graph,
        None,
        None,
        session,
    )
    .await?;
    Ok(detail_view(&graph))
}

/// 事务内替换草稿节点。
#[allow(clippy::too_many_arguments)]
async fn replace_nodes_tx(
    db: &Database,
    rbac: &SharedRbacService,
    graph: DefinitionGraph,
    policy: ProcessRequiredApprovalPolicy,
    request: ReplaceDefinitionNodesRequest,
    actor: &AuditActor,
    identity: &PreparedDefinitionIdentity,
    session: &mut mongodb::ClientSession,
) -> Result<DefinitionDetailView> {
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

/// 发布草稿事务的政策、请求、命令身份与操作人。
///
/// # 用途
/// 将发布命令上下文字段打包。
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
/// 图、用途、账号或动作任一失败都不得进入发布写库。
struct PublishTxInput<'a> {
    /// 单据审批政策。
    policy: ProcessRequiredApprovalPolicy,
    /// 发布请求。
    request: PublishDefinitionRequest,
    /// 审计操作人。
    actor: &'a AuditActor,
    /// 已规范化的当前命令身份及精确旧格式候选。
    identity: &'a PreparedDefinitionIdentity,
}

/// 事务内发布草稿。
///
/// # 用途
/// 回放收据或刷新快照并发布当前草稿。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `graph` - 当前草稿图
/// * `input` - 政策、请求、摘要与操作人
/// * `session` - 事务会话
///
/// # 返回
/// 返回发布后的定义详情。
///
/// # 错误
/// 陈旧锁、图校验失败或写入失败时返回错误。
///
/// # 关键业务约束
/// 任一校验失败必须整体回滚。
async fn publish_tx(
    db: &Database,
    rbac: &SharedRbacService,
    graph: DefinitionGraph,
    input: PublishTxInput<'_>,
    session: &mut mongodb::ClientSession,
) -> Result<DefinitionDetailView> {
    let PublishTxInput {
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
    let mut current = reload_draft_for_cas(
        db,
        &graph.definition,
        request.expected_definition_lock_version,
        session,
    )
    .await?;
    let PublishWriteStep::RefreshSnapshotsAndRetirePrevious = decide_publish_write(
        current.validate_linear().map_err(map_model_error),
        validate_required_purposes(&policy, &current.purpose_refs()),
        validate_assignees(db, rbac, &policy, &current.assignee_ids(), session).await,
        ensure_actions_registered(&policy),
    )?;
    current = prepare_publish_graph(db, current, session).await?;
    let previous = db
        .bpm_workflow()
        .find_published_by_process_kind(policy.process_kind, session)
        .await?;
    let previous_lock = previous.as_ref().map(|item| item.definition_lock_version());
    let (published, previous) = current
        .definition
        .clone()
        .publish_replacing(previous, participant(actor)?, now()?)
        .map_err(map_model_error)?;
    let mut result = DefinitionGraph {
        definition: published,
        nodes: current.nodes.clone(),
        transitions: current.transitions.clone(),
    };
    let result_ref = DefinitionCommandResultRef::from_graph(&result).encode();
    write_receipt(db, &identity.current, &result_ref, session).await?;
    let refreshed = replace_graph(db, current, request.expected_definition_lock_version, session).await?;
    let outcome = db
        .bpm_workflow()
        .publish_and_retire_previous(
            &result.definition,
            previous.as_ref(),
            refreshed.definition.definition_lock_version(),
            previous_lock,
            session,
        )
        .await?;
    result.definition = applied_definition(outcome)?;
    write_definition_audit(
        db,
        actor,
        "approval_definition.publish",
        &result,
        Some(request.expected_definition_lock_version),
        None,
        session,
    )
    .await?;
    Ok(detail_view(&result))
}

/// 事务内退役当前发布版本。
#[allow(clippy::too_many_arguments)]
async fn retire_tx(
    db: &Database,
    rbac: &SharedRbacService,
    graph: DefinitionGraph,
    policy: ProcessRequiredApprovalPolicy,
    request: RetireDefinitionRequest,
    actor: &AuditActor,
    identity: &PreparedDefinitionIdentity,
    session: &mut mongodb::ClientSession,
) -> Result<DefinitionDetailView> {
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
    let published = db
        .bpm_workflow()
        .find_published_by_process_kind(policy.process_kind, session)
        .await?;
    let RetireWriteStep::RetireCurrentPublished = decide_retire_write(
        published.as_ref(),
        &graph.definition.base.id,
        request.expected_definition_lock_version,
    )?;
    let Some(mut retired) = published else {
        return Err(Error::BusinessLogicError(
            "当前没有可退役的已发布定义".to_string(),
        ));
    };
    let expected = retired.definition_lock_version();
    retired
        .retire(participant(actor)?, now()?)
        .map_err(map_model_error)?;
    let graph = DefinitionGraph {
        definition: retired.clone(),
        nodes: graph.nodes,
        transitions: graph.transitions,
    };
    let result_ref = DefinitionCommandResultRef::from_graph(&graph).encode();
    write_receipt(db, &identity.current, &result_ref, session).await?;
    retired.base.version = expected;
    db.approval_process_definitions()
        .update(&mut retired, session)
        .await?;
    let graph = DefinitionGraph {
        definition: retired,
        nodes: graph.nodes,
        transitions: graph.transitions,
    };
    write_definition_audit(
        db,
        actor,
        "approval_definition.retire",
        &graph,
        Some(request.expected_definition_lock_version),
        None,
        session,
    )
    .await?;
    Ok(detail_view(&graph))
}

/// 构造空草稿或从当前发布复制。
async fn build_new_draft(
    db: &Database,
    rbac: &SharedRbacService,
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
        }
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
    Ok(DefinitionGraph {
        definition,
        nodes: Vec::new(),
        transitions: Vec::new(),
    })
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
    rbac: &SharedRbacService,
    policy: &ProcessRequiredApprovalPolicy,
    name: &str,
    version: u32,
    actor: &AuditActor,
    session: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let published = require_current_published(
        db.bpm_workflow()
            .find_published_by_process_kind(policy.process_kind, session)
            .await?,
    )?;
    let source = db
        .bpm_workflow()
        .load_definition_graph(
            &ApprovalProcessDefinitionId::new(published.base.id.clone()),
            session,
        )
        .await?
        .ok_or_else(definition_not_found)?;
    let definition_id = ApprovalProcessDefinitionId::new(next_id());
    let at = now()?;
    let identities = next_copied_node_identities(source.nodes.len());
    let nodes = copy_nodes_for_definition(&source.nodes, definition_id.clone(), &identities, at)
        .map_err(map_model_error)?;
    validate_assignees(db, rbac, policy, &assignee_ids(&nodes), session).await?;
    let transition_ids = next_transition_ids(nodes.len());
    DefinitionGraph::new_populated_draft(
        definition_id,
        policy.process_kind,
        version,
        name,
        participant(actor)?,
        nodes,
        transition_ids,
        at,
    )
    .map_err(map_model_error)
}

/// 持久化新建草稿及其图。
async fn persist_new_draft(db: &Database, graph: &DefinitionGraph, session: &mut dyn Executor) -> Result<()> {
    db.approval_process_definitions()
        .create(&graph.definition, session)
        .await?;
    for node in &graph.nodes {
        db.approval_node_definitions().create(node, session).await?;
    }
    for transition in &graph.transitions {
        db.approval_transition_definitions()
            .create(transition, session)
            .await?;
    }
    Ok(())
}

/// 重新加载草稿并核对锁版本。
async fn reload_draft_for_cas(
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

/// 刷新快照并构造待写草稿图，供发布事务在 receipt-first 后持久化。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `graph` - 已完成发布前校验的草稿图
/// * `session` - 调用方事务执行器
///
/// # 返回
/// 返回尚未写库的完整草稿图。
///
/// # 错误
/// 账号快照缺失或 BPM 重建失败时返回错误。
///
/// # 关键业务约束
/// 发布前必须以当前账号显示名刷新全部节点快照；本函数不得产生物理写入。
async fn prepare_publish_graph(
    db: &Database,
    graph: DefinitionGraph,
    session: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let assignee_ids = graph.assignee_ids();
    let snapshots = load_assignee_snapshots(db, &assignee_ids, session).await?;
    let nodes = apply_snapshots(graph.nodes, &snapshots)?;
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
async fn replace_graph(
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
            let existing_node_id = request
                .node_id
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(ApprovalNodeDefinitionId::new);
            Ok(NodeReplacementDraft {
                existing_node_id,
                new_node_id: ApprovalNodeDefinitionId::new(next_id()),
                new_node_key: next_id(),
                node_name: request.node_name.clone(),
                display_order: request.display_order,
                assignee_participant_id: ParticipantId::new(request.assignee_user_id.trim())
                    .map_err(map_bpm_error)?,
            })
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
async fn validate_assignees(
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
async fn load_assignee_snapshots(
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
fn validate_static_separation(policy: SeparationOfDutiesPolicy, _user_ids: &[String]) -> Result<()> {
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
fn apply_snapshots(
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
fn rebuild_draft_graph(
    definition: &ApprovalProcessDefinition,
    nodes: Vec<ApprovalNodeDefinition>,
) -> Result<DefinitionGraph> {
    let transition_ids = next_transition_ids(nodes.len());
    DefinitionGraph::rebuild_draft(definition, nodes, transition_ids, now()?).map_err(map_model_error)
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
fn next_transition_ids(node_count: usize) -> Vec<ApprovalTransitionDefinitionId> {
    (0..node_count.saturating_mul(2))
        .map(|_| ApprovalTransitionDefinitionId::new(next_id()))
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
    let current = db
        .bpm_workflow()
        .latest_definition_version(process_kind, session)
        .await?
        .unwrap_or(0);
    ApprovalProcessDefinition::next_version_after(current).map_err(map_model_error)
}

/// 事务内若已有同载荷 v3 收据则回读仍未漂移的结果版本。
async fn replay_current_receipt(
    db: &Database,
    identity: &ApprovalCommandIdentity,
    expectation: &DefinitionResultExpectation,
    session: &mut dyn Executor,
) -> Result<Option<DefinitionDetailView>> {
    let Some(receipt) = db
        .bpm_workflow()
        .find_command_receipt(
            identity.command_kind(),
            identity.scope().as_str(),
            identity.idempotency_key(),
            session,
        )
        .await?
    else {
        return Ok(None);
    };
    receipt.reconcile_identity(identity).map_err(map_model_error)?;
    let result_ref = DefinitionCommandResultRef::parse(&receipt.result_ref)?;
    let graph = db
        .bpm_workflow()
        .load_definition_graph(
            &ApprovalProcessDefinitionId::new(result_ref.definition_id.clone()),
            session,
        )
        .await?
        .ok_or_else(idempotency_payload_conflict)?;
    result_ref.ensure_matches(&graph, expectation)?;
    Ok(Some(detail_view(&graph)))
}

/// 事务内先读取 v3 收据；仅在 v3 不存在时读取命令声明的精确旧格式候选。
///
/// 当前格式一旦存在即由 `reconcile_identity` 决定回放或冲突，不得降级至旧格式。
async fn replay_prepared_definition_receipt(
    db: &Database,
    identity: &PreparedDefinitionIdentity,
    expectation: &DefinitionResultExpectation,
    session: &mut dyn Executor,
) -> Result<Option<DefinitionDetailView>> {
    if let Some(view) = replay_current_receipt(db, &identity.current, expectation, session).await? {
        return Ok(Some(view));
    }
    let Some(legacy) = identity.legacy.as_ref() else {
        return Ok(None);
    };
    replay_legacy_receipt(db, identity.current.idempotency_key(), legacy, session).await
}

/// 精确读取一个旧 scope/digest 候选，并以不可变结果事实证明完整载荷。
async fn replay_legacy_receipt(
    db: &Database,
    key: &IdempotencyKey,
    legacy: &LegacyDefinitionIdentity,
    session: &mut dyn Executor,
) -> Result<Option<DefinitionDetailView>> {
    let Some(receipt) = db
        .bpm_workflow()
        .find_command_receipt(legacy.command_kind, &legacy.scope_id, key, session)
        .await?
    else {
        return Ok(None);
    };
    receipt
        .reconcile(&legacy.payload_digest)
        .map_err(map_model_error)?;
    let graph = db
        .bpm_workflow()
        .load_definition_graph(
            &ApprovalProcessDefinitionId::new(receipt.result_ref.clone()),
            session,
        )
        .await?
        .ok_or_else(idempotency_payload_conflict)?;
    ensure_legacy_result(&receipt, &graph, &legacy.proof)?;
    Ok(Some(detail_view(&graph)))
}

/// 写入命令收据。
async fn write_receipt(
    db: &Database,
    identity: &ApprovalCommandIdentity,
    result_ref: &str,
    session: &mut dyn Executor,
) -> Result<()> {
    let receipt = ApprovalCommandReceipt::new(
        ApprovalCommandReceiptId::new(next_id()),
        identity,
        result_ref,
        now()?,
    )
    .map_err(map_model_error)?;
    db.bpm_workflow()
        .insert_command_receipt(&receipt, session)
        .await
        .map_err(mark_receipt_duplicate)
}

/// 写入定义变更审计。
async fn write_definition_audit(
    db: &Database,
    actor: &AuditActor,
    action: &str,
    graph: &DefinitionGraph,
    expected_lock: Option<u64>,
    extra: Option<&str>,
    session: &mut dyn Executor,
) -> Result<()> {
    let document_type = document_type_of(graph.definition.process_kind);
    let message = format!(
        "document_type={} version={} expected_lock={:?} actual_lock={} nodes={} extra={:?}",
        document_type.as_str(),
        graph.definition.definition_version,
        expected_lock,
        graph.definition.definition_lock_version(),
        node_summary(&graph.nodes),
        extra
    );
    let audit = actor.clone().resource_log_with_message(
        action,
        "approval_process_definition",
        graph.definition.base.id.clone(),
        Some(message),
    )?;
    db.audit_logs().create(&audit, session).await?;
    Ok(())
}

/// 取出 CAS 成功后的定义。
fn applied_definition(
    outcome: CasWriteOutcome<ApprovalProcessDefinition>,
) -> Result<ApprovalProcessDefinition> {
    match outcome {
        CasWriteOutcome::Applied(definition) => Ok(definition),
        CasWriteOutcome::VersionConflict(_) => Err(stale_lock_error()),
        CasWriteOutcome::StatusChanged(_) => {
            Err(Error::from_approval_code(ErrorCode::ApprovalDefinitionNotDraft))
        }
        CasWriteOutcome::NotFound => Err(definition_not_found()),
    }
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

/// 校验定义锁版本并映射陈旧锁错误。
///
/// # 参数
/// * `definition` - 当前定义快照
/// * `expected` - 调用方期望的锁版本
///
/// # 返回
/// 锁版本一致时返回 `Ok(())`。
///
/// # 错误
/// 锁版本不一致时返回“未写入任何节点”的稳定冲突错误。
///
/// # 关键业务约束
/// 不得使用业务版本替代 `definition_lock_version`。
fn ensure_lock(definition: &ApprovalProcessDefinition, expected: u64) -> Result<()> {
    definition
        .ensure_lock_version(expected)
        .map_err(|_| stale_lock_error())
}

/// 由定义的流程种类读取必须审批政策。
fn policy_for_definition(definition: &ApprovalProcessDefinition) -> Result<ProcessRequiredApprovalPolicy> {
    require_process_required(document_type_of(definition.process_kind))
}

/// 以当前 RBAC 重验类型级范围，禁止调用方扩大权限。
async fn enforce_visibility(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    visibility: &DefinitionManagementVisibility,
) -> Result<DefinitionManagementVisibility> {
    Ok(definition_management_visibility(db, rbac, actor)
        .await?
        .intersect(visibility))
}

/// 详情读取权。
fn ensure_can_read_detail(
    visibility: &DefinitionManagementVisibility,
    document_type: DocumentType,
) -> Result<()> {
    if visibility.can_read_detail(document_type) {
        return Ok(());
    }
    Err(definition_not_found())
}

/// 节点摘要，供审计使用。
fn node_summary(nodes: &[ApprovalNodeDefinition]) -> String {
    nodes
        .iter()
        .map(|node| format!("{}:{}", node.display_order, node.node_key))
        .collect::<Vec<_>>()
        .join(",")
}

/// 在任何 Repository 读取前把外部幂等键转换为规范值对象。
fn parse_idempotency_key(raw: &str) -> Result<IdempotencyKey> {
    IdempotencyKey::parse(raw.to_string()).map_err(|error| match error {
        ModelError::InvalidField(message) => Error::ValidationError(message.to_string()),
        other => Error::Internal(format!("审批命令幂等键构造失败: {other}")),
    })
}

/// 使用 BPM 实体的名称规则准备完整有序节点载荷。
fn prepare_definition_nodes(nodes: Vec<DefinitionNodeRequest>) -> Result<Vec<DefinitionNodeRequest>> {
    let mut prepared = Vec::with_capacity(nodes.len());
    for mut node in nodes {
        node.node_id = node
            .node_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        node.node_name = ApprovalNodeDefinition::normalize_name(node.node_name).map_err(map_model_error)?;
        let assignee = node.assignee_user_id.trim().to_string();
        ParticipantId::new(assignee.clone()).map_err(map_bpm_error)?;
        node.assignee_user_id = assignee;
        prepared.push(node);
    }
    prepared.sort_by_key(|node| node.display_order);
    Ok(prepared)
}

/// 创建草稿的 v3 身份与唯一旧格式候选。
fn create_draft_identity(
    key: IdempotencyKey,
    document_type: DocumentType,
    name: &str,
    draft_source: DraftSource,
    actor_id: &str,
) -> Result<PreparedDefinitionIdentity> {
    let process_kind = process_kind_of(document_type);
    let current = ApprovalCommandIdentity::new(
        ApprovalCommandKind::DefinitionWrite,
        CREATE_DRAFT_COMMAND_DOMAIN,
        key,
        CanonicalCommandPayload::new().field(CommandPayloadField::Text(process_kind.as_str())),
        CanonicalCommandPayload::new()
            .field(CommandPayloadField::Text(document_type.as_str()))
            .field(CommandPayloadField::Text(name))
            .field(CommandPayloadField::Text(draft_source.as_str()))
            .field(CommandPayloadField::Text(actor_id)),
    )
    .map_err(map_identity_error)?;
    Ok(PreparedDefinitionIdentity {
        current,
        legacy: Some(LegacyDefinitionIdentity {
            command_kind: ApprovalCommandKind::DefinitionWrite,
            scope_id: process_kind.as_str().to_string(),
            payload_digest: legacy_payload_digest(&[
                document_type.as_str(),
                name,
                draft_source.as_str(),
                actor_id,
            ]),
            proof: LegacyDefinitionProof::Unprovable,
        }),
    })
}

/// 整组节点替换的 v3 身份；Create/Replace 即使资源文本相同也使用不同 domain。
fn replace_nodes_identity(
    key: IdempotencyKey,
    definition_id: &str,
    expected_lock: u64,
    nodes: &[DefinitionNodeRequest],
    actor_id: &str,
) -> Result<PreparedDefinitionIdentity> {
    let node_fields = nodes
        .iter()
        .map(|node| {
            CommandPayloadField::Sequence(vec![
                CommandPayloadField::OptionalText(node.node_id.as_deref()),
                CommandPayloadField::Text(&node.node_name),
                CommandPayloadField::U32(node.display_order),
                CommandPayloadField::Text(&node.assignee_user_id),
            ])
        })
        .collect();
    let current = ApprovalCommandIdentity::new(
        ApprovalCommandKind::DefinitionWrite,
        REPLACE_NODES_COMMAND_DOMAIN,
        key,
        CanonicalCommandPayload::new().field(CommandPayloadField::Text(definition_id)),
        CanonicalCommandPayload::new()
            .field(CommandPayloadField::Text(definition_id))
            .field(CommandPayloadField::U64(expected_lock))
            .field(CommandPayloadField::Sequence(node_fields))
            .field(CommandPayloadField::Text(actor_id)),
    )
    .map_err(map_identity_error)?;
    Ok(PreparedDefinitionIdentity {
        current,
        legacy: None,
    })
}

/// 发布或退役的 v3 身份与可证明旧格式候选。
fn lock_command_identity(
    key: IdempotencyKey,
    command_kind: ApprovalCommandKind,
    domain: &'static str,
    definition_id: &str,
    expected_lock: u64,
    actor_id: &str,
) -> Result<PreparedDefinitionIdentity> {
    let current = ApprovalCommandIdentity::new(
        command_kind,
        domain,
        key,
        CanonicalCommandPayload::new().field(CommandPayloadField::Text(definition_id)),
        CanonicalCommandPayload::new()
            .field(CommandPayloadField::Text(definition_id))
            .field(CommandPayloadField::U64(expected_lock))
            .field(CommandPayloadField::Text(actor_id)),
    )
    .map_err(map_identity_error)?;
    let proof = match command_kind {
        ApprovalCommandKind::PublishDefinition => LegacyDefinitionProof::Published {
            definition_id: definition_id.to_string(),
            expected_lock,
            actor_id: actor_id.to_string(),
        },
        ApprovalCommandKind::RetireDefinition => LegacyDefinitionProof::Retired {
            definition_id: definition_id.to_string(),
            expected_lock,
            actor_id: actor_id.to_string(),
        },
        _ => {
            return Err(Error::Internal("定义锁命令种类不属于发布或退役".to_string()));
        }
    };
    Ok(PreparedDefinitionIdentity {
        current,
        legacy: Some(LegacyDefinitionIdentity {
            command_kind,
            scope_id: definition_id.to_string(),
            payload_digest: legacy_payload_digest(&[&expected_lock.to_string(), actor_id]),
            proof,
        }),
    })
}

/// 只用于精确读取旧收据候选的历史 U+001F 摘要。
fn legacy_payload_digest(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            hasher.update([0x1f]);
        }
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())
}

impl DefinitionCommandResultRef {
    /// 从尚未或已经持久化的图冻结定义身份与结果锁版本。
    fn from_graph(graph: &DefinitionGraph) -> Self {
        Self {
            definition_id: graph.definition.base.id.clone(),
            definition_lock_version: graph.definition.definition_lock_version(),
        }
    }

    /// 使用长度定界 ID 编码版本化结果引用。
    fn encode(&self) -> String {
        format!(
            "{DEFINITION_RESULT_REF_PREFIX}{}:{}:{}",
            self.definition_id.len(),
            self.definition_id,
            self.definition_lock_version
        )
    }

    /// 解析当前结果引用；旧 raw ID 不得被当前 v3 收据接受。
    fn parse(raw: &str) -> Result<Self> {
        let payload = raw
            .strip_prefix(DEFINITION_RESULT_REF_PREFIX)
            .ok_or_else(idempotency_payload_conflict)?;
        let (length, payload) = payload.split_once(':').ok_or_else(idempotency_payload_conflict)?;
        let length = length
            .parse::<usize>()
            .map_err(|_| idempotency_payload_conflict())?;
        if payload.len() <= length || !payload.is_char_boundary(length) {
            return Err(idempotency_payload_conflict());
        }
        let definition_id = &payload[..length];
        let version = payload[length..]
            .strip_prefix(':')
            .ok_or_else(idempotency_payload_conflict)?
            .parse::<u64>()
            .map_err(|_| idempotency_payload_conflict())?;
        if definition_id.is_empty() {
            return Err(idempotency_payload_conflict());
        }
        Ok(Self {
            definition_id: definition_id.to_string(),
            definition_lock_version: version,
        })
    }

    /// 证明当前图仍是该命令冻结的精确结果版本与资源。
    fn ensure_matches(
        &self,
        graph: &DefinitionGraph,
        expectation: &DefinitionResultExpectation,
    ) -> Result<()> {
        let resource_matches = match expectation {
            DefinitionResultExpectation::ProcessKind(process_kind) => {
                graph.definition.process_kind == *process_kind
            }
            DefinitionResultExpectation::DefinitionId(definition_id) => {
                graph.definition.base.id == *definition_id
            }
        };
        if resource_matches
            && graph.definition.base.id == self.definition_id
            && graph.definition.definition_lock_version() == self.definition_lock_version
        {
            return Ok(());
        }
        Err(idempotency_payload_conflict())
    }
}

/// 配置状态。
fn configuration_status(
    requirement: ApprovalRequirement,
    published: Option<u32>,
    draft: Option<u32>,
) -> DefinitionConfigurationStatus {
    let _ = draft;
    match requirement {
        ApprovalRequirement::NoApproval => DefinitionConfigurationStatus::NotApplicable,
        ApprovalRequirement::ProcessRequired if published.is_some() => {
            DefinitionConfigurationStatus::Published
        }
        ApprovalRequirement::ProcessRequired => DefinitionConfigurationStatus::MissingConfiguration,
    }
}

/// 类型级允许动作。
fn allowed_actions(
    requirement: ApprovalRequirement,
    can_define: bool,
    published: Option<u32>,
    draft: Option<u32>,
) -> Vec<DefinitionAllowedAction> {
    if !can_define || requirement != ApprovalRequirement::ProcessRequired {
        return Vec::new();
    }
    let mut actions = Vec::new();
    if draft.is_none() {
        actions.push(DefinitionAllowedAction::CreateDraft);
    } else {
        actions.push(DefinitionAllowedAction::ReplaceNodes);
        actions.push(DefinitionAllowedAction::Publish);
    }
    if published.is_some() {
        actions.push(DefinitionAllowedAction::Retire);
    }
    actions
}

/// 审批要求视图。
fn requirement_view(requirement: ApprovalRequirement) -> ApprovalRequirementView {
    match requirement {
        ApprovalRequirement::NoApproval => ApprovalRequirementView::NoApproval,
        ApprovalRequirement::ProcessRequired => ApprovalRequirementView::ProcessRequired,
    }
}

/// 构造详情视图。
fn detail_view(graph: &DefinitionGraph) -> DefinitionDetailView {
    let document_type = document_type_of(graph.definition.process_kind);
    let mut nodes = graph.nodes.clone();
    nodes.sort_by_key(|node| node.display_order);
    DefinitionDetailView {
        definition_id: graph.definition.base.id.clone(),
        document_type,
        document_type_label: document_type.label().to_string(),
        name: graph.definition.name.clone(),
        definition_version: graph.definition.definition_version,
        status: graph.definition.status.as_str().to_string(),
        entry_node_key: graph.definition.entry_node_key.clone(),
        definition_lock_version: graph.definition.definition_lock_version(),
        nodes: nodes.iter().map(node_view).collect(),
        created_by: graph.definition.created_by.as_str().to_string(),
        published_by: graph
            .definition
            .published_by
            .as_ref()
            .map(|item| item.as_str().to_string()),
        published_at: graph.definition.published_at.map(|item| item.unix_secs()),
        retired_by: graph
            .definition
            .retired_by
            .as_ref()
            .map(|item| item.as_str().to_string()),
        retired_at: graph.definition.retired_at.map(|item| item.unix_secs()),
    }
}

/// 构造节点视图。
fn node_view(node: &ApprovalNodeDefinition) -> DefinitionNodeView {
    DefinitionNodeView {
        node_id: node.base.id.clone(),
        node_key: node.node_key.clone(),
        node_name: node.node_name.clone(),
        node_type: node.node_type.as_str().to_string(),
        node_purpose: node.node_purpose.clone(),
        display_order: node.display_order,
        assignee_user_id: node.assignee_participant_id.as_str().to_string(),
        assignee_name_snapshot: node.assignee_label_snapshot.clone(),
    }
}

/// 构造版本摘要。
fn version_item(definition: &ApprovalProcessDefinition) -> DefinitionVersionItem {
    DefinitionVersionItem {
        definition_id: definition.base.id.clone(),
        definition_version: definition.definition_version,
        status: definition.status.as_str().to_string(),
        name: definition.name.clone(),
        definition_lock_version: definition.definition_lock_version(),
    }
}

/// 当前调用方时间。
fn now() -> Result<Timestamp> {
    Ok(Timestamp::from_utc(Utc::now()))
}

/// 构造处理人引用。
fn participant(actor: &AuditActor) -> Result<ParticipantId> {
    ParticipantId::new(actor.id().to_string()).map_err(map_bpm_error)
}

/// 映射 BPM 模型错误。
fn map_model_error(error: ModelError) -> Error {
    match error {
        ModelError::CommandReceiptConflict => {
            Error::from_approval_code(ErrorCode::ApprovalIdempotencyPayloadConflict)
        }
        ModelError::InvalidField(_) | ModelError::InvalidTransition(_) => {
            Error::from_approval_code(ErrorCode::ApprovalDefinitionInvalid)
        }
        ModelError::InvalidStatus(message) => Error::ConflictError(message.to_string()),
        ModelError::Overflow(message) => Error::BusinessLogicError(format!("计数溢出: {message}")),
        other => Error::BusinessLogicError(other.to_string()),
    }
}

/// 映射 BPM 边界错误。
fn map_bpm_error(error: bpm::Error) -> Error {
    let _ = error;
    Error::from_approval_code(ErrorCode::ApprovalDefinitionInvalid)
}

/// v3 身份构造失败只可能来自服务端固定 domain，按内部错误处理。
fn map_identity_error(error: ModelError) -> Error {
    Error::Internal(format!("审批定义命令身份构造失败: {error}"))
}

/// 返回稳定幂等载荷冲突。
fn idempotency_payload_conflict() -> Error {
    Error::from_approval_code(ErrorCode::ApprovalIdempotencyPayloadConflict)
}

/// 证明旧收据结果完整对应历史载荷；不可证明时不得降级回放。
fn ensure_legacy_result(
    receipt: &ApprovalCommandReceipt,
    graph: &DefinitionGraph,
    proof: &LegacyDefinitionProof,
) -> Result<()> {
    let definition = &graph.definition;
    let proven = match proof {
        LegacyDefinitionProof::Unprovable => false,
        LegacyDefinitionProof::Published {
            definition_id,
            expected_lock,
            actor_id,
        } => {
            let published_lock = expected_lock.checked_add(2);
            let retired_lock = expected_lock.checked_add(3);
            let lock_matches = match definition.status {
                ApprovalDefinitionStatus::Published => {
                    published_lock == Some(definition.definition_lock_version())
                }
                ApprovalDefinitionStatus::Retired => {
                    retired_lock == Some(definition.definition_lock_version())
                }
                ApprovalDefinitionStatus::Draft => false,
            };
            receipt.result_ref == *definition_id
                && definition.base.id == *definition_id
                && definition
                    .published_by
                    .as_ref()
                    .is_some_and(|actor| actor.as_str() == actor_id)
                && definition.published_at.is_some()
                && lock_matches
        }
        LegacyDefinitionProof::Retired {
            definition_id,
            expected_lock,
            actor_id,
        } => {
            receipt.result_ref == *definition_id
                && definition.base.id == *definition_id
                && definition.status == ApprovalDefinitionStatus::Retired
                && expected_lock.checked_add(1) == Some(definition.definition_lock_version())
                && definition
                    .retired_by
                    .as_ref()
                    .is_some_and(|actor| actor.as_str() == actor_id)
                && definition.retired_at.is_some()
        }
    };
    if proven {
        return Ok(());
    }
    Err(idempotency_payload_conflict())
}

/// 使用当前启用角色重新验证目标类型定义管理权。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `actor` - 已认证操作人
/// * `policy` - 目标单据类型必须审批政策
/// * `executor` - 调用方事务或非事务执行器
///
/// # 返回
/// 当前启用角色授予该类型定义管理权时返回 `Ok(())`。
///
/// # 错误
/// 缺少权限或仅剩禁用角色残留 Casbin grant 时返回禁止。
///
/// # 关键业务约束
/// 回放与 Fresh 必须共用本检查，禁止用账号级 Casbin 判定绕过停用角色。
async fn ensure_definition_admin_permission(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    policy: &ProcessRequiredApprovalPolicy,
    executor: &mut dyn Executor,
) -> Result<()> {
    let visibility = definition_management_visibility_with_executor(db, rbac, actor, executor).await?;
    ensure_definition_admin_allowed(visibility.can_define(policy.document_type))
}

/// 仅把 receipt-first 的唯一键竞争标记为可恢复错误。
fn mark_receipt_duplicate(error: database::Error) -> Error {
    match error {
        error @ database::Error::DuplicateKey(_)
            if error.duplicate_index_name() == Some(APPROVAL_COMMAND_RECEIPT_IDEMPOTENCY_INDEX) =>
        {
            Error::ReceiptDuplicate(error)
        }
        other => Error::from(other),
    }
}

/// 只有收据竞争、瞬态事务和提交结果未知允许退出失败会话后回读。
fn definition_command_may_have_committed(error: &Error) -> bool {
    matches!(
        error,
        Error::ReceiptDuplicate(_) | Error::TransientTransaction(_) | Error::OutcomeUnknown(_)
    )
}

/// 并发胜者可能仍在提交，使用有界指数退避等待新会话可见。
fn definition_recovery_delay(attempt: usize) -> Duration {
    Duration::from_millis(5_u64 << attempt.min(5))
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

/// 发布写库步骤。仅在全部重验通过后允许刷新快照并退役旧版本。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PublishWriteStep {
    /// 允许准备快照并在 receipt-first 后替换图与发布定义。
    RefreshSnapshotsAndRetirePrevious,
}

/// 退役写库步骤。仅当前 PUBLISHED 且锁匹配才允许写回。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetireWriteStep {
    /// 允许对当前已发布定义执行 retire + CAS 写回。
    RetireCurrentPublished,
}

/// `CURRENT_PUBLISHED` 必须命中当前发布定义，缺失则失败关闭。
///
/// # 错误
/// 当前没有已发布定义时返回校验错误。
fn require_current_published<T>(published: Option<T>) -> Result<T> {
    published.ok_or_else(|| Error::from_approval_code(ErrorCode::ApprovalDraftSourceNotAvailable))
}

/// 只能退役当前已发布定义。
///
/// # 错误
/// 无 PUBLISHED 或请求 ID 不是当前发布版时返回业务错误。
fn ensure_retire_target(published_id: Option<&str>, requested_id: &str) -> Result<()> {
    let Some(published_id) = published_id else {
        return Err(Error::BusinessLogicError(
            "当前没有可退役的已发布定义".to_string(),
        ));
    };
    if published_id != requested_id {
        return Err(Error::BusinessLogicError("只能退役当前已发布定义".to_string()));
    }
    Ok(())
}

/// 目标与锁均匹配后才允许退役写回。
///
/// # 错误
/// 无发布版、ID 不匹配或锁版本陈旧时返回错误。
fn decide_retire_write(
    published: Option<&ApprovalProcessDefinition>,
    requested_id: &str,
    expected_lock: u64,
) -> Result<RetireWriteStep> {
    ensure_retire_target(published.map(|item| item.base.id.as_str()), requested_id)?;
    let published =
        published.ok_or_else(|| Error::BusinessLogicError("当前没有可退役的已发布定义".to_string()))?;
    ensure_lock(published, expected_lock)?;
    Ok(RetireWriteStep::RetireCurrentPublished)
}

/// 图、用途、账号或动作任一失败都不得进入发布写库。
///
/// # 错误
/// 返回第一个失败的校验错误。
fn decide_publish_write(
    graph: Result<()>,
    purposes: Result<()>,
    assignees: Result<()>,
    actions: Result<()>,
) -> Result<PublishWriteStep> {
    graph?;
    purposes?;
    assignees?;
    actions?;
    Ok(PublishWriteStep::RefreshSnapshotsAndRetirePrevious)
}

/// 写端口类型级定义管理权闸门。
///
/// # 错误
/// 缺少 `definition_admin` 时禁止写入。
fn ensure_definition_admin_allowed(allowed: bool) -> Result<()> {
    if allowed {
        return Ok(());
    }
    Err(Error::Forbidden("没有该单据类型的流程定义管理权限".to_string()))
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
fn ensure_static_decide_permission(has_decide: bool) -> Result<()> {
    if has_decide {
        return Ok(());
    }
    Err(Error::BusinessLogicError(
        "指定审批人不具备静态审批权限".to_string(),
    ))
}

/// 第二活动草稿错误。
fn second_draft_error() -> Error {
    Error::ConflictError("该单据类型已有活动草稿".to_string())
}

/// 陈旧锁错误。
fn stale_lock_error() -> Error {
    Error::from_approval_code(ErrorCode::ApprovalDefinitionVersionConflict)
}

/// 不泄露存在性的未找到错误。
fn definition_not_found() -> Error {
    Error::NotFound("审批流程定义不存在".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bpm::ids::ApprovalProcessDefinitionId;
    use bpm::ProcessKind;
    use mongodb::error::{Error as MongoError, ErrorKind, WriteError, WriteFailure};
    use serde_json::json;

    fn draft_definition(process_kind: ProcessKind, entry: &str) -> ApprovalProcessDefinition {
        ApprovalProcessDefinition::new_draft(
            ApprovalProcessDefinitionId::new("def-1"),
            process_kind,
            1,
            "测试流程",
            entry,
            ParticipantId::new("admin").unwrap(),
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap()
    }

    fn node(id: &str, key: &str, order: u32, purpose: Option<&str>, user: &str) -> ApprovalNodeDefinition {
        ApprovalNodeDefinition::new(bpm::model::NewNodeDefinition {
            id: ApprovalNodeDefinitionId::new(id),
            process_definition_id: ApprovalProcessDefinitionId::new("def-1"),
            node_key: key.into(),
            node_name: format!("节点{order}"),
            node_purpose: purpose.map(ToOwned::to_owned),
            display_order: order,
            assignee_participant_id: ParticipantId::new(user).unwrap(),
            assignee_label_snapshot: "张三".to_string(),
            at: Timestamp::from_unix_secs(1).unwrap(),
        })
        .unwrap()
    }

    fn two_node_publish_graph() -> DefinitionGraph {
        let nodes = vec![node("id1", "n1", 1, None, "u1"), node("id2", "n2", 2, None, "u2")];
        let definition = draft_definition(ProcessKind::StockAdjustment, "n1");
        DefinitionGraph::rebuild_draft(
            &definition,
            nodes,
            next_transition_ids(2),
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap()
    }

    fn production_source() -> &'static str {
        include_str!("definition.rs")
            .split("mod tests {")
            .next()
            .expect("必须存在生产代码")
    }

    fn source_fn<'a>(source: &'a str, name: &str, next: &str) -> &'a str {
        source
            .split(name)
            .nth(1)
            .and_then(|body| body.split(next).next())
            .unwrap_or(source)
    }

    fn duplicate_key_error(index: Option<&str>) -> database::Error {
        let message = index.map_or_else(
            || "E11000 duplicate key".to_string(),
            |index| format!("E11000 duplicate key index: {index} dup key"),
        );
        let write_error: WriteError = serde_json::from_value(json!({
            "code": 11000,
            "codeName": "DuplicateKey",
            "errmsg": message,
            "errInfo": null,
        }))
        .expect("duplicate fixture");
        let mongo: MongoError = ErrorKind::Write(WriteFailure::WriteError(write_error)).into();
        database::Error::DuplicateKey(mongo)
    }

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
        let create_tx = source_fn(
            production_source(),
            "async fn create_draft_tx",
            "async fn replace_nodes_tx",
        );
        let gate = create_tx.find("decide_create_draft_write").expect("创建闸门");
        let receipt = create_tx.find("write_receipt").expect("receipt");
        assert!(gate < receipt);
        assert!(receipt < create_tx.find("persist_new_draft").expect("persist"));
    }

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
    /// 政策与 ProcessKind 映射穷尽，Service 无 BPM 第二定义源。
    #[test]
    fn policy_mapping_is_exhaustive_and_service_has_no_second_bpm_source() {
        for document_type in ALL_DOCUMENT_TYPES {
            let process_kind = process_kind_of(document_type);
            assert_eq!(document_type_of(process_kind), document_type);
            let _ = policy_of(document_type).unwrap();
        }
        let production = include_str!("definition.rs")
            .split("mod tests {")
            .next()
            .expect("必须存在生产代码");
        assert!(production.contains("validate_linear"));
        assert!(production.contains("plan_replacement_nodes"));
        assert!(!production.contains("generate_linear_transitions"));
        assert!(!production.contains("validate_transition"));
        assert!(!production.contains("validate_entry_node"));
        assert!(!production.contains("entities::approval::"));
        assert!(!production.contains(&format!("{}{}", "CARD_", "SALES_APPROVAL")));
        assert!(!production.contains("validate_definition("));
        assert!(!production.contains("access_control::DataScope"));
        assert!(!production.contains("approval_management_scope"));
    }

    /// 发布重验岗位分离不把提交人隔离伪装成实例 DataScope。
    #[test]
    fn publish_static_separation_does_not_forge_instance_data_scope() {
        validate_static_separation(
            SeparationOfDutiesPolicy::ForbidSubmitterAsApprover,
            &["u1".to_string(), "u1".to_string()],
        )
        .unwrap();
        assert!(production_source().contains("不伪造实例"));
        assert!(!production_source().contains("access_control::DataScope"));
        let failed_assignees = Err(Error::ValidationError(
            "指定审批人账号不存在、已停用或任职失效".into(),
        ));
        assert!(decide_publish_write(Ok(()), Ok(()), failed_assignees, Ok(())).is_err());
        let publish_tx = source_fn(production_source(), "async fn publish_tx", "async fn retire_tx");
        let gate = publish_tx.find("decide_publish_write").expect("发布闸门");
        let prepared = publish_tx.find("prepare_publish_graph").expect("刷新规划");
        let receipt = publish_tx.find("write_receipt").expect("收据首写");
        let graph_write = publish_tx.find("replace_graph").expect("图写入");
        assert!(gate < prepared);
        assert!(prepared < receipt);
        assert!(receipt < graph_write);
    }

    /// 账号或静态权限重验失败时发布不得进入写库步骤。
    #[test]
    fn publish_revalidation_failure_blocks_write_step() {
        let no_decide = ensure_static_decide_permission(false).unwrap_err();
        assert!(matches!(
            no_decide,
            Error::BusinessLogicError(message) if message.contains("approval") || message.contains("静态审批")
        ));
        ensure_static_decide_permission(true).unwrap();
        assert!(decide_publish_write(
            Ok(()),
            Ok(()),
            Err(Error::ValidationError(
                "指定审批人账号不存在、已停用或任职失效".into()
            )),
            Ok(()),
        )
        .is_err());
        assert!(decide_publish_write(
            Ok(()),
            Ok(()),
            Err(Error::BusinessLogicError("指定审批人不具备静态审批权限".into())),
            Ok(()),
        )
        .is_err());
    }

    /// 类型级权限由 Service 强制：无管理权不得进入写端口。
    #[test]
    fn type_level_permission_is_enforced_by_helpers() {
        let denied = ensure_definition_admin_allowed(false).unwrap_err();
        assert!(matches!(denied, Error::Forbidden(message) if message.contains("流程定义管理权限")));
        ensure_definition_admin_allowed(true).unwrap();
        let visibility = DefinitionManagementVisibility::from_type_permissions(
            vec![DocumentType::StockAdjustment],
            Vec::new(),
        );
        assert!(ensure_can_read_detail(&visibility, DocumentType::StockAdjustment).is_ok());
        assert!(ensure_can_read_detail(&visibility, DocumentType::SalesOrder).is_err());
        let claimed = DefinitionManagementVisibility::from_type_permissions(
            vec![DocumentType::StockAdjustment, DocumentType::SalesOrder],
            vec![DocumentType::SalesOrder],
        );
        let intersected = visibility.intersect(&claimed);
        assert!(intersected.can_define(DocumentType::StockAdjustment));
        assert!(!intersected.can_define(DocumentType::SalesOrder));
        assert_eq!(
            allowed_actions(ApprovalRequirement::ProcessRequired, false, None, None),
            Vec::new()
        );
        assert_eq!(
            allowed_actions(ApprovalRequirement::ProcessRequired, true, None, None),
            vec![DefinitionAllowedAction::CreateDraft]
        );
        assert_eq!(
            allowed_actions(ApprovalRequirement::ProcessRequired, true, None, Some(1)),
            vec![
                DefinitionAllowedAction::ReplaceNodes,
                DefinitionAllowedAction::Publish
            ]
        );
        assert_eq!(
            allowed_actions(ApprovalRequirement::ProcessRequired, true, Some(2), Some(3)),
            vec![
                DefinitionAllowedAction::ReplaceNodes,
                DefinitionAllowedAction::Publish,
                DefinitionAllowedAction::Retire
            ]
        );
        assert_eq!(
            allowed_actions(ApprovalRequirement::ProcessRequired, true, Some(2), None),
            vec![
                DefinitionAllowedAction::CreateDraft,
                DefinitionAllowedAction::Retire
            ]
        );
        assert!(allowed_actions(ApprovalRequirement::NoApproval, true, None, None).is_empty());
    }

    /// 无已发布定义一律配置缺失，含从未发布仅有草稿、退役后仍留草稿。
    #[test]
    fn retired_catalog_status_is_missing_configuration() {
        assert_eq!(
            configuration_status(ApprovalRequirement::ProcessRequired, None, None),
            DefinitionConfigurationStatus::MissingConfiguration
        );
        assert_eq!(
            configuration_status(ApprovalRequirement::ProcessRequired, Some(1), Some(2)),
            DefinitionConfigurationStatus::Published
        );
        assert_eq!(
            configuration_status(ApprovalRequirement::ProcessRequired, None, Some(1)),
            DefinitionConfigurationStatus::MissingConfiguration
        );
        assert_eq!(
            configuration_status(ApprovalRequirement::NoApproval, None, None),
            DefinitionConfigurationStatus::NotApplicable
        );
    }

    /// 四条定义命令固定 v3 golden，Create/Replace domain 隔离且完整载荷可区分。
    #[test]
    fn canonical_payloads_are_stable() {
        assert!(matches!(
            parse_idempotency_key("  "),
            Err(Error::ValidationError(_))
        ));
        assert!(matches!(
            parse_idempotency_key(&"界".repeat(43)),
            Err(Error::ValidationError(_))
        ));
        let key = parse_idempotency_key("  key-1  ").expect("规范 key");
        assert_eq!(key.as_str(), "key-1");
        let create = create_draft_identity(
            key.clone(),
            DocumentType::StockAdjustment,
            "库存调整",
            DraftSource::Empty,
            "admin-1",
        )
        .expect("创建身份");
        assert_eq!(
            create.current.scope().as_str(),
            "v3:dfdad7669efdc17e5e2b4a013c9e3ff9fe2fc972e2a1f730557e11039fa77574"
        );
        assert_eq!(
            create.current.digest().as_str(),
            "v3:987f7ac0bf5bbd761d38f649dfe68a98f6e6c923605ce31e22a08b714cd470a6"
        );

        let nodes = prepare_definition_nodes(vec![
            DefinitionNodeRequest {
                node_id: None,
                node_name: " 财务 ".to_string(),
                display_order: 2,
                assignee_user_id: " u2 ".to_string(),
            },
            DefinitionNodeRequest {
                node_id: Some(" node-1 ".to_string()),
                node_name: " 仓储复核 ".to_string(),
                display_order: 1,
                assignee_user_id: " u1 ".to_string(),
            },
        ])
        .expect("节点规范化");
        assert_eq!(nodes[0].node_name, "仓储复核");
        assert_eq!(nodes[0].node_id.as_deref(), Some("node-1"));
        assert_eq!(nodes[1].assignee_user_id, "u2");
        let replace = replace_nodes_identity(key.clone(), "def-1", 3, &nodes, "admin-1").expect("替换身份");
        assert_eq!(
            replace.current.scope().as_str(),
            "v3:8fb38d7db619da0a2f1f164fa6575056b8ecadc45c066396f07aaba97c5a06f3"
        );
        assert_eq!(
            replace.current.digest().as_str(),
            "v3:6623c0836bdd568ef761a797ee6b73dbb799bae019eb20a479fe1f101a5dc3f5"
        );

        let publish = lock_command_identity(
            key.clone(),
            ApprovalCommandKind::PublishDefinition,
            PUBLISH_DEFINITION_COMMAND_DOMAIN,
            "def-1",
            3,
            "admin-1",
        );
        let publish = publish.expect("发布身份");
        assert_eq!(
            publish.current.scope().as_str(),
            "v3:5001966af5c0b5d438e4adb337a5f052689071e4fad50ab18800eb5055394786"
        );
        assert_eq!(
            publish.current.digest().as_str(),
            "v3:fe9b47c8cd4d4e5b98c9b4ebab65b8b4756668aed1d8068705b7bd0024c5b345"
        );

        let retire = lock_command_identity(
            key.clone(),
            ApprovalCommandKind::RetireDefinition,
            RETIRE_DEFINITION_COMMAND_DOMAIN,
            "def-1",
            3,
            "admin-1",
        )
        .expect("退役身份");
        assert_eq!(
            retire.current.scope().as_str(),
            "v3:c89adeafb2fcbb835edf67a65ad722c7fdf172e8c44be3c54372e2aa9e00fb86"
        );
        assert_eq!(
            retire.current.digest().as_str(),
            "v3:6ae60ca970d53104d79c8ff103c64542fa44118fb0436416eaccd6e9e8fe34db"
        );

        assert_ne!(create.current.scope(), replace.current.scope());
        let create_changed = create_draft_identity(
            key.clone(),
            DocumentType::StockAdjustment,
            "库存\u{1f}调整",
            DraftSource::Empty,
            "admin-1",
        )
        .expect("分隔符文本");
        assert_ne!(create.current.digest(), create_changed.current.digest());
        let literal_null_nodes = prepare_definition_nodes(vec![DefinitionNodeRequest {
            node_id: Some("NULL".to_string()),
            node_name: "审批".to_string(),
            display_order: 1,
            assignee_user_id: "用户甲".to_string(),
        }])
        .expect("字面量 NULL");
        let absent_nodes = prepare_definition_nodes(vec![DefinitionNodeRequest {
            node_id: None,
            node_name: "审批".to_string(),
            display_order: 1,
            assignee_user_id: "用户甲".to_string(),
        }])
        .expect("空可选值");
        assert_ne!(
            replace_nodes_identity(key.clone(), "def-1", 3, &literal_null_nodes, "admin-1")
                .unwrap()
                .current
                .digest(),
            replace_nodes_identity(key.clone(), "def-1", 3, &absent_nodes, "admin-1")
                .unwrap()
                .current
                .digest()
        );

        assert!(matches!(
            map_model_error(ModelError::CommandReceiptConflict),
            Error::Coded(ErrorCode::ApprovalIdempotencyPayloadConflict)
        ));
        let result_ref = DefinitionCommandResultRef {
            definition_id: "def-1".to_string(),
            definition_lock_version: 1,
        }
        .encode();
        let same = ApprovalCommandReceipt::new(
            ApprovalCommandReceiptId::new("r1"),
            &create.current,
            result_ref,
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap();
        same.reconcile_identity(&create.current).expect("同载荷必须回读");
        assert!(matches!(
            map_model_error(same.reconcile_identity(&create_changed.current).unwrap_err()),
            Error::Coded(ErrorCode::ApprovalIdempotencyPayloadConflict)
        ));

        let create = source_fn(
            production_source(),
            "pub async fn create_definition_draft",
            "pub async fn replace_definition_nodes",
        );
        assert!(
            create.find("parse_idempotency_key").expect("key 规范化")
                < create.find("ensure_definition_admin").expect("创建权限")
        );
        let publish = source_fn(
            production_source(),
            "pub async fn publish_definition",
            "pub async fn retire_definition",
        );
        assert!(
            publish.find("parse_idempotency_key").expect("key 规范化")
                < publish.find("require_graph").expect("首次查库")
        );
        let retire = source_fn(
            production_source(),
            "pub async fn retire_definition",
            "pub async fn definition_versions",
        );
        assert!(
            retire.find("parse_idempotency_key").expect("key 规范化")
                < retire.find("require_graph").expect("首次查库")
        );
        let replace = source_fn(
            production_source(),
            "pub async fn replace_definition_nodes",
            "pub async fn publish_definition",
        );
        assert!(
            replace.find("parse_idempotency_key").expect("key 规范化")
                < replace.find("require_graph").expect("首次查库")
        );
        let recover = source_fn(
            production_source(),
            "async fn recover_definition_command",
            "async fn create_draft_tx",
        );
        assert!(recover.contains("with_transaction"));
        assert!(recover.contains("replay_prepared_definition_receipt"));
        assert!(recover.contains("ensure_definition_admin_permission"));
        assert!(!recover.contains("contains("));
        let prepared_replay = source_fn(
            production_source(),
            "async fn replay_prepared_definition_receipt",
            "async fn replay_legacy_receipt",
        );
        assert!(
            prepared_replay
                .find("replay_current_receipt")
                .expect("必须先查当前 v3")
                < prepared_replay
                    .find("replay_legacy_receipt")
                    .expect("当前不存在后才能查旧候选")
        );
    }

    /// 只有幂等三元组唯一索引冲突可进入胜者回读，ID 或未知索引必须失败关闭。
    #[test]
    fn receipt_duplicate_recovery_requires_exact_idempotency_index() {
        let idempotency = mark_receipt_duplicate(duplicate_key_error(Some(
            APPROVAL_COMMAND_RECEIPT_IDEMPOTENCY_INDEX,
        )));
        assert!(matches!(idempotency, Error::ReceiptDuplicate(_)));
        assert!(definition_command_may_have_committed(&idempotency));

        let id_collision =
            mark_receipt_duplicate(duplicate_key_error(Some("uk_approval_command_receipts_id")));
        assert!(matches!(id_collision, Error::ConflictError(_)));
        assert!(!definition_command_may_have_committed(&id_collision));

        let unknown = mark_receipt_duplicate(duplicate_key_error(None));
        assert!(matches!(unknown, Error::ConflictError(_)));
        assert!(!definition_command_may_have_committed(&unknown));
    }

    /// 当前结果引用冻结 UTF-8 字节长度与锁版本，资源或版本漂移必须失败关闭。
    #[test]
    fn definition_result_reference_is_versioned_and_exact() {
        let unicode = DefinitionCommandResultRef {
            definition_id: "定义-一".to_string(),
            definition_lock_version: 7,
        };
        assert_eq!(
            DefinitionCommandResultRef::parse(&unicode.encode()).unwrap(),
            unicode
        );
        assert!(DefinitionCommandResultRef::parse("def-1").is_err());
        assert!(DefinitionCommandResultRef::parse("definition-v1:99:def-1:1").is_err());

        let graph = two_node_publish_graph();
        let result_ref = DefinitionCommandResultRef::from_graph(&graph);
        result_ref
            .ensure_matches(
                &graph,
                &DefinitionResultExpectation::DefinitionId(graph.definition.base.id.clone()),
            )
            .expect("精确结果");
        assert!(result_ref
            .ensure_matches(
                &graph,
                &DefinitionResultExpectation::DefinitionId("other".to_string()),
            )
            .is_err());
        let mut drifted = graph.clone();
        drifted.definition.base.version += 1;
        assert!(result_ref
            .ensure_matches(
                &drifted,
                &DefinitionResultExpectation::ProcessKind(ProcessKind::StockAdjustment),
            )
            .is_err());
    }

    /// 旧收据只按精确候选读取；发布/退役可由不可变事实证明，创建来源不可证明。
    #[test]
    fn legacy_definition_receipts_require_complete_immutable_proof() {
        let key = IdempotencyKey::parse("key-legacy").unwrap();
        let identity = lock_command_identity(
            key,
            ApprovalCommandKind::PublishDefinition,
            PUBLISH_DEFINITION_COMMAND_DOMAIN,
            "def-1",
            2,
            "admin",
        )
        .unwrap();
        let mut receipt = ApprovalCommandReceipt::new(
            ApprovalCommandReceiptId::new("receipt-legacy"),
            &identity.current,
            "def-1",
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap();
        receipt.scope_id = "def-1".to_string();
        receipt.payload_digest = legacy_payload_digest(&["2", "admin"]);

        let draft = two_node_publish_graph();
        let refreshed = DefinitionGraph::rebuild_draft(
            &draft.definition,
            draft.nodes,
            next_transition_ids(2),
            Timestamp::from_unix_secs(2).unwrap(),
        )
        .unwrap();
        let mut published = refreshed;
        published
            .definition
            .publish(
                ParticipantId::new("admin").unwrap(),
                Timestamp::from_unix_secs(3).unwrap(),
            )
            .unwrap();
        ensure_legacy_result(
            &receipt,
            &published,
            &LegacyDefinitionProof::Published {
                definition_id: "def-1".to_string(),
                expected_lock: 2,
                actor_id: "admin".to_string(),
            },
        )
        .expect("发布旧结果可证明");
        assert!(ensure_legacy_result(
            &receipt,
            &published,
            &LegacyDefinitionProof::Published {
                definition_id: "def-1".to_string(),
                expected_lock: 2,
                actor_id: "other".to_string(),
            },
        )
        .is_err());
        assert!(ensure_legacy_result(&receipt, &published, &LegacyDefinitionProof::Unprovable,).is_err());

        let retire_expected = published.definition.definition_lock_version();
        published
            .definition
            .retire(
                ParticipantId::new("admin").unwrap(),
                Timestamp::from_unix_secs(4).unwrap(),
            )
            .unwrap();
        ensure_legacy_result(
            &receipt,
            &published,
            &LegacyDefinitionProof::Retired {
                definition_id: "def-1".to_string(),
                expected_lock: retire_expected,
                actor_id: "admin".to_string(),
            },
        )
        .expect("退役旧结果可证明");
    }

    /// 四条定义写命令均在完整只读校验后先写收据，再写图、状态或审计。
    #[test]
    fn definition_commands_are_receipt_first_after_read_validation() {
        let create = source_fn(
            production_source(),
            "async fn create_draft_tx",
            "async fn replace_nodes_tx",
        );
        let create_receipt = create.find("write_receipt").expect("创建收据");
        assert!(create.find("build_new_draft").expect("创建只读准备") < create_receipt);
        assert!(create_receipt < create.find("persist_new_draft").expect("创建图写入"));

        let replace = source_fn(
            production_source(),
            "async fn replace_nodes_tx",
            "async fn publish_tx",
        );
        let replace_receipt = replace.find("write_receipt").expect("替换收据");
        assert!(replace.find("prepare_replacement").expect("替换只读准备") < replace_receipt);
        assert!(replace_receipt < replace.find("apply_draft_graph").expect("替换图写入"));

        let publish = source_fn(production_source(), "async fn publish_tx", "async fn retire_tx");
        let publish_receipt = publish.find("write_receipt").expect("发布收据");
        assert!(publish.find("prepare_publish_graph").expect("发布只读准备") < publish_receipt);
        assert!(publish_receipt < publish.find("replace_graph").expect("发布图写入"));
        assert!(publish_receipt < publish.find("publish_and_retire_previous").expect("发布状态写入"));

        let retire = source_fn(
            production_source(),
            "async fn retire_tx",
            "async fn build_new_draft",
        );
        let retire_receipt = retire.find("write_receipt").expect("退役收据");
        assert!(retire.find("decide_retire_write").expect("退役只读校验") < retire_receipt);
        assert!(retire_receipt < retire.find("approval_process_definitions").expect("退役状态写入"));
    }
    /// 只能退役当前 PUBLISHED；无发布版或非当前版失败，锁匹配才允许写回。
    #[test]
    fn retire_only_current_published() {
        assert!(matches!(
            ensure_retire_target(None, "def-1"),
            Err(Error::BusinessLogicError(message)) if message.contains("没有可退役")
        ));
        assert!(matches!(
            ensure_retire_target(Some("def-pub"), "def-draft"),
            Err(Error::BusinessLogicError(message)) if message.contains("只能退役当前已发布定义")
        ));
        ensure_retire_target(Some("def-1"), "def-1").expect("ID 匹配应放行");

        let published = {
            let mut definition = draft_definition(ProcessKind::StockAdjustment, "n1");
            definition
                .publish(
                    ParticipantId::new("admin").unwrap(),
                    Timestamp::from_unix_secs(2).unwrap(),
                )
                .unwrap();
            definition
        };
        let lock = published.definition_lock_version();
        assert!(matches!(
            decide_retire_write(None, "def-1", lock),
            Err(Error::BusinessLogicError(message)) if message.contains("没有可退役")
        ));
        assert!(matches!(
            decide_retire_write(Some(&published), "other-id", lock),
            Err(Error::BusinessLogicError(message)) if message.contains("只能退役当前已发布定义")
        ));
        assert!(matches!(
            decide_retire_write(Some(&published), &published.base.id, lock + 1),
            Err(Error::Coded(ErrorCode::ApprovalDefinitionVersionConflict))
        ));
        assert!(matches!(
            decide_retire_write(Some(&published), &published.base.id, lock),
            Ok(RetireWriteStep::RetireCurrentPublished)
        ));
        assert_eq!(
            configuration_status(ApprovalRequirement::ProcessRequired, None, None),
            DefinitionConfigurationStatus::MissingConfiguration
        );
        let retire_tx = source_fn(
            production_source(),
            "async fn retire_tx",
            "async fn build_new_draft",
        );
        assert!(retire_tx.contains("decide_retire_write"));
    }

    /// draft_source=CURRENT_PUBLISHED 缺发布源必须失败关闭。
    #[test]
    fn current_published_requires_existing_definition() {
        assert!(matches!(
            require_current_published::<&str>(None),
            Err(Error::Coded(ErrorCode::ApprovalDraftSourceNotAvailable))
        ));
        assert_eq!(require_current_published(Some("def-pub")).unwrap(), "def-pub");
        let copy_src = source_fn(
            production_source(),
            "async fn copy_published_draft",
            "async fn persist_new_draft",
        );
        assert!(copy_src.contains("require_current_published"));
    }
    /// 详情按 display_order 排序；版本摘要带状态、名称和锁。
    #[test]
    fn detail_and_version_views_assemble_sorted_nodes_and_audit() {
        let mut graph = two_node_publish_graph();
        graph.nodes.reverse();
        let draft_view = detail_view(&graph);
        assert_eq!(
            draft_view
                .nodes
                .iter()
                .map(|item| item.display_order)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(draft_view.nodes[0].node_key, "n1");
        assert_eq!(draft_view.nodes[0].node_type, "USER_APPROVAL");
        assert_eq!(draft_view.nodes[1].node_type, "USER_APPROVAL");
        assert_eq!(draft_view.entry_node_key, "n1");
        assert_eq!(draft_view.created_by, "admin");
        assert_eq!(
            draft_view.definition_lock_version,
            graph.definition.definition_lock_version()
        );
        assert!(draft_view.published_by.is_none());
        assert!(draft_view.retired_by.is_none());

        let actor = ParticipantId::new("admin").unwrap();
        graph
            .definition
            .publish(actor.clone(), Timestamp::from_unix_secs(2).unwrap())
            .unwrap();
        let published_view = detail_view(&graph);
        assert_eq!(published_view.status, "PUBLISHED");
        assert_eq!(published_view.published_by.as_deref(), Some("admin"));
        assert_eq!(published_view.published_at, Some(2));

        graph
            .definition
            .retire(actor, Timestamp::from_unix_secs(3).unwrap())
            .unwrap();
        let retired_view = detail_view(&graph);
        assert_eq!(retired_view.status, "RETIRED");
        assert_eq!(retired_view.retired_by.as_deref(), Some("admin"));
        assert_eq!(retired_view.retired_at, Some(3));

        let item = version_item(&graph.definition);
        assert_eq!(item.definition_id, graph.definition.base.id);
        assert_eq!(item.definition_version, 1);
        assert_eq!(item.status, "RETIRED");
        assert_eq!(item.name, "测试流程");
        assert_eq!(
            item.definition_lock_version,
            graph.definition.definition_lock_version()
        );
        assert_eq!(node_view(&graph.nodes[1]).node_id, "id1");
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
