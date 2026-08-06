//! 域 D09 `supplier_capability` 服务编排。
//!
//! 一家供应商维护一种或多种能力（多选，§4.5）；能力修订内联结构化快照
//! （§2.2/§4.4），更新时追加能力修订并推进表头 `current_revision_id`；
//! 同一能力有效区间不得重叠（§6.2 跨行约束，事务内校验）。

use database::{AccessControlExt, NoTransaction, SupplierExt, Transactional};
use entities::common::revision::RevisionBase;
use entities::common::time::BusinessDate;
use entities::field_update::FieldUpdate;
use entities::supplier::{
    CapabilityStatus, SupplierCapability, SupplierCapabilityData, SupplierCapabilityId,
    SupplierCapabilityRevision, SupplierCapabilityRevisionData, SupplierCapabilityRevisionId,
    SupplierCapabilityUpdate,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

use super::dto::{
    normalize_sort, CreateSupplierCapabilityRequest, PageView, SortDir, SupplierCapabilityListParams,
    SupplierCapabilityView, UpdateSupplierCapabilityRequest, CAPABILITY_SORT_FIELDS,
};
use super::{page_or_default, page_size_or_default};

/// 能力列表筛选条件类型（经 `SupplierExt` 关联类型跨 crate 可达）。
type SupplierCapabilityFilter = <mongodb::Database as SupplierExt>::SupplierCapabilityFilter;

/// 供应商能力服务。
pub struct SupplierCapabilityService {
    db: Database,
}

impl SupplierCapabilityService {
    /// 创建供应商能力服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 分页查询供应商能力列表。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn supplier_capability_list(
        &self,
        supplier_id: &str,
        params: &SupplierCapabilityListParams,
    ) -> Result<PageView<SupplierCapabilityView>> {
        params.validate()?;
        let (sort_by, sort_dir) = normalize_sort(&params.sort_by, &params.sort_dir, CAPABILITY_SORT_FIELDS)?;
        let filter = SupplierCapabilityFilter {
            supplier_id: Some(entities::ids::SupplierAccountId::new(supplier_id)),
            capability_code: params.capability_code,
            status: params.status,
            page: page_or_default(params.page),
            page_size: page_size_or_default(params.page_size),
            sort_by: Some(sort_by.to_string()),
            sort_ascending: matches!(sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_capabilities()
            .search_supplier_capabilities(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierCapabilityView {
                id: row.id,
                supplier_id: row.supplier_id.to_string(),
                capability_code: row.capability_code,
                service_region: row.service_region,
                owner_user_id: row.owner_user_id,
                fulfillment_note: row.fulfillment_note,
                valid_from: row.valid_from,
                valid_to: row.valid_to,
                status: row.status,
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

    /// 创建供应商能力（跨集合事务：能力 + 首版能力修订 + 审计原子写入）。
    ///
    /// 能力代码重复由 `(supplier_id, capability_code)` 唯一索引拦截
    /// （§6.2），重复提交返回 409。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建能力的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 供应商不存在
    /// * `ConflictError` - 能力代码与既有能力重复（唯一索引透出）
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_supplier_capability(
        &self,
        supplier_id: &str,
        req: CreateSupplierCapabilityRequest,
        actor: &AuditActor,
    ) -> Result<SupplierCapabilityView> {
        req.validate()?;
        self.ensure_supplier_exists(supplier_id).await?;
        let capability_id = SupplierCapabilityId::new(next_id());
        let revision_id = SupplierCapabilityRevisionId::new(next_id());
        let mut capability = SupplierCapability::new(
            capability_id.clone(),
            SupplierCapabilityData {
                supplier_id: entities::ids::SupplierAccountId::new(supplier_id),
                capability_code: req.capability_code,
                service_region: req.service_region,
                owner_user_id: req.owner_user_id,
                fulfillment_note: req.fulfillment_note,
                valid_from: req.valid_from,
                valid_to: req.valid_to,
                status: req.status.unwrap_or(CapabilityStatus::Active),
            },
            actor.id(),
        )?;
        capability.stable.current_revision_id = Some(revision_id.to_string());
        let revision = SupplierCapabilityRevision::new(
            revision_id,
            SupplierCapabilityRevisionData {
                supplier_id: entities::ids::SupplierAccountId::new(supplier_id),
                capability_code: capability.capability_code,
                service_region: capability.service_region.clone(),
                owner_user_id: capability.owner_user_id.clone(),
                fulfillment_note: capability.fulfillment_note.clone(),
                valid_from: capability.valid_from,
                valid_to: capability.valid_to,
                status: capability.stable.status,
                revision_no: 1,
            },
        )?;
        let audit = actor.clone().resource_log(
            "supplier_capability.create",
            "supplier_capability",
            capability.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let capability_for_tx = capability.clone();
        let revision_for_tx = revision.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_capability_revisions()
                        .create(&revision_for_tx, session)
                        .await?;
                    db.supplier_capabilities()
                        .create(&capability_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(capability.into())
    }

    /// 更新供应商能力（乐观锁；跨集合事务：追加能力修订 + CAS 更新能力 + 审计）。
    ///
    /// 内容变更形成新修订快照（§4.4：后续基础资料修改不改变历史修订）；
    /// 同一能力有效区间不得重叠（§6.2 跨行约束，事务内校验）。
    ///
    /// # 参数
    /// * `id` - 能力 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后能力的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 能力不存在
    /// * `ConflictError` - 期望版本与当前版本不一致，或新窗口与既有修订重叠
    /// * `ValidationError` - 请求体校验失败
    pub async fn update_supplier_capability(
        &self,
        id: &str,
        req: UpdateSupplierCapabilityRequest,
        actor: &AuditActor,
    ) -> Result<SupplierCapabilityView> {
        req.validate()?;
        let mut capability = self
            .db
            .supplier_capabilities()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("能力不存在".to_string()))?;
        if capability.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        capability.update(
            SupplierCapabilityUpdate {
                service_region: option_update(req.service_region),
                owner_user_id: req.owner_user_id,
                fulfillment_note: option_update(req.fulfillment_note),
                valid_to: req.valid_to.map_or(FieldUpdate::Unchanged, FieldUpdate::Set),
                status: req.status,
            },
            actor.id(),
        )?;
        let audit = actor.clone().resource_log(
            "supplier_capability.update",
            "supplier_capability",
            capability.base.id.clone(),
        )?;
        let updated_by = actor.id().to_string();

        let db = self.db.clone();
        let client = db.client().clone();
        let mut capability_for_tx = capability.clone();
        let revision_for_tx = SupplierCapabilityRevision::new(
            SupplierCapabilityRevisionId::new(next_id()),
            SupplierCapabilityRevisionData {
                supplier_id: capability.supplier_id.clone(),
                capability_code: capability.capability_code,
                service_region: capability.service_region.clone(),
                owner_user_id: capability.owner_user_id.clone(),
                fulfillment_note: capability.fulfillment_note.clone(),
                valid_from: capability.valid_from,
                valid_to: capability.valid_to,
                status: capability.stable.status,
                revision_no: 0,
            },
        )?;
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let history = db
                        .supplier_capability_revisions()
                        .find_many(
                            ::mongodb::bson::doc! {
                                "supplier_id": capability_for_tx.supplier_id.to_string(),
                                "capability_code": capability_for_tx.capability_code.as_str(),
                            },
                            session,
                        )
                        .await?;
                    let next_no = history
                        .iter()
                        .map(|revision| revision.revision.revision_no)
                        .max()
                        .unwrap_or(0)
                        + 1;
                    ensure_no_window_overlap(
                        &history,
                        capability_for_tx.valid_from,
                        capability_for_tx.valid_to,
                    )?;
                    let revision = SupplierCapabilityRevision {
                        revision: RevisionBase::new(next_no),
                        ..revision_for_tx
                    };
                    capability_for_tx.stable.current_revision_id = Some(revision.base.id.clone());
                    capability_for_tx.stable.touch(&updated_by);
                    db.supplier_capability_revisions()
                        .create(&revision, session)
                        .await?;
                    db.supplier_capabilities()
                        .update(&mut capability_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<SupplierCapability, crate::errors::Error>(capability_for_tx)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 校验供应商角色存在。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    ///
    /// # 返回
    /// 供应商存在返回 `Ok(())`。
    ///
    /// # 错误
    /// * `NotFound` - 供应商不存在
    async fn ensure_supplier_exists(&self, supplier_id: &str) -> Result<()> {
        self.db
            .supplier_accounts()
            .find_by_id(supplier_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
        Ok(())
    }
}

/// 将可选文本入参映射为 `FieldUpdate`：`None` 表示不修改，空字符串表示清除。
///
/// # 参数
/// * `value` - 请求携带的文本
///
/// # 返回
/// 返回实体更新意图。
fn option_update(value: Option<String>) -> FieldUpdate<String> {
    match value {
        Some(raw) if raw.trim().is_empty() => FieldUpdate::Clear,
        Some(raw) => FieldUpdate::Set(raw),
        None => FieldUpdate::Unchanged,
    }
}

/// 校验新能力窗口与既有修订区间不重叠（§6.2：同一能力有效区间不得重叠）。
///
/// # 参数
/// * `history` - 既有能力修订
/// * `valid_from` - 更新后能力生效开始
/// * `valid_to` - 更新后能力生效结束
///
/// # 返回
/// 无重叠返回 `Ok(())`。
///
/// # 错误
/// 与任一既有修订区间重叠时返回 `ConflictError`。
fn ensure_no_window_overlap(
    history: &[SupplierCapabilityRevision],
    valid_from: BusinessDate,
    valid_to: Option<BusinessDate>,
) -> Result<()> {
    for existing in history {
        let a_covers = |day: BusinessDate| valid_to.is_none_or(|end| day <= end);
        let b_covers = |day: BusinessDate| existing.valid_to.is_none_or(|end| day <= end);
        if a_covers(existing.valid_from) && b_covers(valid_from) {
            return Err(Error::ConflictError(
                "更新后能力生效区间与既有修订重叠，请调整生效日期".to_string(),
            ));
        }
    }
    Ok(())
}
