//! 域 D11 `warehouse` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 仓库创建与修订追加：跨集合（warehouses + warehouse_revisions + 审计）→
//!   `database::Transactional::with_transaction`，保证「仓库身份 + 当前修订指针 +
//!   修订快照」原子可见（数据模型 §6.3）；
//! - 仓库-SKU 预警策略单集合 CRUD → `&mut NoTransaction`（审计日志按 D01
//!   既有写法独立写入）。
//!
//! 业务规则来自 entities（`Warehouse::new`/`WarehouseRevision::new` 完成校验与
//! 规范化，`SensitiveText` 封装敏感列），Service 只编排：字典存在性校验、
//! 修订序号递增、生效区间重叠检测与事务写入。地址/联系人指纹复用
//! `entities::file_asset::content_fingerprint`（数据模型 §4.5.5 唯一实现）；
//! 跨域只调对方 Repository（D10 `skus` 校验策略引用的 SKU；D02 `audit_logs`
//! 写审计），禁止 Service 依赖 Service。

use database::{AccessControlExt, CatalogExt, NoTransaction, Transactional, WarehouseExt};
use entities::common::time::BusinessDate;
use entities::file_asset::content_fingerprint;
use entities::ids::{SkuId, WarehouseId};
use entities::ids::{WarehouseRevisionId, WarehouseSkuPolicyId};
use entities::warehouse::status::EnableStatus;
use entities::warehouse::warehouse_entity::{Warehouse, WarehouseData};
use entities::warehouse::warehouse_revision::{SensitiveText, WarehouseRevision, WarehouseRevisionData};
use entities::warehouse::warehouse_sku_policy::{
    WarehouseSkuPolicy, WarehouseSkuPolicyData, WarehouseSkuPolicyUpdate,
};
use id_generator::next_id;
use mongodb::bson::doc;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

mod dto;

pub use self::dto::{
    CreateWarehouseRequest, CreateWarehouseSkuPolicyRequest, PageView, UpdateWarehouseRequest,
    UpdateWarehouseSkuPolicyRequest, WarehouseListParams, WarehouseRevisionListParams, WarehouseRevisionView,
    WarehouseSkuPolicyListParams, WarehouseSkuPolicyView, WarehouseView,
};

/// 仓库列表筛选条件类型（经 `WarehouseExt` 关联类型跨 crate 可达）。
type WarehouseFilter = <mongodb::Database as WarehouseExt>::WarehouseFilter;
/// 仓库修订列表筛选条件类型。
type WarehouseRevisionFilter = <mongodb::Database as WarehouseExt>::WarehouseRevisionFilter;
/// 仓库-SKU 预警策略列表筛选条件类型。
type WarehouseSkuPolicyFilter = <mongodb::Database as WarehouseExt>::WarehouseSkuPolicyFilter;

/// 敏感字段指纹密钥（HMAC-SHA256，带密钥禁止裸摘要）。
///
/// 地基修订候选：密钥应从 `config` 注入（services 层当前只持有 `Database`），
/// 此处使用固定占位密钥；指纹算法与实体形态已固化（数据模型 §4.5.5）。
const FINGERPRINT_KEY: &[u8] = b"erp-warehouse-sensitive-fingerprint-key-v1";

/// 仓库域服务。
///
/// 提供仓库稳定身份、仓库修订与仓库-SKU 预警策略的创建、查询、更新编排。
pub struct WarehouseService {
    db: Database,
}

impl WarehouseService {
    /// 创建仓库域服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 分页查询仓库列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`warehouse_code`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn warehouse_list(&self, params: &WarehouseListParams) -> Result<PageView<WarehouseView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = WarehouseFilter {
            warehouse_code: query.warehouse_code,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .warehouses()
            .search_warehouses(&filter, &mut NoTransaction)
            .await?;
        // 投影行类型属于仓储私有子树（`repository/mod.rs` 冻结），按字段映射为响应视图。
        let items = page
            .items
            .into_iter()
            .map(|row| WarehouseView {
                id: row.id,
                warehouse_code: row.warehouse_code,
                status: row.status,
                created_at: row.created_at,
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 创建仓库（仓库稳定身份 + 首个修订，跨集合事务）。
    ///
    /// 地址与联系人生成带密钥 HMAC 指纹的 `SensitiveText` 后落库
    /// （数据模型 §4.5.5：数据库加密列 + 查询指纹，禁止裸摘要）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建仓库的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - warehouse_code 重复（唯一索引透出）
    pub async fn warehouse_create(
        &self,
        req: CreateWarehouseRequest,
        actor: &AuditActor,
    ) -> Result<WarehouseView> {
        req.validate()?;
        let id = WarehouseId::new(next_id());
        let revision_id = WarehouseRevisionId::new(next_id());
        let mut warehouse = Warehouse::new(
            id.clone(),
            WarehouseData {
                warehouse_code: req.warehouse_code,
                status: req.status.unwrap_or(EnableStatus::Active),
            },
            actor.id(),
        )?;
        let revision = build_warehouse_revision(
            id.clone(),
            revision_id,
            1,
            WarehouseRevisionInput {
                name: req.name,
                address: req.address,
                contact: req.contact,
                effective_from: req.effective_from,
                effective_to: req.effective_to,
                change_reason: req.change_reason,
            },
        )?;
        let audit = actor
            .clone()
            .resource_log("warehouse.create", "warehouse", id.to_string())?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.warehouse()
                        .create_warehouse_with_revision(&mut warehouse, &revision, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<Warehouse, crate::errors::Error>(warehouse)
                })
            })
            .await
            .map(Into::into)
    }

    /// 更新仓库（追加新修订并更新稳定身份，跨集合事务）。
    ///
    /// `warehouse_code` 是稳定代码不可修改；「有库存或有效预占时不得停用」
    /// 需要库存域数据，当前未接线（见域报告「未实现且已知的缺口」）。
    ///
    /// # 参数
    /// * `id` - 仓库 ID
    /// * `req` - 更新请求（含期望版本与新修订快照）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后仓库的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 仓库不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn warehouse_update(
        &self,
        id: &str,
        req: UpdateWarehouseRequest,
        actor: &AuditActor,
    ) -> Result<WarehouseView> {
        req.validate()?;
        let mut warehouse = self
            .db
            .warehouses()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("仓库不存在".to_string()))?;
        ensure_version(warehouse.base.version, req.version)?;
        let revision_no = self.next_warehouse_revision_no(id).await?;
        let revision = build_warehouse_revision(
            WarehouseId::new(id.to_string()),
            WarehouseRevisionId::new(next_id()),
            revision_no,
            WarehouseRevisionInput {
                name: req.name,
                address: req.address,
                contact: req.contact,
                effective_from: req.effective_from,
                effective_to: req.effective_to,
                change_reason: req.change_reason,
            },
        )?;
        warehouse.stable.current_revision_id = Some(revision.base.id.clone());
        warehouse.stable.status = req.status;
        warehouse.stable.touch(actor.id());
        let audit = actor
            .clone()
            .resource_log("warehouse.update", "warehouse", warehouse.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.warehouse_revisions().create(&revision, session).await?;
                    db.warehouses().update(&mut warehouse, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<Warehouse, crate::errors::Error>(warehouse)
                })
            })
            .await
            .map(Into::into)
    }

    /// 分页查询仓库修订列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`warehouse_id`/`name` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（不含加密地址/联系人等敏感字段）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn warehouse_revision_list(
        &self,
        params: &WarehouseRevisionListParams,
    ) -> Result<PageView<WarehouseRevisionView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = WarehouseRevisionFilter {
            warehouse_id: query.warehouse_id,
            name: query.name,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .warehouse_revisions()
            .search_warehouse_revisions(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| WarehouseRevisionView {
                id: row.id,
                warehouse_id: row.warehouse_id,
                revision_no: row.revision_no,
                name: row.name,
                effective_from: row.effective_from,
                effective_to: row.effective_to,
                change_reason: row.change_reason,
                created_at: row.created_at,
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 分页查询仓库-SKU 预警策略列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`warehouse_id`/`sku_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn warehouse_sku_policy_list(
        &self,
        params: &WarehouseSkuPolicyListParams,
    ) -> Result<PageView<WarehouseSkuPolicyView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = WarehouseSkuPolicyFilter {
            warehouse_id: query.warehouse_id,
            sku_id: query.sku_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .warehouse_sku_policies()
            .search_warehouse_sku_policies(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| WarehouseSkuPolicyView {
                id: row.id,
                warehouse_id: row.warehouse_id,
                sku_id: row.sku_id,
                minimum_available_quantity: row.minimum_available_quantity,
                status: row.status,
                effective_from: row.effective_from,
                effective_to: row.effective_to,
                created_at: row.created_at,
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 创建仓库-SKU 预警策略（单集合写入，无事务）。
    ///
    /// 校验仓库与 SKU 存在，且与同 (仓库, SKU) 既有策略的启用区间不重叠
    /// （数据模型 §6.3：同一仓库和 SKU 的启用区间不得重叠）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建策略的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `NotFound` - 仓库或 SKU 不存在
    /// * `BusinessLogicError` - 与既有策略的启用区间重叠
    pub async fn warehouse_sku_policy_create(
        &self,
        req: CreateWarehouseSkuPolicyRequest,
        actor: &AuditActor,
    ) -> Result<WarehouseSkuPolicyView> {
        req.validate()?;
        self.db
            .warehouses()
            .find_by_id(req.warehouse_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("仓库不存在".to_string()))?;
        self.db
            .skus()
            .find_by_id(req.sku_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("SKU不存在".to_string()))?;
        self.ensure_no_overlap(
            &req.warehouse_id,
            &req.sku_id,
            req.effective_from,
            req.effective_to,
        )
        .await?;
        let id = WarehouseSkuPolicyId::new(next_id());
        let policy = WarehouseSkuPolicy::new(
            id.clone(),
            WarehouseSkuPolicyData {
                warehouse_id: req.warehouse_id,
                sku_id: req.sku_id,
                minimum_available_quantity: req.minimum_available_quantity,
                status: req.status.unwrap_or(EnableStatus::Active),
                effective_from: req.effective_from,
                effective_to: req.effective_to,
            },
        )?;
        let audit = actor.clone().resource_log(
            "warehouse_sku_policy.create",
            "warehouse_sku_policy",
            id.to_string(),
        )?;
        self.db
            .warehouse_sku_policies()
            .create(&policy, &mut NoTransaction)
            .await?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
        Ok(policy.into())
    }

    /// 更新仓库-SKU 预警策略（乐观锁语义；`warehouse_id`/`sku_id` 是策略身份）。
    ///
    /// # 参数
    /// * `id` - 策略 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后策略的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 策略不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn warehouse_sku_policy_update(
        &self,
        id: &str,
        req: UpdateWarehouseSkuPolicyRequest,
        actor: &AuditActor,
    ) -> Result<WarehouseSkuPolicyView> {
        req.validate()?;
        let mut policy = self
            .db
            .warehouse_sku_policies()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("预警策略不存在".to_string()))?;
        ensure_version(policy.base.version, req.version)?;
        policy.update(WarehouseSkuPolicyUpdate {
            minimum_available_quantity: req.minimum_available_quantity,
            status: req.status,
        })?;
        let audit = actor.clone().resource_log(
            "warehouse_sku_policy.update",
            "warehouse_sku_policy",
            policy.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.warehouse_sku_policies().update(&mut policy, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<WarehouseSkuPolicy, crate::errors::Error>(policy)
                })
            })
            .await
            .map(Into::into)
    }

    /// 删除仓库-SKU 预警策略（软删除，乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 策略 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回删除结果。
    ///
    /// # 错误
    /// * `NotFound` - 策略不存在
    /// * `ConflictError` - 并发修改（CAS 冲突）
    pub async fn warehouse_sku_policy_delete(&self, id: &str, actor: &AuditActor) -> Result<()> {
        let mut policy = self
            .db
            .warehouse_sku_policies()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("预警策略不存在".to_string()))?;
        let audit = actor.clone().resource_log(
            "warehouse_sku_policy.delete",
            "warehouse_sku_policy",
            policy.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.warehouse_sku_policies()
                        .soft_delete(&mut policy, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await
    }

    /// 计算某仓库已有修订的最大序号 + 1（唯一索引兜底并发）。
    ///
    /// # 参数
    /// * `warehouse_id` - 仓库 ID
    ///
    /// # 返回
    /// 返回下一个修订序号。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    async fn next_warehouse_revision_no(&self, warehouse_id: &str) -> Result<u32> {
        let revisions = self
            .db
            .warehouse_revisions()
            .find_many(doc! { "warehouse_id": warehouse_id }, &mut NoTransaction)
            .await?;
        Ok(revisions
            .iter()
            .map(|revision| revision.revision.revision_no)
            .max()
            .unwrap_or(0)
            + 1)
    }

    /// 校验新策略与同 (仓库, SKU) 既有策略的启用区间不重叠（半开区间）。
    ///
    /// # 参数
    /// * `warehouse_id` - 仓库
    /// * `sku_id` - SKU
    /// * `effective_from` - 新策略生效开始日
    /// * `effective_to` - 新策略生效结束日（空为无限期）
    ///
    /// # 返回
    /// 不重叠时返回 `Ok(())`。
    ///
    /// # 错误
    /// 与既有策略区间重叠时返回 `BusinessLogicError`。
    async fn ensure_no_overlap(
        &self,
        warehouse_id: &WarehouseId,
        sku_id: &SkuId,
        effective_from: BusinessDate,
        effective_to: Option<BusinessDate>,
    ) -> Result<()> {
        let existing = self
            .db
            .warehouse_sku_policies()
            .find_many(
                doc! {
                    "warehouse_id": warehouse_id.to_string(),
                    "sku_id": sku_id.to_string(),
                },
                &mut NoTransaction,
            )
            .await?;
        if existing.iter().any(|policy| {
            intervals_overlap(
                effective_from,
                effective_to,
                policy.effective_from,
                policy.effective_to,
            )
        }) {
            return Err(Error::BusinessLogicError(
                "同一仓库和SKU的启用区间不得重叠".to_string(),
            ));
        }
        Ok(())
    }
}

/// 校验期望版本与当前版本一致（乐观锁语义）。
///
/// # 参数
/// * `current` - 当前版本
/// * `expected` - 期望版本
///
/// # 返回
/// 一致时返回 `Ok(())`。
///
/// # 错误
/// 不一致时返回 `ConflictError`（HTTP 409）。
fn ensure_version(current: u64, expected: u64) -> Result<()> {
    if current != expected {
        return Err(Error::ConflictError(
            "数据已被其他请求修改，请刷新后重试".to_string(),
        ));
    }
    Ok(())
}

/// 仓库修订构建输入（名称/地址/联系人/生效区间/变更原因）。
struct WarehouseRevisionInput {
    /// 仓库名称。
    name: String,
    /// 地址明文。
    address: String,
    /// 联系人明文。
    contact: String,
    /// 生效起始日。
    effective_from: BusinessDate,
    /// 生效截止日。
    effective_to: Option<BusinessDate>,
    /// 变更原因。
    change_reason: String,
}

/// 构造仓库修订（名称/变更原因校验 + 地址/联系人敏感值指纹化）。
///
/// 地址与联系人按数据模型 §4.5.5 生成带密钥 HMAC 指纹的 `SensitiveText`；
/// 密文列当前无生产级加密原语（见域报告「地基修订候选」），P3 落库为
/// 明文占位 + 真 HMAC 查询指纹，列表投影与 Debug 均不暴露。
///
/// # 参数
/// * `warehouse_id` - 所属仓库
/// * `revision_id` - 修订 ID
/// * `revision_no` - 修订序号
/// * `input` - 修订内容
///
/// # 返回
/// 返回仓库修订实体。
///
/// # 错误
/// 字段校验失败时返回错误。
fn build_warehouse_revision(
    warehouse_id: WarehouseId,
    revision_id: WarehouseRevisionId,
    revision_no: u32,
    input: WarehouseRevisionInput,
) -> Result<WarehouseRevision> {
    Ok(WarehouseRevision::new(
        revision_id,
        WarehouseRevisionData {
            warehouse_id,
            revision_no,
            name: input.name,
            address: SensitiveText::new(
                input.address.clone(),
                content_fingerprint(&input.address, FINGERPRINT_KEY),
            )?,
            contact: SensitiveText::new(
                input.contact.clone(),
                content_fingerprint(&input.contact, FINGERPRINT_KEY),
            )?,
            effective_from: input.effective_from,
            effective_to: input.effective_to,
            change_reason: input.change_reason,
        },
    )?)
}

/// 判断两个半开生效区间 `[from, to)` 是否重叠（`None` 表示无限期）。
///
/// # 参数
/// * `from_a` / `to_a` - 区间 A
/// * `from_b` / `to_b` - 区间 B
///
/// # 返回
/// 存在交集时返回 `true`。
fn intervals_overlap(
    from_a: BusinessDate,
    to_a: Option<BusinessDate>,
    from_b: BusinessDate,
    to_b: Option<BusinessDate>,
) -> bool {
    let a_ends = to_a.unwrap_or(BusinessDate::from_ymd(9999, 12, 31).expect("远期末日"));
    let b_ends = to_b.unwrap_or(BusinessDate::from_ymd(9999, 12, 31).expect("远期末日"));
    from_a < b_ends && from_b < a_ends
}

#[cfg(test)]
mod tests {
    use super::intervals_overlap;
    use entities::common::time::BusinessDate;

    #[test]
    fn half_open_intervals_overlap_correctly() {
        let from_a = BusinessDate::from_ymd(2026, 1, 1).unwrap();
        let to_a = BusinessDate::from_ymd(2026, 3, 1).unwrap();
        let from_b = BusinessDate::from_ymd(2026, 2, 1).unwrap();
        let to_b = BusinessDate::from_ymd(2026, 4, 1).unwrap();
        // [2026-01-01, 2026-03-01) ∩ [2026-02-01, 2026-04-01) 有交集。
        assert!(intervals_overlap(from_a, Some(to_a), from_b, Some(to_b)));

        // 相邻半开区间：前一段结束日 = 后一段开始日，不重叠。
        let adj_from = BusinessDate::from_ymd(2026, 3, 1).unwrap();
        let adj_to = BusinessDate::from_ymd(2026, 5, 1).unwrap();
        assert!(!intervals_overlap(
            from_a,
            Some(to_a),
            adj_from,
            Some(adj_to)
        ));

        // 无限期与有限区间重叠。
        assert!(intervals_overlap(from_a, None, from_b, Some(to_b)));

        // 完全分离的区间不重叠。
        let from_c = BusinessDate::from_ymd(2026, 5, 1).unwrap();
        let to_c = BusinessDate::from_ymd(2026, 6, 1).unwrap();
        assert!(!intervals_overlap(from_a, Some(to_a), from_c, Some(to_c)));
    }
}
