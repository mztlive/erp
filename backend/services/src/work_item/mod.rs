//! 域 D03 `work_item` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 领取走仓储条件更新（行锁原子），其余状态迁移（暂挂/转交/完成/关闭）为
//!   多集合写入（业务行 + 审计日志）→ `with_transaction` 内原子提交；
//! - 查询一律 `&mut NoTransaction`。
//!
//! 跨域：只经 `DatabaseExt` 调对方域 Repository（P3-service-api §2）。本域依赖
//! D02：派发业务单据类任务时，经 `db.business_documents()` 校验单据已注册；
//! 「对象类型是否属于业务单据」的判定来自 `entities::document_registry::DocumentType`
//! 的 serde 目录（跨域开放目录，不复制对方域枚举清单）。

use database::{AccessControlExt, DocumentRegistryExt, NoTransaction, Transactional, WorkItemExt};
use entities::document_registry::DocumentType;
use entities::work_item::{WorkItem, WorkItemId};
use id_generator::next_id;
use mongodb::Database;
use serde::Deserialize;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

mod dto;

pub use self::dto::{
    ClaimWorkItemRequest, CloseWorkItemRequest, CompleteWorkItemRequest, DeferWorkItemRequest,
    DispatchWorkItemRequest, PageView, TransferWorkItemRequest, WorkItemListParams, WorkItemView,
};

/// 待办列表筛选条件类型（经 `WorkItemExt` 关联类型跨 crate 可达）。
type WorkItemFilter = <mongodb::Database as WorkItemExt>::WorkItemFilter;

/// 待办服务。
///
/// 提供正式待办的派发、领取、暂挂、转交、完成与关闭编排。
pub struct WorkItemService {
    db: Database,
}

impl WorkItemService {
    /// 创建待办服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 分页查询待办列表（工作队列）。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传。
    ///
    /// # 参数
    /// * `params` - 查询参数（`owner_role`/`owner_user_id`/`status` 等扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn work_item_list(&self, params: &WorkItemListParams) -> Result<PageView<WorkItemView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = WorkItemFilter {
            work_item_type: query.work_item_type,
            status: query.status,
            owner_role: query.owner_role,
            owner_user_id: query.owner_user_id,
            priority: query.priority,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .work_items()
            .search_work_items(&filter, &mut NoTransaction)
            .await?;
        // 投影行类型属于仓储私有子树（`repository/mod.rs` 冻结，无法命名），
        // 此处按字段映射为响应视图，避免把仓储类型泄漏到接口层。
        let items = page
            .items
            .into_iter()
            .map(|row| WorkItemView {
                id: row.id,
                work_item_type: row.work_item_type,
                business_object_type: row.business_object_type,
                business_object_id: row.business_object_id,
                subject_version: row.subject_version,
                status: row.status,
                owner_role: row.owner_role,
                owner_user_id: row.owner_user_id,
                priority: row.priority,
                due_at: row.due_at,
                reason_code: None,
                impact_summary: None,
                completion_action: String::new(),
                completed_at: None,
                completed_by: None,
                close_reason_code: None,
                close_reason_text: None,
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

    /// 查询待办详情。
    ///
    /// # 参数
    /// * `id` - 待办 ID
    ///
    /// # 返回
    /// 返回完整待办视图（含产生原因与关闭/完成审计字段）。
    ///
    /// # 错误
    /// * `NotFound` - 待办不存在
    pub async fn work_item_detail(&self, id: &str) -> Result<WorkItemView> {
        let item = self
            .db
            .work_items()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("待办不存在".to_string()))?;
        Ok(item.into())
    }

    /// 派发正式待办。
    ///
    /// 同一业务对象、任务类型同时最多一个有效任务，重复派发由部分唯一索引
    /// `uk_work_items_active` 透出 `DuplicateKey` → 409（不做「先查后插」）。
    /// 业务对象类型命中 `DocumentType` 目录（判定来自 entities）时，任务指向
    /// 的单据必须已在 D02 注册。
    ///
    /// # 参数
    /// * `req` - 派发请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建的待办视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `NotFound` - 业务单据类对象未注册
    /// * `ConflictError` - 同一对象同类型已有有效任务（唯一索引透出）
    pub async fn dispatch_work_item(
        &self,
        req: DispatchWorkItemRequest,
        actor: &AuditActor,
    ) -> Result<WorkItemView> {
        req.validate()?;
        if is_business_document_type(&req.business_object_type) {
            self.ensure_business_document_registered(&req.business_object_id)
                .await?;
        }
        let item = WorkItem::new(WorkItemId::new(next_id()), req.into_data())?;
        let audit = actor
            .clone()
            .resource_log("work_item.dispatch", "work_item", item.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let item_for_tx = item.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.work_items().create(&item_for_tx, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(item.into())
    }

    /// 领取待办。
    ///
    /// 领取 = 条件更新（行锁）原子完成（§6.1）：仅当行内状态仍为 `UNCLAIMED`
    /// 时迁移到 `IN_PROGRESS` 并写入领取人；被他人抢先领取或版本陈旧时返回
    /// `OptimisticLockingError` → 409。
    ///
    /// # 参数
    /// * `id` - 待办 ID
    /// * `req` - 领取请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回领取后的待办视图。
    ///
    /// # 错误
    /// * `NotFound` - 待办不存在
    /// * `ConflictError` - 已被他人领取或版本陈旧
    pub async fn claim_work_item(
        &self,
        id: &str,
        req: ClaimWorkItemRequest,
        actor: &AuditActor,
    ) -> Result<WorkItemView> {
        req.validate()?;
        let mut item = self.load_with_version(id, req.version).await?;
        item.claim(actor.id())?;
        let audit = actor
            .clone()
            .resource_log("work_item.claim", "work_item", id.to_string())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let claimed = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.work_items().claim(&mut item, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<WorkItem, crate::errors::Error>(item)
                })
            })
            .await?;

        Ok(claimed.into())
    }

    /// 暂挂待办。
    ///
    /// 暂挂后任务回到待领取状态并清除当前责任人（W02 契约）；乐观锁版本
    /// 不一致或行内状态已变化时返回 409。
    ///
    /// # 参数
    /// * `id` - 待办 ID
    /// * `req` - 暂挂请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回暂挂后的待办视图。
    ///
    /// # 错误
    /// * `NotFound` - 待办不存在
    /// * `ConflictError` - 版本陈旧或并发冲突
    pub async fn defer_work_item(
        &self,
        id: &str,
        req: DeferWorkItemRequest,
        actor: &AuditActor,
    ) -> Result<WorkItemView> {
        req.validate()?;
        let mut item = self.load_with_version(id, req.version).await?;
        item.defer()?;
        self.update_with_audit(item, "work_item.defer", actor).await
    }

    /// 转交待办。
    ///
    /// 转交直接更新责任角色与责任人并记录审计，任务保持处理中（W02 契约）。
    ///
    /// # 参数
    /// * `id` - 待办 ID
    /// * `req` - 转交请求（含期望版本与新责任人）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回转交后的待办视图。
    ///
    /// # 错误
    /// * `NotFound` - 待办不存在
    /// * `ConflictError` - 版本陈旧或并发冲突
    pub async fn transfer_work_item(
        &self,
        id: &str,
        req: TransferWorkItemRequest,
        actor: &AuditActor,
    ) -> Result<WorkItemView> {
        req.validate()?;
        let mut item = self.load_with_version(id, req.version).await?;
        item.transfer(req.owner_role, req.owner_user_id)?;
        self.update_with_audit(item, "work_item.transfer", actor).await
    }

    /// 正式完成任务。
    ///
    /// 仅 `IN_PROGRESS` 可完成（实体状态机校验），写入完成审计；业务事实的
    /// 状态变化由对应强类型事务完成（§6.1），本接口只终结任务本身。
    ///
    /// # 参数
    /// * `id` - 待办 ID
    /// * `req` - 完成请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回完成后的待办视图。
    ///
    /// # 错误
    /// * `NotFound` - 待办不存在
    /// * `ConflictError` - 版本陈旧或并发冲突
    /// * `BusinessLogicError` - 状态机不允许完成
    pub async fn complete_work_item(
        &self,
        id: &str,
        req: CompleteWorkItemRequest,
        actor: &AuditActor,
    ) -> Result<WorkItemView> {
        req.validate()?;
        let mut item = self.load_with_version(id, req.version).await?;
        item.complete(actor.id(), entities::common::time::Instant::now())?;
        self.update_with_audit(item, "work_item.complete", actor).await
    }

    /// 关闭待办。
    ///
    /// 关闭必须记录结构化原因（§6.1：只有重复、误派或已有替代正式任务时允许
    /// 关闭）；任务类型是否允许人工关闭由实体
    /// `WorkItemType::is_manually_closable` 给出，本服务按保守默认拒绝人工关闭
    /// 审批/确认/结果未知/异常补偿类任务。
    ///
    /// # 参数
    /// * `id` - 待办 ID
    /// * `req` - 关闭请求（含期望版本与关闭原因）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回关闭后的待办视图。
    ///
    /// # 错误
    /// * `NotFound` - 待办不存在
    /// * `ConflictError` - 版本陈旧或并发冲突
    /// * `BusinessLogicError` - 状态机不允许关闭或类型不允许人工关闭
    pub async fn close_work_item(
        &self,
        id: &str,
        req: CloseWorkItemRequest,
        actor: &AuditActor,
    ) -> Result<WorkItemView> {
        req.validate()?;
        let mut item = self.load_with_version(id, req.version).await?;
        if !item.work_item_type.is_manually_closable() {
            return Err(Error::BusinessLogicError(
                "该任务类型不允许人工关闭（审批、确认、结果未知或异常补偿任务）".to_string(),
            ));
        }
        item.close(req.into_close_data())?;
        self.update_with_audit(item, "work_item.close", actor).await
    }

    /// 按 ID 加载待办并校验期望版本。
    ///
    /// # 参数
    /// * `id` - 待办 ID
    /// * `expected_version` - 请求携带的期望版本
    ///
    /// # 返回
    /// 返回加载的待办实体。
    ///
    /// # 错误
    /// 待办不存在返回 `NotFound`；版本不一致返回 `ConflictError`。
    async fn load_with_version(&self, id: &str, expected_version: u64) -> Result<WorkItem> {
        let item = self
            .db
            .work_items()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("待办不存在".to_string()))?;
        if item.base.version != expected_version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        Ok(item)
    }

    /// 在单个事务中更新待办并追加审计日志。
    ///
    /// `Repository::update` 以 `id + version` CAS 兜底并发竞争（base.rs：
    /// `OptimisticLockingError` → 服务层 `ConflictError`）。
    ///
    /// # 参数
    /// * `mut item` - 已由实体完成状态迁移的待办
    /// * `action` - 审计动作名
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后的待办视图。
    ///
    /// # 错误
    /// 并发写入冲突或审计写入失败时返回错误。
    async fn update_with_audit(
        &self,
        mut item: WorkItem,
        action: &str,
        actor: &AuditActor,
    ) -> Result<WorkItemView> {
        let audit = actor
            .clone()
            .resource_log(action, "work_item", item.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.work_items().update(&mut item, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<WorkItem, crate::errors::Error>(item)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 校验业务单据已注册（跨域 D02 仓储读取）。
    ///
    /// # 参数
    /// * `document_id` - 业务单据 ID
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 单据未注册时返回 `NotFound`。
    async fn ensure_business_document_registered(&self, document_id: &str) -> Result<()> {
        self.db
            .business_documents()
            .find_by_id(document_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("业务单据未注册".to_string()))?;
        Ok(())
    }
}

/// 判断对象类型代码是否属于业务单据目录（判定来自 entities 的 serde 目录）。
///
/// 业务对象类型是跨域开放目录（§6.1），本服务不复制 `DocumentType` 枚举清单，
/// 通过实体的反序列化目录判定是否命中业务单据类型。
///
/// # 参数
/// * `object_type` - 业务对象类型代码
///
/// # 返回
/// 命中 `DocumentType` 任一变体时返回 `true`。
fn is_business_document_type(object_type: &str) -> bool {
    use serde::de::{
        value::{Error as SerdeError, StrDeserializer},
        IntoDeserializer,
    };
    let deserializer: StrDeserializer<SerdeError> = object_type.into_deserializer();
    DocumentType::deserialize(deserializer).is_ok()
}
