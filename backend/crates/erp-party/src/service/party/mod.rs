//! 域 D07 `party` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 创建主体（party + 首版 party_revision + 审计）→ 跨集合，必须事务；
//! - 更新主体（追加 party_revision + CAS 更新生效指针 + 审计）→
//!   `PartyDomainRepository::append_party_revision` 声明「必须收到事务执行器」；
//! - 软删除主体 / 查询 → 单集合，`&mut NoTransaction`。

pub mod company;

use std::sync::Arc;

use application_core::{AuditActor, normalized_text};
use erp_core::common::revision::RevisionBase;
use erp_core::field_update::FieldUpdate;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use serde::Serialize;
use validator::Validate;

use crate::entity::party::{
    Party, PartyData, PartyId, PartyKind, PartyRevision, PartyRevisionData, PartyRevisionId, PartyStatus,
    PartyUpdate,
};
use crate::error::{Error, Result};
use crate::ports::{PartyAuditPort, SupplierRolePort};
use crate::repository::PartyExt;

pub mod address;
pub mod bank_account;
pub mod contact;
pub mod sensitive;
pub mod tax_profile;

pub use address::PartyAddressService;
pub use bank_account::PartyBankAccountService;
pub use contact::PartyContactService;
pub use sensitive::{SensitiveDataCodec, SensitiveFieldKind, SensitiveRevealScope};
pub use tax_profile::PartyTaxProfileService;

use crate::dto::party::SortDir;
pub use crate::dto::party::{
    CreatePartyAddressRequest, CreatePartyBankAccountRequest, CreatePartyContactRequest, CreatePartyRequest,
    CreatePartyTaxProfileRequest, PageView, PartyAddressListParams, PartyAddressView,
    PartyBankAccountListParams, PartyBankAccountView, PartyContactListParams, PartyContactView,
    PartyListParams, PartyRevisionListParams, PartyRevisionView, PartyTaxProfileListParams,
    PartyTaxProfileView, PartyView, UpdatePartyAddressRequest, UpdatePartyBankAccountRequest,
    UpdatePartyContactRequest, UpdatePartyRequest, UpdatePartyTaxProfileRequest,
};

/// 主体列表筛选条件类型（经 `PartyExt` 关联类型跨 crate 可达）。
type PartyFilter = <mongodb::Database as PartyExt>::PartyFilter;
/// 主体修订列表筛选条件类型。
type PartyRevisionFilter = <mongodb::Database as PartyExt>::PartyRevisionFilter;

/// 主体详情视图：主体 + 当前生效修订快照。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PartyDetailView {
    /// 主体响应视图。
    #[serde(flatten)]
    pub party: PartyView,
    /// 当前生效修订。
    pub current_revision: Option<PartyRevisionView>,
}

/// 主体服务。
///
/// 提供主体与修订链的创建、查询与更新编排（§6.2：稳定主体 + 不可变修订）。
pub struct PartyService {
    db: Database,
    audit: Arc<dyn PartyAuditPort>,
    supplier_roles: Arc<dyn SupplierRolePort>,
}

impl PartyService {
    /// 创建主体服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `audit` - 审计写入端口
    /// * `supplier_roles` - 供应商角色事实端口
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(
        db: Database,
        audit: Arc<dyn PartyAuditPort>,
        supplier_roles: Arc<dyn SupplierRolePort>,
    ) -> Self {
        Self { db, audit, supplier_roles }
    }

    /// 创建主体（跨集合事务：party + 首版 party_revision + 审计原子写入）。
    ///
    /// 同一事务写入 `party_revisions` 与 `parties`（表头携带
    /// `current_revision_id` 指向首版），保证「修订 + 生效指针」原子可见
    /// （数据模型 §6.2）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建主体的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - party_no 或统一社会信用代码与既有主体重复
    pub async fn create_party(&self, req: CreatePartyRequest, actor: &AuditActor) -> Result<PartyView> {
        self.create_party_record(req, None, actor).await
    }

    /// 公司入口复用主体与首版名称事务。
    async fn create_party_record(
        &self,
        req: CreatePartyRequest,
        company: Option<crate::entity::party::company::CompanyProfile>,
        actor: &AuditActor,
    ) -> Result<PartyView> {
        req.validate()?;
        let party_id = PartyId::new(next_id());
        let revision_id = PartyRevisionId::new(next_id());
        let mut party = Party::new(
            party_id.clone(),
            PartyData {
                party_no: req.party_no,
                party_kind: req.party_kind.unwrap_or(PartyKind::Enterprise),
                unified_credit_code: req.unified_credit_code,
                status: req.status.unwrap_or(PartyStatus::Active),
            },
            actor.id(),
        )?;
        party.company_profile = company;
        self.ensure_party_identity_available(
            &party.party_no,
            party.unified_credit_code.as_deref(),
            None,
            &mut NoTransaction,
        )
        .await?;
        let revision = PartyRevision::new(
            revision_id.clone(),
            PartyRevisionData {
                party_id: party_id.clone(),
                revision_no: 1,
                legal_name: req.legal_name,
                short_name: req.short_name,
                change_reason: req.change_reason,
            },
        )?;
        party.stable.current_revision_id = Some(revision_id.to_string());
        let audit = self.audit.resource_log(actor.clone(), "party.create", "party", party_id.to_string())?;

        let db = self.db.clone();
        let client = db.client().clone();
        let audit_port = self.audit.clone();
        let party_for_tx = party.clone();
        let revision_for_tx = revision.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.party_revisions().create(&revision_for_tx, session).await?;
                    db.parties().create(&party_for_tx, session).await?;
                    audit_port.persist(&audit, session).await?;
                    Ok::<(), crate::error::Error>(())
                })
            })
            .await?;

        Ok(party.into())
    }

    /// 分页查询主体列表。
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
    pub async fn party_list(&self, params: &PartyListParams) -> Result<PageView<PartyView>> {
        params.validate()?;
        let query = params.normalized()?;
        let matching_name_ids = match query.keyword.as_deref() {
            Some(keyword) => {
                self.db.party().matching_current_party_ids_by_name(keyword, &mut NoTransaction).await?
            },
            None => Vec::new(),
        };
        let filter = PartyFilter {
            matching_name_ids,
            keyword: query.keyword,
            party_kind: query.party_kind,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self.db.parties().search_parties(&filter, &mut NoTransaction).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| {
                PartyView::from_party_parts(
                    row.id,
                    row.party_no,
                    row.party_kind,
                    row.unified_credit_code,
                    row.status,
                    row.current_revision_id,
                    row.version,
                    row.created_at,
                )
            })
            .collect();

        Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
    }

    /// 查询主体详情（主体 + 当前生效修订快照）。
    ///
    /// # 参数
    /// * `id` - 主体 ID
    ///
    /// # 返回
    /// 返回主体详情视图；当前生效修订缺失时 `current_revision` 为 `None`。
    ///
    /// # 错误
    /// * `NotFound` - 主体不存在
    pub async fn party_detail(&self, id: &str) -> Result<PartyDetailView> {
        let (party, revision) = self
            .db
            .party()
            .find_with_current_revision(&PartyId::new(id), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("主体不存在".to_string()))?;
        let current_revision = revision.and_then(|revision| {
            party.current_revision(std::slice::from_ref(&revision)).ok().cloned().map(Into::into)
        });
        Ok(PartyDetailView { party: party.into(), current_revision })
    }

    /// 更新主体（乐观锁 + 追加修订）。
    ///
    /// 期望版本 `req.version` 与当前版本不一致时直接返回冲突（409）；
    /// 新修订保存即成为当前修订。仓储层 `append_party_revision` 以
    /// `id + version` CAS 兜底并发竞争。
    ///
    /// # 参数
    /// * `id` - 主体 ID
    /// * `req` - 更新请求（含期望版本与新修订快照）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后主体的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 主体不存在
    /// * `ConflictError` - 期望版本与当前版本不一致，或统一社会信用代码冲突
    /// * `ValidationError` - 请求体校验失败
    pub async fn update_party(
        &self,
        id: &str,
        req: UpdatePartyRequest,
        actor: &AuditActor,
    ) -> Result<PartyView> {
        self.update_party_record(id, req, None, actor).await
    }

    /// 公司和名称修订在同一主体事务中更新。
    async fn update_party_record(
        &self,
        id: &str,
        req: UpdatePartyRequest,
        company: Option<crate::entity::party::company::CompanyProfile>,
        actor: &AuditActor,
    ) -> Result<PartyView> {
        req.validate()?;
        let prepared = self.load_party_for_update(id, req, company.as_ref(), actor).await?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "party.update",
            "party",
            prepared.party_id().to_string(),
        )?;
        let updated_by = actor.id().to_string();

        // 下一修订号必须在写事务快照内读取，避免并发复用序号。
        let db = self.db.clone();
        let client = db.client().clone();
        let audit_port = self.audit.clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let next_no = db
                        .party_revisions()
                        .next_revision_no(&PartyId::new(prepared.party_id().to_string()), session)
                        .await?;
                    let (mut party, revision_for_tx, party_update) = prepared.into_parts();
                    let revision = PartyRevision { revision: RevisionBase::new(next_no), ..revision_for_tx };
                    party.update(party_update, &updated_by)?;
                    db.party().append_party_revision(&mut party, &revision, &updated_by, session).await?;
                    audit_port.persist(&audit, session).await?;
                    Ok::<Party, crate::error::Error>(party)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 加载主体并完成事务外的全部守卫与预校验（erp-party-004）。
    ///
    /// 公司分支守卫、供应商资料边界、乐观锁版本校验与信用代码预冲突查询
    /// 均在此完成；事务内只保留修订号分配、实体更新与追加写入。
    /// 校验顺序与冲突语义保持不变。
    ///
    /// # 参数
    /// * `id` - 主体 ID
    /// * `req` - 更新请求（含期望版本与新修订快照）
    /// * `company` - 公司资料分支；`Some` 表示公司入口
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回待更新主体与序号占位的修订草稿（修订号由事务内回填）。
    ///
    /// # 错误
    /// * `NotFound` - 主体不存在
    /// * `ConflictError` - 期望版本不一致或信用代码冲突
    /// * `ValidationError` - 请求体校验失败
    async fn load_party_for_update(
        &self,
        id: &str,
        req: UpdatePartyRequest,
        company: Option<&crate::entity::party::company::CompanyProfile>,
        actor: &AuditActor,
    ) -> Result<PendingPartyUpdate> {
        let mut party = self.load_party(id).await?;
        if party.company_profile.is_some() && company.is_none() {
            return Err(Error::BusinessLogicError("请在公司主体维护中修改公司资料".into()));
        }
        if let Some(company) = company {
            party.company_profile = Some(company.clone());
        }
        ensure_outside_supplier_profile(self.supplier_roles.as_ref(), &PartyId::new(id)).await?;
        map_version_conflict(party.ensure_version(req.version))?;

        // 预校验信用代码冲突：与实体规范化规则一致，避免仅依赖唯一索引透出笼统冲突。
        if let Some(raw_code) = req.unified_credit_code.as_ref() {
            let mut probe = party.clone();
            probe.update(
                PartyUpdate {
                    unified_credit_code: FieldUpdate::from_optional_text(Some(raw_code.clone())),
                    status: None,
                },
                actor.id(),
            )?;
            self.ensure_party_identity_available(
                &probe.party_no,
                probe.unified_credit_code.as_deref(),
                Some(party.base.id.as_str()),
                &mut NoTransaction,
            )
            .await?;
        }

        let revision_for_tx = PartyRevision::new(
            PartyRevisionId::new(next_id()),
            PartyRevisionData {
                party_id: PartyId::new(party.base.id.clone()),
                revision_no: 0,
                legal_name: req.legal_name,
                short_name: req.short_name,
                change_reason: req.change_reason,
            },
        )?;
        let pending = PendingPartyUpdate {
            party,
            revision: revision_for_tx,
            update: PartyUpdate {
                unified_credit_code: FieldUpdate::from_optional_text(req.unified_credit_code),
                status: req.status,
            },
        };
        Ok(pending)
    }

    /// 分页查询主体修订列表。
    ///
    /// # 参数
    /// * `party_id` - 稳定主体 ID
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn party_revision_list(
        &self,
        party_id: &str,
        params: &PartyRevisionListParams,
    ) -> Result<PageView<PartyRevisionView>> {
        params.validate()?;
        let paging = crate::dto::party::normalize_paging(
            params.page,
            params.page_size,
            &params.sort_by,
            &params.sort_dir,
            crate::dto::party::PARTY_REVISION_SORT_FIELDS,
        )?;
        let filter = PartyRevisionFilter {
            party_id: Some(PartyId::new(party_id)),
            legal_name: normalized_text(params.legal_name.as_deref()),
            short_name: normalized_text(params.short_name.as_deref()),
            page: paging.page,
            page_size: paging.page_size,
            sort_by: Some(paging.sort_by.to_string()),
            sort_ascending: matches!(paging.sort_dir, SortDir::Asc),
        };
        let page = self.db.party_revisions().search_party_revisions(&filter, &mut NoTransaction).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| {
                PartyRevisionView::from_revision_parts(
                    row.id,
                    row.revision_no,
                    row.legal_name,
                    row.short_name,
                    row.change_reason,
                    row.version,
                    row.created_at,
                )
            })
            .collect();

        Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
    }

    /// 按 ID 加载未删除主体。
    ///
    /// # 参数
    /// * `id` - 主体 ID
    ///
    /// # 返回
    /// 返回主体实体。
    ///
    /// # 错误
    /// * `NotFound` - 主体不存在
    pub async fn load_party(&self, id: &str) -> Result<Party> {
        self.db
            .parties()
            .find_party(&PartyId::new(id), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("主体不存在".to_string()))
    }

    /// 确保主体编号与统一社会信用代码未被占用。
    ///
    /// 全局唯一索引包含软删除记录，因此必须按「含已删除」查询占用状态；
    /// 并发竞争仍由唯一索引兜底，并映射为字段级冲突提示。
    ///
    /// # 参数
    /// * `party_no` - 已规范化的主体编号
    /// * `unified_credit_code` - 已规范化的统一社会信用代码；`None` 表示不校验
    /// * `exclude_party_id` - 更新场景下排除自身 ID
    /// * `executor` - 数据访问执行器，由调用方决定是否位于事务中
    ///
    /// # 返回
    /// 身份可用时返回 `Ok(())`。
    ///
    /// # 错误
    /// * `ConflictError` - 主体编号或统一社会信用代码已被占用
    async fn ensure_party_identity_available(
        &self,
        party_no: &str,
        unified_credit_code: Option<&str>,
        exclude_party_id: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if let Some(existing) =
            self.db.parties().find_by_party_no_including_deleted(party_no, executor).await?
            && !exclude_party_id.is_some_and(|id| existing.base.id == id)
        {
            return Err(Error::ConflictError(format!("主体编号「{party_no}」已存在")));
        }

        let Some(credit_code) = unified_credit_code else {
            return Ok(());
        };
        if let Some(existing) =
            self.db.parties().find_by_unified_credit_code_including_deleted(credit_code, executor).await?
            && !exclude_party_id.is_some_and(|id| existing.base.id == id)
        {
            return Err(Error::ConflictError(format!("统一社会信用代码「{credit_code}」已存在")));
        }

        Ok(())
    }
}

/// 待更新主体与事务内应用的更新意图（erp-party-004）。
///
/// `load_party_for_update` 在事务外完成全部守卫后，把实体更新意图与修订草稿
/// 暂存于此；事务闭包内解包应用，避免闭包 `move` 大量请求状态。
struct PendingPartyUpdate {
    party: Party,
    revision: PartyRevision,
    update: PartyUpdate,
}

impl PendingPartyUpdate {
    /// 返回待更新主体的稳定 ID。
    ///
    /// # 参数
    /// * `self` - 待更新主体包装
    ///
    /// # 返回
    /// 返回主体稳定 ID 引用。
    fn party_id(&self) -> &str {
        &self.party.base.id
    }

    /// 解包为事务内应用的三元组。
    ///
    /// # 参数
    /// * `self` - 待更新主体包装
    ///
    /// # 返回
    /// 返回 `(主体, 修订草稿, 更新意图)`。
    fn into_parts(self) -> (Party, PartyRevision, PartyUpdate) {
        (self.party, self.revision, self.update)
    }
}

/// 拒绝通过共享 Party 子资源接口访问已挂供应商角色的主体。
///
/// 供应商的主体、联系人、地址、税务等事实必须由供应商资料根级命令统一维护，
/// 以确保 Party 与 Supplier 双版本及其子事实位于同一事务边界。
pub async fn ensure_outside_supplier_profile(
    supplier_roles: &dyn SupplierRolePort,
    party_id: &PartyId,
) -> Result<()> {
    let has_supplier_role = supplier_roles.party_has_supplier_role(party_id).await?;
    ensure_supplier_profile_boundary(has_supplier_role)
}

/// 校验从属事实所属主体存在（erp-party-001）。
///
/// 四个从属事实服务（联系人/地址/税务资料/银行账户）创建入口的同一守卫；
/// 存在性读取使用非事务执行器，与调用方后续事务边界无关。
///
/// # 参数
/// * `db` - 数据库实例
/// * `party_id` - 主体 ID
///
/// # 返回
/// 主体存在返回 `Ok(())`。
///
/// # 错误
/// * `NotFound` - 主体不存在
pub(crate) async fn ensure_party_exists(db: &Database, party_id: &str) -> Result<()> {
    db.parties()
        .find_by_id(party_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("主体不存在".to_string()))?;
    Ok(())
}

/// 校验从属事实创建入口的同一守卫组合（erp-party-011）。
///
/// 主体存在性与供应商资料边界按创建路径固定顺序执行；与分开调用两守卫语义一致。
///
/// # 参数
/// * `db` - 数据库实例
/// * `supplier_roles` - 供应商角色事实端口
/// * `party_id` - 主体 ID
///
/// # 返回
/// 两守卫均通过返回 `Ok(())`。
///
/// # 错误
/// * `NotFound` - 主体不存在
/// * `BusinessLogicError` - 主体已挂供应商角色
pub(crate) async fn ensure_new_fact_guards(
    db: &Database,
    supplier_roles: &dyn SupplierRolePort,
    party_id: &str,
) -> Result<()> {
    ensure_party_exists(db, party_id).await?;
    ensure_outside_supplier_profile(supplier_roles, &PartyId::new(party_id)).await
}

/// 将实体乐观锁冲突映射为稳定的业务冲突错误（erp-party-011）。
///
/// 各从属事实与主体更新入口的 `ensure_version` 映射唯一来源；冲突文案由实体层保证。
///
/// # 参数
/// * `version_check` - 实体 `ensure_version` 的返回结果
///
/// # 返回
/// 版本一致返回 `Ok(())`。
///
/// # 错误
/// * `ConflictError` - 期望版本与当前版本不一致
pub(crate) fn map_version_conflict(version_check: erp_core::Result<()>) -> Result<()> {
    version_check.map_err(|error| Error::ConflictError(error.to_string()))
}

/// 将仓储查询结果转换为稳定的供应商资料边界错误。
fn ensure_supplier_profile_boundary(has_supplier_role: bool) -> Result<()> {
    if has_supplier_role {
        return Err(Error::BusinessLogicError("供应商主体资料只能通过供应商资料根级接口维护".to_string()));
    }
    Ok(())
}

/// 清除同一主体其他行的默认标记（跨行约束，§6.2）。
///
/// 适用于联系人/地址/税务资料/银行账户的「同一主体同一时点最多一个默认
/// 有效行」约束：加载 `is_default = true` 的既有行，除 `exclude_id` 外逐行
/// 清除默认标记并 CAS 更新。**必须收到事务执行器**：与主写入组成同一
/// 原子边界，传入 `NoTransaction` 时中途失败会留下多个默认行。
///
/// `$accessor` 必须是 `PartyExt` 的集合访问器方法名（`party_contacts`/
/// `party_addresses`/`party_tax_profiles`/`party_bank_accounts`），行实体
/// 必须带 `is_default: bool` 字段与公开 `base` 元数据。
macro_rules! clear_default_marks {
    ($db:expr, $accessor:ident, $party_id:expr, $exclude:expr, $executor:expr) => {{
        let exclude: Option<&::std::string::String> = $exclude;
        $db.$accessor()
            .clear_other_default_marks(&$party_id, exclude.map(|id| id.as_str()), $executor)
            .await?;
    }};
}
pub(crate) use clear_default_marks;

#[cfg(test)]
mod tests {
    use super::{ensure_supplier_profile_boundary, map_version_conflict};

    #[test]
    fn supplier_party_rejects_shared_party_subresource_access() {
        assert!(ensure_supplier_profile_boundary(true).is_err());
        assert!(ensure_supplier_profile_boundary(false).is_ok());
    }

    #[test]
    fn version_conflict_maps_to_stable_conflict_error() {
        assert!(map_version_conflict(Ok(())).is_ok());
        let error =
            map_version_conflict(Err(erp_core::Error::from("数据已被其他请求修改，请刷新后重试")))
                .unwrap_err();
        assert!(matches!(error, crate::error::Error::ConflictError(_)));
        assert_eq!(error.to_string(), "数据冲突: 数据已被其他请求修改，请刷新后重试");
    }
}
