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
//! 规范化，`WarehouseSkuPolicy` 封装生效区间与重叠规则，`SensitiveText` 封装
//! 敏感列），Service 只编排字典存在性校验、修订序号查询与事务写入。地址/联系人指纹复用
//! `entities::file_asset::content_fingerprint`（数据模型 §4.5.5 唯一实现）；
//! 跨域只调对方 Repository（D10 `skus` 校验策略引用的 SKU；D02 `audit_logs`
//! 写审计），禁止 Service 依赖 Service。

use database::{AccessControlExt, CatalogExt, NoTransaction, Transactional, WarehouseExt};
use entities::common::time::BusinessDate;
use entities::file_asset::content_fingerprint;
use entities::ids::WarehouseId;
use entities::ids::{WarehouseRevisionId, WarehouseSkuPolicyId};
use entities::warehouse::status::EnableStatus;
use entities::warehouse::warehouse_entity::{Warehouse, WarehouseData, WarehouseUpdate};
use entities::warehouse::warehouse_revision::{SensitiveText, WarehouseRevision, WarehouseRevisionData};
use entities::warehouse::warehouse_sku_policy::{
    WarehouseSkuPolicy, WarehouseSkuPolicyData, WarehouseSkuPolicyUpdate,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::iam::{self, SharedRbacService};
use entities::work_item::{AvailableWorkItemAccount, WorkItemType};
use entities::{AccountKind, Permission};

mod dto;

pub use self::dto::{
    CreateWarehouseRequest, CreateWarehouseSkuPolicyRequest, PageView,
    UpdateWarehouseFulfillmentHandlersRequest, UpdateWarehouseRequest, UpdateWarehouseSkuPolicyRequest,
    WarehouseFulfillmentHandlerOptionView, WarehouseListParams, WarehouseRevisionListParams,
    WarehouseRevisionView, WarehouseSkuPolicyListParams, WarehouseSkuPolicyView, WarehouseView,
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
    rbac: SharedRbacService,
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
        let rbac = iam::shared_rbac_service(db.clone());
        Self { db, rbac }
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
                inbound_handler_user_id: row.inbound_handler_user_id,
                outbound_handler_user_id: row.outbound_handler_user_id,
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
        self.ensure_handler_eligible(
            &req.inbound_handler_user_id,
            required_fulfillment_permissions("purchase_receipt"),
            "入库",
        )
        .await?;
        self.ensure_handler_eligible(
            &req.outbound_handler_user_id,
            required_fulfillment_permissions("delivery"),
            "仓发",
        )
        .await?;
        let id = WarehouseId::new(next_id());
        let revision_id = WarehouseRevisionId::new(next_id());
        let mut warehouse = Warehouse::new(
            id.clone(),
            WarehouseData {
                warehouse_code: req.warehouse_code,
                status: req.status.unwrap_or(EnableStatus::Active),
                inbound_handler_user_id: Some(req.inbound_handler_user_id),
                outbound_handler_user_id: Some(req.outbound_handler_user_id),
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
        self.ensure_handler_eligible(
            &req.inbound_handler_user_id,
            required_fulfillment_permissions("purchase_receipt"),
            "入库",
        )
        .await?;
        self.ensure_handler_eligible(
            &req.outbound_handler_user_id,
            required_fulfillment_permissions("delivery"),
            "仓发",
        )
        .await?;
        let mut warehouse = self
            .db
            .warehouse()
            .warehouse(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("仓库不存在".to_string()))?;
        if !warehouse.matches_version(req.version) {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        let revision_no = self
            .db
            .warehouse()
            .next_revision_no(id, &mut NoTransaction)
            .await?;
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
        warehouse.update(
            WarehouseUpdate {
                status: Some(req.status),
                inbound_handler_user_id: Some(Some(req.inbound_handler_user_id)),
                outbound_handler_user_id: Some(Some(req.outbound_handler_user_id)),
            },
            actor.id(),
        )?;
        warehouse.apply_revision(&revision)?;
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

    /// 更新仓库入库与仓发经办人。
    ///
    /// # 参数
    /// * `id` - 仓库稳定 ID
    /// * `req` - 期望版本与两个具体经办人
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后的仓库稳定视图。
    ///
    /// # 错误
    /// 仓库不存在、版本冲突、账号不可用或缺少对应履约权限时返回错误。
    ///
    /// # 关键业务约束
    /// 配置变更只用于之后新建的履约任务，不改派已开放任务。
    pub async fn warehouse_fulfillment_handlers_update(
        &self,
        id: &str,
        req: UpdateWarehouseFulfillmentHandlersRequest,
        actor: &AuditActor,
    ) -> Result<WarehouseView> {
        req.validate()?;
        self.ensure_handler_eligible(
            &req.inbound_handler_user_id,
            required_fulfillment_permissions("purchase_receipt"),
            "入库",
        )
        .await?;
        self.ensure_handler_eligible(
            &req.outbound_handler_user_id,
            required_fulfillment_permissions("delivery"),
            "仓发",
        )
        .await?;

        let mut warehouse = self
            .db
            .warehouse()
            .warehouse(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("仓库不存在".to_string()))?;
        if !warehouse.matches_version(req.version) {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        let audit_message = format!(
            "inbound:{}->{};outbound:{}->{}",
            warehouse.inbound_handler_user_id.as_deref().unwrap_or("未配置"),
            req.inbound_handler_user_id,
            warehouse.outbound_handler_user_id.as_deref().unwrap_or("未配置"),
            req.outbound_handler_user_id,
        );
        warehouse.update(
            WarehouseUpdate {
                status: None,
                inbound_handler_user_id: Some(Some(req.inbound_handler_user_id)),
                outbound_handler_user_id: Some(Some(req.outbound_handler_user_id)),
            },
            actor.id(),
        )?;
        let audit = actor.clone().resource_log_with_message(
            "warehouse.fulfillment_handlers.update",
            "warehouse",
            warehouse.base.id.clone(),
            Some(audit_message),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.warehouses().update(&mut warehouse, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<Warehouse, crate::errors::Error>(warehouse)
                })
            })
            .await
            .map(Into::into)
    }

    /// 列出仓库收发责任配置可选的具体账号。
    ///
    /// # 返回
    /// 返回可登录管理账号及其入库、仓发权限资格。
    ///
    /// # 错误
    /// 账号或权限数据读取失败时返回错误。
    pub async fn warehouse_fulfillment_handler_options(
        &self,
    ) -> Result<Vec<WarehouseFulfillmentHandlerOptionView>> {
        let inbound = handler_permissions(required_fulfillment_permissions("purchase_receipt"));
        let outbound = handler_permissions(required_fulfillment_permissions("delivery"));
        let accounts = self
            .db
            .accounts()
            .list_by_kind(AccountKind::Admin, &mut NoTransaction)
            .await?;
        let mut options = Vec::new();
        for account in accounts {
            if AvailableWorkItemAccount::from_account(&account).is_err() {
                continue;
            }
            let permissions = self
                .rbac
                .permissions(account.kind, account.base.id.as_str())
                .await?;
            let inbound_eligible = inbound
                .iter()
                .all(|required| permissions.iter().any(|granted| granted.covers(required)));
            let outbound_eligible = outbound
                .iter()
                .all(|required| permissions.iter().any(|granted| granted.covers(required)));
            if !inbound_eligible && !outbound_eligible {
                continue;
            }
            let login_account = account.secret.account().to_string();
            options.push(WarehouseFulfillmentHandlerOptionView {
                user_id: account.base.id,
                display_name: account.name,
                account: login_account,
                inbound_eligible,
                outbound_eligible,
            });
        }
        options.sort_by(|left, right| {
            left.display_name
                .cmp(&right.display_name)
                .then_with(|| left.user_id.cmp(&right.user_id))
        });
        Ok(options)
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
            .warehouse()
            .warehouse(req.warehouse_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("仓库不存在".to_string()))?;
        self.db
            .skus()
            .find_by_id(req.sku_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("SKU不存在".to_string()))?;
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
        let existing = self
            .db
            .warehouse()
            .sku_policies_for_dimensions(&policy.warehouse_id, &policy.sku_id, &mut NoTransaction)
            .await?;
        policy
            .ensure_no_overlap(&existing)
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
        let audit = actor.clone().resource_log(
            "warehouse_sku_policy.create",
            "warehouse_sku_policy",
            id.to_string(),
        )?;
        let policy_for_tx = policy.clone();
        crate::transaction::run_audited(&self.db, audit, move |db, session| {
            Box::pin(async move {
                db.warehouse_sku_policies()
                    .create(&policy_for_tx, session)
                    .await?;
                Ok(())
            })
        })
        .await?;
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
            .warehouse()
            .sku_policy(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("预警策略不存在".to_string()))?;
        if !policy.matches_version(req.version) {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        policy.update(WarehouseSkuPolicyUpdate {
            minimum_available_quantity: req.minimum_available_quantity,
            status: req.status,
        })?;
        let existing = self
            .db
            .warehouse()
            .sku_policies_for_dimensions(&policy.warehouse_id, &policy.sku_id, &mut NoTransaction)
            .await?;
        policy
            .ensure_no_overlap(&existing)
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
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
            .warehouse()
            .sku_policy(id, &mut NoTransaction)
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

    /// 校验仓储经办人是当前有效账号且具备对应正式操作权限。
    async fn ensure_handler_eligible(
        &self,
        account_id: &str,
        required_permissions: &[&str],
        operation_label: &str,
    ) -> Result<()> {
        let account_id = account_id.trim();
        let account = self
            .db
            .accounts()
            .find_work_item_account(account_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| {
                Error::BusinessLogicError(format!("{operation_label}经办人账号不存在或已停用，请重新选择"))
            })?;
        AvailableWorkItemAccount::from_account(&account).map_err(|_| {
            Error::BusinessLogicError(format!("{operation_label}经办人账号不可用，请重新选择"))
        })?;
        let required = handler_permissions(required_permissions);
        let permissions = self.rbac.permissions(account.kind, account_id).await?;
        if required
            .iter()
            .all(|required| permissions.iter().any(|granted| granted.covers(required)))
        {
            return Ok(());
        }
        Err(Error::BusinessLogicError(format!(
            "{operation_label}经办人缺少对应操作权限，请先调整角色或重新选择"
        )))
    }
}

/// 读取履约工作项登记的完整执行权限；未知对象属于程序配置错误。
fn required_fulfillment_permissions(business_object_type: &str) -> &'static [&'static str] {
    WorkItemType::FulfillmentOperation
        .fulfillment_execution_permissions(business_object_type)
        .expect("仓库责任对象必须登记履约完整执行权限")
}

/// 把固定收发操作权限代码解析为权限对象；非法代码属于程序错误。
fn handler_permissions(codes: &[&str]) -> Vec<Permission> {
    codes
        .iter()
        .map(|code| Permission::parse(code).expect("固定仓储操作权限必须合法"))
        .collect()
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
