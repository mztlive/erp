//! 域 D09 `supplier` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 创建供应商（supplier_account + 首个商务结算版本 + 审计）→ 跨集合，必须
//!   事务（`SupplierRepository::create_supplier_with_initial_profile` 声明
//!   「必须收到事务执行器」）；
//! - 追加商务版本（版本 + 推进 `current_commercial_profile_revision_id` + 审计）
//!   → 跨集合，必须事务；
//! - 更新/删除供应商 → 单集合 + 审计。
//!
//! 跨域（只走对方 Repository，禁止 Service 依赖 Service）：
//! - D07 `party`：创建供应商前校验主体存在，商务版本签约/付款主体校验存在；
//! - D05 `file_asset`：资质附件存在性校验（qualification 子模块）。

use database::{AccessControlExt, NoTransaction, PartyExt, SupplierExt, Transactional};
use entities::common::revision::RevisionBase;
use entities::field_update::FieldUpdate;
use entities::ids::PartyId;
use entities::supplier::{
    InvoiceType, ReconciliationCycle, SettlementMode, SupplierAccount, SupplierAccountData,
    SupplierAccountId, SupplierAccountStatus, SupplierAccountUpdate, SupplierCommercialProfileRevision,
    SupplierCommercialProfileRevisionData, SupplierCommercialProfileRevisionId,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

pub mod capability;
mod dto;
pub mod qualification;
pub mod rating;

pub use self::dto::{
    CommercialProfileListParams, CommercialProfileView, CreateCommercialProfileRequest,
    CreateSupplierCapabilityRequest, CreateSupplierQualificationRequest, CreateSupplierRatingRequest,
    CreateSupplierRequest, PageView, SupplierCapabilityListParams, SupplierCapabilityView,
    SupplierDetailView, SupplierListParams, SupplierQualificationListParams, SupplierQualificationView,
    SupplierRatingListParams, SupplierRatingView, SupplierView, UpdateSupplierCapabilityRequest,
    UpdateSupplierQualificationRequest, UpdateSupplierRequest,
};

use self::dto::SortDir;

/// 供应商角色列表筛选条件类型（经 `SupplierExt` 关联类型跨 crate 可达）。
type SupplierAccountFilter = <mongodb::Database as SupplierExt>::SupplierAccountFilter;
/// 商务结算版本列表筛选条件类型。
type CommercialProfileFilter = <mongodb::Database as SupplierExt>::SupplierCommercialProfileFilter;

/// 供应商服务。
///
/// 提供供应商角色与商务结算版本的创建、查询与更新编排（§6.2：供应商角色
/// 携带 `current_commercial_profile_revision_id` 指向当前版本）。
pub struct SupplierService {
    db: Database,
}

impl SupplierService {
    /// 创建供应商服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 创建供应商（跨集合事务：supplier_account + 首个商务版本 + 审计原子写入）。
    ///
    /// 前置校验：共用主体存在（D07）、签约主体与付款主体存在（D07）、该主体
    /// 不得已有供应商角色（§6.2：一个 party 最多一个有效供应商角色，唯一
    /// 索引兜底）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建供应商角色的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 共用主体/签约主体/付款主体不存在
    /// * `ConflictError` - 供应商编号重复或该主体已有供应商角色（唯一索引透出）
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_supplier(
        &self,
        req: CreateSupplierRequest,
        actor: &AuditActor,
    ) -> Result<SupplierView> {
        req.validate()?;
        self.ensure_party_exists(&req.party_id).await?;
        self.ensure_party_exists(&req.signing_entity_party_id).await?;
        self.ensure_party_exists(&req.payment_entity_party_id).await?;

        let profile_revision_id = SupplierCommercialProfileRevisionId::new(next_id());
        let supplier = SupplierAccount::new(
            SupplierAccountId::new(next_id()),
            SupplierAccountData {
                party_id: req.party_id,
                supplier_no: req.supplier_no,
                default_payment_term_id: req.default_payment_term_id,
                current_commercial_profile_revision_id: Some(profile_revision_id.clone()),
                status: req.status.unwrap_or(SupplierAccountStatus::Active),
            },
            actor.id(),
        )?;
        let revision = SupplierCommercialProfileRevision::new(
            profile_revision_id,
            SupplierCommercialProfileRevisionData {
                supplier_id: SupplierAccountId::new(supplier.base.id.clone()),
                revision_no: 1,
                settlement_mode: req.settlement_mode,
                reconciliation_cycle: req.reconciliation_cycle,
                payment_term_snapshot: req.payment_term_snapshot,
                invoice_type: req.invoice_type,
                invoice_tax_rate: req.invoice_tax_rate,
                signing_entity_party_id: req.signing_entity_party_id,
                payment_entity_party_id: req.payment_entity_party_id,
                change_reason: req.change_reason,
            },
        )?;
        let audit = actor
            .clone()
            .resource_log("supplier.create", "supplier", supplier.base.id.clone())?;

        let db = self.db.clone();
        let client = db.client().clone();
        let supplier_for_tx = supplier.clone();
        let revision_for_tx = revision.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier()
                        .create_supplier_with_initial_profile(&supplier_for_tx, &revision_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(supplier.into())
    }

    /// 分页查询供应商角色列表。
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
    pub async fn supplier_list(&self, params: &SupplierListParams) -> Result<PageView<SupplierView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SupplierAccountFilter {
            keyword: query.keyword,
            party_id: query.party_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_accounts()
            .search_supplier_accounts(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierView {
                id: row.id,
                party_id: row.party_id,
                supplier_no: row.supplier_no,
                default_payment_term_id: row.default_payment_term_id,
                current_commercial_profile_revision_id: row.current_commercial_profile_revision_id,
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

    /// 查询供应商角色详情（供应商 + 当前商务结算版本 + 主体编号）。
    ///
    /// # 参数
    /// * `id` - 供应商角色 ID
    ///
    /// # 返回
    /// 返回供应商详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 供应商角色不存在
    pub async fn supplier_detail(&self, id: &str) -> Result<SupplierDetailView> {
        let supplier = self.load_supplier(id).await?;
        let party_no = self.party_no(&supplier.party_id).await;
        let current_profile = match &supplier.current_commercial_profile_revision_id {
            Some(revision_id) => self
                .db
                .supplier_commercial_profile_revisions()
                .find_by_id(revision_id, &mut NoTransaction)
                .await?
                .map(Into::into),
            None => None,
        };
        Ok(SupplierDetailView {
            account: supplier.into(),
            party_no,
            current_profile,
        })
    }

    /// 更新供应商角色（乐观锁；单集合 + 审计）。
    ///
    /// # 参数
    /// * `id` - 供应商角色 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后供应商角色的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 供应商角色不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn update_supplier(
        &self,
        id: &str,
        req: UpdateSupplierRequest,
        actor: &AuditActor,
    ) -> Result<SupplierView> {
        req.validate()?;
        let mut supplier = self.load_supplier(id).await?;
        if supplier.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        supplier.update(
            SupplierAccountUpdate {
                default_payment_term_id: payment_term_update(req.default_payment_term_id),
                current_commercial_profile_revision_id: FieldUpdate::Unchanged,
                status: req.status,
            },
            actor.id(),
        )?;
        let audit = actor
            .clone()
            .resource_log("supplier.update", "supplier", supplier.base.id.clone())?;

        let db = self.db.clone();
        let client = db.client().clone();
        let mut supplier_for_tx = supplier.clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_accounts()
                        .update(&mut supplier_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<SupplierAccount, crate::errors::Error>(supplier_for_tx)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 软删除供应商角色（单集合操作，无事务）。
    ///
    /// 停用/删除角色仍可被历史单据引用（§6.2），不删除主体与版本历史。
    ///
    /// # 参数
    /// * `id` - 供应商角色 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 删除成功返回 `Ok(())`。
    ///
    /// # 错误
    /// * `NotFound` - 供应商角色不存在
    pub async fn delete_supplier(&self, id: &str, actor: &AuditActor) -> Result<()> {
        let mut supplier = self.load_supplier(id).await?;
        let audit = actor
            .clone()
            .resource_log("supplier.delete", "supplier", supplier.base.id.clone())?;
        self.db
            .supplier_accounts()
            .soft_delete(&mut supplier, &mut NoTransaction)
            .await?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
        Ok(())
    }

    /// 分页查询商务结算版本列表（§6.2 历史查询）。
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
    pub async fn commercial_profile_list(
        &self,
        supplier_id: &str,
        params: &CommercialProfileListParams,
    ) -> Result<PageView<CommercialProfileView>> {
        params.validate()?;
        let (sort_by, sort_dir) = dto::normalize_sort(
            &params.sort_by,
            &params.sort_dir,
            dto::COMMERCIAL_PROFILE_SORT_FIELDS,
        )?;
        let filter = CommercialProfileFilter {
            supplier_id: Some(SupplierAccountId::new(supplier_id)),
            page: page_or_default(params.page),
            page_size: page_size_or_default(params.page_size),
            sort_by: Some(sort_by.to_string()),
            sort_ascending: matches!(sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_commercial_profile_revisions()
            .search_commercial_profiles(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| CommercialProfileView {
                id: row.id,
                supplier_id: row.supplier_id.to_string(),
                revision_no: row.revision_no,
                settlement_mode: parse_settlement_mode(&row.settlement_mode),
                reconciliation_cycle: parse_reconciliation_cycle(&row.reconciliation_cycle),
                payment_term_snapshot: row.payment_term_snapshot,
                invoice_type: parse_invoice_type(&row.invoice_type),
                invoice_tax_rate: None,
                signing_entity_party_id: None,
                payment_entity_party_id: None,
                change_reason: row.change_reason,
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

    /// 追加商务结算版本（跨集合事务：新版本 + 推进生效指针 + 审计原子写入）。
    ///
    /// 新版本保存即成为当前商务版本。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建商务版本与更新后供应商的视图对。
    ///
    /// # 错误
    /// * `NotFound` - 供应商或签约/付款主体不存在
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_commercial_profile(
        &self,
        supplier_id: &str,
        req: CreateCommercialProfileRequest,
        actor: &AuditActor,
    ) -> Result<(CommercialProfileView, SupplierView)> {
        req.validate()?;
        self.ensure_party_exists(&req.signing_entity_party_id).await?;
        self.ensure_party_exists(&req.payment_entity_party_id).await?;
        let supplier = self.load_supplier(supplier_id).await?;
        let audit = actor.clone().resource_log(
            "supplier_commercial_profile.create",
            "supplier_commercial_profile",
            supplier.base.id.clone(),
        )?;
        let updated_by = actor.id().to_string();

        // 事务内重读版本历史：计算下一版本号。
        let db = self.db.clone();
        let client = db.client().clone();
        let mut supplier_for_tx = supplier.clone();
        let supplier_id_for_tx = SupplierAccountId::new(supplier_id);
        let revision_for_tx = SupplierCommercialProfileRevision::new(
            SupplierCommercialProfileRevisionId::new(next_id()),
            SupplierCommercialProfileRevisionData {
                supplier_id: supplier_id_for_tx.clone(),
                revision_no: 0,
                settlement_mode: req.settlement_mode,
                reconciliation_cycle: req.reconciliation_cycle,
                payment_term_snapshot: req.payment_term_snapshot,
                invoice_type: req.invoice_type,
                invoice_tax_rate: req.invoice_tax_rate,
                signing_entity_party_id: req.signing_entity_party_id,
                payment_entity_party_id: req.payment_entity_party_id,
                change_reason: req.change_reason,
            },
        )?;
        let (revision, updated) = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let history = db
                        .supplier_commercial_profile_revisions()
                        .list_revision_history(&supplier_id_for_tx, session)
                        .await?;
                    let next_no = history
                        .iter()
                        .map(|revision| revision.revision.revision_no)
                        .max()
                        .unwrap_or(0)
                        + 1;
                    let revision = SupplierCommercialProfileRevision {
                        revision: RevisionBase::new(next_no),
                        ..revision_for_tx
                    };
                    supplier_for_tx.update(
                        SupplierAccountUpdate {
                            default_payment_term_id: FieldUpdate::Unchanged,
                            current_commercial_profile_revision_id: FieldUpdate::Set(
                                SupplierCommercialProfileRevisionId::new(revision.base.id.clone()),
                            ),
                            status: None,
                        },
                        &updated_by,
                    )?;
                    db.supplier_commercial_profile_revisions()
                        .create(&revision, session)
                        .await?;
                    db.supplier_accounts()
                        .update(&mut supplier_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(SupplierCommercialProfileRevision, SupplierAccount), crate::errors::Error>((
                        revision,
                        supplier_for_tx,
                    ))
                })
            })
            .await?;

        Ok((revision.into(), updated.into()))
    }

    /// 按 ID 加载未删除供应商角色。
    ///
    /// # 参数
    /// * `id` - 供应商角色 ID
    ///
    /// # 返回
    /// 返回供应商角色实体。
    ///
    /// # 错误
    /// * `NotFound` - 供应商角色不存在
    pub async fn load_supplier(&self, id: &str) -> Result<SupplierAccount> {
        self.db
            .supplier_accounts()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))
    }

    /// 校验主体存在（D07 跨域读）。
    ///
    /// # 参数
    /// * `party_id` - 主体 ID
    ///
    /// # 返回
    /// 主体存在返回 `Ok(())`。
    ///
    /// # 错误
    /// * `NotFound` - 主体不存在
    pub async fn ensure_party_exists(&self, party_id: &PartyId) -> Result<()> {
        self.db
            .parties()
            .find_by_id(party_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("企业主体不存在，请先创建主体".to_string()))?;
        Ok(())
    }

    /// 查询主体编号（D07 跨域读；缺失时静默降级为 `None`）。
    ///
    /// # 参数
    /// * `party_id` - 主体 ID
    ///
    /// # 返回
    /// 返回主体编号。
    async fn party_no(&self, party_id: &PartyId) -> Option<String> {
        self.db
            .parties()
            .find_by_id(party_id, &mut NoTransaction)
            .await
            .ok()
            .flatten()
            .map(|party| party.party_no)
    }
}

/// 分页默认值辅助（与 `crate::query` 对齐，供子模块复用）。
fn page_or_default(page: Option<u64>) -> u64 {
    page.unwrap_or(1)
}

/// 分页大小默认值辅助（与 `crate::query` 对齐，供子模块复用）。
fn page_size_or_default(page_size: Option<u32>) -> u32 {
    page_size.unwrap_or(20).clamp(1, 100)
}

/// 将可选结算条件入参映射为 `FieldUpdate`：`None` 表示不修改，空字符串表示清除。
///
/// # 参数
/// * `value` - 请求携带的结算条件引用
///
/// # 返回
/// 返回实体更新意图。
fn payment_term_update(value: Option<String>) -> FieldUpdate<String> {
    match value {
        Some(term) if term.trim().is_empty() => FieldUpdate::Clear,
        Some(term) => FieldUpdate::Set(term),
        None => FieldUpdate::Unchanged,
    }
}

/// 从稳定代码解析结算方式（投影行字符串形态 → 类型化枚举）。
///
/// 投影行由本系统 serde 写入，未知代码仅可能来自人工改库；按失败安全策略
/// 回落默认枚举值。
///
/// # 参数
/// * `code` - 稳定代码
///
/// # 返回
/// 返回解析后的枚举值。
fn parse_settlement_mode(code: &str) -> SettlementMode {
    match code {
        "prepayment" => SettlementMode::Prepayment,
        "pay_after_use" => SettlementMode::PayAfterUse,
        "cash_settlement" => SettlementMode::CashSettlement,
        _ => SettlementMode::Prepayment,
    }
}

/// 从稳定代码解析对账周期（投影行字符串形态 → 类型化枚举）。
///
/// # 参数
/// * `code` - 稳定代码
///
/// # 返回
/// 返回解析后的枚举值。
fn parse_reconciliation_cycle(code: &str) -> ReconciliationCycle {
    match code {
        "daily" => ReconciliationCycle::Daily,
        "weekly" => ReconciliationCycle::Weekly,
        "monthly" => ReconciliationCycle::Monthly,
        "quarterly" => ReconciliationCycle::Quarterly,
        "yearly" => ReconciliationCycle::Yearly,
        "none" => ReconciliationCycle::None,
        _ => ReconciliationCycle::None,
    }
}

/// 从稳定代码解析发票类型（投影行字符串形态 → 类型化枚举）。
///
/// # 参数
/// * `code` - 稳定代码
///
/// # 返回
/// 返回解析后的枚举值。
fn parse_invoice_type(code: &str) -> InvoiceType {
    match code {
        "vat_special" => InvoiceType::VatSpecial,
        "vat_normal" => InvoiceType::VatNormal,
        "electronic" => InvoiceType::Electronic,
        _ => InvoiceType::VatSpecial,
    }
}
