//! 域 D02 `document_registry` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 单据注册（幂等）、动作追加、关系/参与人写入：单集合 + 审计日志 → 跨审计
//!   集合写入按 TRANSACTIONS.md「基本用法」编排在 `with_transaction` 内
//!   （仓库尚无 `run_audited_transaction` 模板，与 source_registry 同款写法）；
//! - 查询一律 `&mut NoTransaction`。
//!
//! 跨域：只经 `DatabaseExt` 调对方域 Repository（P3-service-api §2）。本域依赖
//! D01：登记外部来源单据时，经 `db.external_identity_maps()` 校验来源身份映射
//! 已登记（读取对方仓储，不经过对方 Service）。

use database::{
    AccessControlExt, DocumentRegistryExt, Executor, NoTransaction, SourceRegistryExt, Transactional,
};
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::{
    BusinessDocument, BusinessDocumentData, BusinessDocumentId, DocumentParticipant, DocumentRelation,
    DocumentRelationId, DocumentType, WorkflowAction, WorkflowActionId,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

mod dto;

pub use self::dto::{
    AppendWorkflowActionRequest, BusinessDocumentListParams, BusinessDocumentView,
    CreateDocumentParticipantRequest, CreateDocumentRelationRequest, DocumentParticipantView,
    DocumentRelationView, PageView, RegisterBusinessDocumentRequest, WorkflowActionListParams,
    WorkflowActionView,
};

/// 构造跨域单据注册行。
///
/// 稳定 `document_id` 与业务实体主键一致；草稿尚未分配正式号时 `document_no` 可为空。
///
/// # 参数
/// * `document_id` - 与业务实体相同的稳定主键
/// * `document_type` - 合同固定单据类型
/// * `document_no` - 正式编号；草稿传空
///
/// # 返回
/// 返回尚未正式化、尚未绑定的注册实体。
///
/// # 错误
/// 编号超长时返回校验错误。
pub fn new_registered_document(
    document_id: impl AsRef<str>,
    document_type: DocumentType,
    document_no: impl Into<String>,
) -> Result<BusinessDocument> {
    BusinessDocument::new(
        BusinessDocumentId::new(document_id.as_ref()),
        BusinessDocumentData {
            document_type,
            document_no: document_no.into(),
        },
    )
    .map_err(Into::into)
}

/// 在调用方事务内持久化单据注册行。
///
/// # 参数
/// * `db` - 数据库
/// * `document` - 已构造的注册行
/// * `executor` - 调用方执行器
///
/// # 错误
/// 唯一键冲突或仓储失败时返回错误。
pub async fn persist_registered_document(
    db: &mongodb::Database,
    document: &BusinessDocument,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.business_documents().create(document, executor).await?;
    Ok(())
}

/// 按执行器查询注册行。
///
/// # 错误
/// 仓储读取失败时返回错误。
pub async fn find_registered_document(
    db: &mongodb::Database,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<BusinessDocument>> {
    db.business_documents()
        .find_by_id(document_id, executor)
        .await
        .map_err(Into::into)
}

/// 按执行器查询单据审批绑定。
///
/// # 错误
/// 单据不存在或仓储失败时返回错误。
pub async fn find_approval_binding(
    db: &mongodb::Database,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalDefinitionBinding>> {
    let document = find_registered_document(db, document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("业务单据未注册".to_string()))?;
    Ok(document.approval_binding)
}

/// 单据注册列表筛选条件类型（经 `DocumentRegistryExt` 关联类型跨 crate 可达）。
type BusinessDocumentFilter = <mongodb::Database as DocumentRegistryExt>::BusinessDocumentFilter;
/// 工作流动作列表筛选条件类型。
type WorkflowActionFilter = <mongodb::Database as DocumentRegistryExt>::WorkflowActionFilter;

/// 单据注册服务。
///
/// 提供单据注册、工作流动作、单据关系与参与人的查询与写入编排。
pub struct DocumentRegistryService {
    db: Database,
}

impl DocumentRegistryService {
    /// 创建单据注册服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 幂等注册业务单据。
    ///
    /// 唯一约束由 `uk_business_documents_identity` 承担：同身份同 ID 幂等成功，
    /// 同身份不同 ID 透出唯一索引冲突（409）。外部来源单据可携带 D01 来源身份
    /// 映射 ID，提供时必须已登记（跨域读取对方仓储）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回注册行视图（幂等命中时返回已存在行）。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `NotFound` - 外部身份映射不存在
    /// * `ConflictError` - 同身份不同 ID 的重复注册（唯一索引透出）
    pub async fn register_business_document(
        &self,
        req: RegisterBusinessDocumentRequest,
        actor: &AuditActor,
    ) -> Result<BusinessDocumentView> {
        req.validate()?;
        if let Some(map_id) = &req.external_identity_map_id {
            self.db
                .external_identity_maps()
                .find_by_id(map_id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("外部身份映射不存在".to_string()))?;
        }
        let id = req
            .id
            .map(BusinessDocumentId::new)
            .unwrap_or_else(|| BusinessDocumentId::new(next_id()));
        let doc = BusinessDocument::new(
            id,
            BusinessDocumentData {
                document_type: req.document_type,
                document_no: req.document_no,
            },
        )?;
        let audit = actor.clone().resource_log(
            "business_document.register",
            "business_document",
            doc.base.id.clone(),
        )?;

        // 幂等注册按「先插后查」返回已存在行（repository 注释：不需要事务执行器），
        // 审计日志独立写入；已存在行命中时不再追加审计。
        let existing = self
            .db
            .business_documents()
            .register(&doc, &mut NoTransaction)
            .await?;
        if let Some(existing) = existing {
            return Ok(existing.into());
        }
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;

        Ok(doc.into())
    }

    /// 分页查询单据注册列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn business_document_list(
        &self,
        params: &BusinessDocumentListParams,
    ) -> Result<PageView<BusinessDocumentView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = BusinessDocumentFilter {
            document_type: query.document_type,
            document_no: query.document_no,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .business_documents()
            .search_business_documents(&filter, &mut NoTransaction)
            .await?;
        // 投影行类型属于仓储私有子树（`repository/mod.rs` 冻结，无法命名），
        // 此处按字段映射为响应视图，避免把仓储类型泄漏到接口层。
        let items = page
            .items
            .into_iter()
            .map(|row| BusinessDocumentView {
                id: row.id,
                document_type: row.document_type,
                document_no: row.document_no,
                formalized_at: row.formalized_at,
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询单据注册详情。
    ///
    /// # 参数
    /// * `id` - 单据注册 ID
    ///
    /// # 返回
    /// 返回注册行视图。
    ///
    /// # 错误
    /// * `NotFound` - 单据注册不存在
    pub async fn business_document_detail(&self, id: &str) -> Result<BusinessDocumentView> {
        let doc = self
            .db
            .business_documents()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("单据注册不存在".to_string()))?;
        Ok(doc.into())
    }

    /// 查询注册行上的审批绑定。
    ///
    /// # 参数
    /// * `id` - 单据注册 ID
    ///
    /// # 返回
    /// 无绑定返回 `None`。
    ///
    /// # 错误
    /// 单据未注册时返回 `NotFound`。
    pub async fn approval_binding(&self, id: &str) -> Result<Option<ApprovalDefinitionBinding>> {
        find_approval_binding(&self.db, id, &mut NoTransaction).await
    }

    /// 追加工作流动作。
    ///
    /// 动作追加是单据域状态迁移的审计留痕（§6.1：追加式动作，只追加不修改）；
    /// 目标单据必须已注册。责任角色取操作人账号类型（后台账号当前只有 admin；
    /// 业务角色由单据域自己的事务注入）。
    ///
    /// # 参数
    /// * `req` - 追加请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建的动作视图。
    ///
    /// # 错误
    /// * `NotFound` - 目标单据未注册
    /// * `ValidationError` - 请求体校验失败
    pub async fn append_workflow_action(
        &self,
        req: AppendWorkflowActionRequest,
        actor: &AuditActor,
    ) -> Result<WorkflowActionView> {
        req.validate()?;
        self.ensure_document_registered(&req.document_id).await?;
        let action = WorkflowAction::new(
            WorkflowActionId::new(next_id()),
            req.into_data(actor.kind().as_str(), actor.id()),
        )?;
        let audit = actor.clone().resource_log(
            "workflow_action.append",
            "workflow_action",
            action.base.id.clone(),
        )?;
        let action_for_tx = action.clone();
        self.write_with_audit(move |tx_db, session| {
            Box::pin(async move {
                tx_db.workflow_actions().create(&action_for_tx, session).await?;
                tx_db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await?;

        Ok(action.into())
    }

    /// 分页查询工作流动作列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`document_id`/`actor_id`/`action_type` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn workflow_action_list(
        &self,
        params: &WorkflowActionListParams,
    ) -> Result<PageView<WorkflowActionView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = WorkflowActionFilter {
            document_id: query.document_id,
            actor_id: query.actor_id,
            action_type: query.action_type,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .workflow_actions()
            .search_workflow_actions(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| WorkflowActionView {
                id: row.id,
                document_id: row.document_id,
                action_type: row.action_type,
                from_status: row.from_status,
                to_status: row.to_status,
                actor_id: row.actor_id,
                actor_role: row.actor_role,
                comment: None,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询单据的全部关系（出向 + 入向）。
    ///
    /// # 参数
    /// * `document_id` - 业务单据 ID
    ///
    /// # 返回
    /// 返回出向与入向关系的合并视图。
    ///
    /// # 错误
    /// * `RepositoryError` - 数据库查询失败
    pub async fn document_relation_list(
        &self,
        document_id: &BusinessDocumentId,
    ) -> Result<Vec<DocumentRelationView>> {
        let mut relations = self
            .db
            .document_relations()
            .list_by_from_document(document_id, &mut NoTransaction)
            .await?;
        relations.extend(
            self.db
                .document_relations()
                .list_by_to_document(document_id, &mut NoTransaction)
                .await?,
        );
        relations.sort_by_key(|relation| relation.base.created_at);
        Ok(relations.into_iter().map(Into::into).collect())
    }

    /// 建立单据关系。
    ///
    /// 两端单据必须均已注册；关系方向与类型语义由实体层校验（禁止自关联）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建的关系视图。
    ///
    /// # 错误
    /// * `NotFound` - 任一单据未注册
    /// * `ConflictError` - 同向关系重复（唯一索引透出）
    pub async fn create_document_relation(
        &self,
        req: CreateDocumentRelationRequest,
        actor: &AuditActor,
    ) -> Result<DocumentRelationView> {
        req.validate()?;
        self.ensure_document_registered(&req.from_document_id).await?;
        self.ensure_document_registered(&req.to_document_id).await?;
        let relation = DocumentRelation::new(DocumentRelationId::new(next_id()), req.into_data())?;
        let audit = actor.clone().resource_log(
            "document_relation.create",
            "document_relation",
            relation.base.id.clone(),
        )?;
        let relation_for_tx = relation.clone();
        self.write_with_audit(move |tx_db, session| {
            Box::pin(async move {
                tx_db
                    .document_relations()
                    .create(&relation_for_tx, session)
                    .await?;
                tx_db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await?;

        Ok(relation.into())
    }

    /// 按参与人查询其参与过的全部单据（“我的参与单据”）。
    ///
    /// # 参数
    /// * `user_id` - 参与人用户 ID
    ///
    /// # 返回
    /// 返回按参与时间倒序排列的参与记录视图。
    ///
    /// # 错误
    /// * `ValidationError` - 用户 ID 为空白
    /// * `RepositoryError` - 数据库查询失败
    pub async fn document_participant_list(&self, user_id: &str) -> Result<Vec<DocumentParticipantView>> {
        if user_id.trim().is_empty() {
            return Err(Error::ValidationError("用户ID不能为空".to_string()));
        }
        let items = self
            .db
            .document_participants()
            .list_by_user(user_id, &mut NoTransaction)
            .await?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    /// 登记单据参与人。
    ///
    /// 参与记录只追加不删除（§4.6 客户历史参与者查看权依据）；目标单据必须
    /// 已注册；记录人取操作人账号 ID。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建的参与人视图。
    ///
    /// # 错误
    /// * `NotFound` - 目标单据未注册
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_document_participant(
        &self,
        req: CreateDocumentParticipantRequest,
        actor: &AuditActor,
    ) -> Result<DocumentParticipantView> {
        req.validate()?;
        self.ensure_document_registered(&req.document_id).await?;
        let participant = DocumentParticipant::new(
            entities::ids::DocumentParticipantId::new(next_id()),
            req.into_data(actor.id()),
        )?;
        let audit = actor.clone().resource_log(
            "document_participant.create",
            "document_participant",
            participant.base.id.clone(),
        )?;
        let participant_for_tx = participant.clone();
        self.write_with_audit(move |tx_db, session| {
            Box::pin(async move {
                tx_db
                    .document_participants()
                    .create(&participant_for_tx, session)
                    .await?;
                tx_db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await?;

        Ok(participant.into())
    }

    /// 校验业务单据已注册。
    ///
    /// # 参数
    /// * `document_id` - 业务单据 ID
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 单据未注册时返回 `NotFound`。
    async fn ensure_document_registered(&self, document_id: &BusinessDocumentId) -> Result<()> {
        self.db
            .business_documents()
            .find_by_id(document_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("业务单据未注册".to_string()))?;
        Ok(())
    }

    /// 在单个事务中执行业务写入并追加审计日志（TRANSACTIONS.md「基本用法」）。
    ///
    /// 事务闭包内不做外部 HTTP / 文件 I/O；提交结果未知由
    /// `database::Error::CommitOutcomeUnknown` → `services::Error::OutcomeUnknown` 映射。
    ///
    /// # 参数
    /// * `transaction` - 业务写入闭包（收到事务会话执行器）
    ///
    /// # 返回
    /// 返回事务闭包的结果。
    async fn write_with_audit<T, F>(&self, transaction: F) -> Result<T>
    where
        T: Send + 'static,
        F: for<'a> FnOnce(
                &'a mongodb::Database,
                &'a mut mongodb::ClientSession,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = crate::errors::Result<T>> + Send + 'a>,
            > + Send
            + 'static,
    {
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| Box::pin(async move { transaction(&db, session).await }))
            .await
    }
}
