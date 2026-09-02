//! 域 D07 `party` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 创建主体（party + 首版 party_revision + 审计）→ 跨集合，必须事务；
//! - 更新主体（追加 party_revision + CAS 更新生效指针 + 审计）→
//!   `PartyRepository::append_party_revision` 声明「必须收到事务执行器」；
//! - 软删除主体 / 查询 → 单集合，`&mut NoTransaction`。

use database::{AccessControlExt, Executor, NoTransaction, PartyExt, SupplierExt, Transactional};
use entities::common::revision::RevisionBase;
use entities::field_update::FieldUpdate;
use entities::party::{
    Party, PartyData, PartyId, PartyKind, PartyRevision, PartyRevisionData, PartyRevisionId, PartyStatus,
    PartyUpdate,
};
use id_generator::next_id;
use mongodb::Database;
use serde::Serialize;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

pub mod address;
pub mod bank_account;
pub mod contact;
mod dto;
pub mod sensitive;
pub mod tax_profile;

pub use sensitive::{SensitiveDataCodec, SensitiveFieldKind, SensitiveRevealScope};

pub use self::dto::{
    CreatePartyAddressRequest, CreatePartyBankAccountRequest, CreatePartyContactRequest, CreatePartyRequest,
    CreatePartyTaxProfileRequest, PageView, PartyAddressListParams, PartyAddressView,
    PartyBankAccountListParams, PartyBankAccountView, PartyContactListParams, PartyContactView,
    PartyListParams, PartyRevisionListParams, PartyRevisionView, PartyTaxProfileListParams,
    PartyTaxProfileView, PartyView, UpdatePartyAddressRequest, UpdatePartyBankAccountRequest,
    UpdatePartyContactRequest, UpdatePartyRequest, UpdatePartyTaxProfileRequest,
};

use self::dto::SortDir;

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
}

impl PartyService {
    /// 创建主体服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
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
        let audit = actor
            .clone()
            .resource_log("party.create", "party", party_id.to_string())?;

        let db = self.db.clone();
        let client = db.client().clone();
        let party_for_tx = party.clone();
        let revision_for_tx = revision.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.party_revisions().create(&revision_for_tx, session).await?;
                    db.parties().create(&party_for_tx, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
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
        let filter = PartyFilter {
            keyword: query.keyword,
            party_kind: query.party_kind,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .parties()
            .search_parties(&filter, &mut NoTransaction)
            .await?;
        // 投影行类型属于仓储私有子树（`repository/mod.rs` 冻结，无法命名），
        // 此处按字段映射为响应视图，避免把仓储类型泄漏到接口层。
        let items = page
            .items
            .into_iter()
            .map(|row| PartyView {
                id: row.id,
                party_no: row.party_no,
                party_kind: row.party_kind,
                unified_credit_code: row.unified_credit_code,
                status: row.status,
                current_revision_id: row.current_revision_id,
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
            party
                .current_revision(std::slice::from_ref(&revision))
                .ok()
                .cloned()
                .map(Into::into)
        });
        Ok(PartyDetailView {
            party: party.into(),
            current_revision,
        })
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
        req.validate()?;
        let party = self.load_party(id).await?;
        ensure_outside_supplier_profile(&self.db, &PartyId::new(id)).await?;
        party
            .ensure_version(req.version)
            .map_err(|error| Error::ConflictError(error.to_string()))?;

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

        let audit = actor
            .clone()
            .resource_log("party.update", "party", party.base.id.clone())?;
        let updated_by = actor.id().to_string();

        // 下一修订号必须在写事务快照内读取，避免并发复用序号。
        let db = self.db.clone();
        let client = db.client().clone();
        let mut party_for_tx = party.clone();
        let revision_for_tx = PartyRevision::new(
            PartyRevisionId::new(next_id()),
            PartyRevisionData {
                party_id: PartyId::new(party_for_tx.base.id.clone()),
                revision_no: 0,
                legal_name: req.legal_name,
                short_name: req.short_name,
                change_reason: req.change_reason,
            },
        )?;
        let unified_credit_code = req.unified_credit_code;
        let status = req.status;
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let next_no = db
                        .party_revisions()
                        .next_revision_no(&PartyId::new(party_for_tx.base.id.clone()), session)
                        .await?;
                    let revision = PartyRevision {
                        revision: RevisionBase::new(next_no),
                        ..revision_for_tx
                    };
                    party_for_tx.update(
                        PartyUpdate {
                            unified_credit_code: FieldUpdate::from_optional_text(unified_credit_code),
                            status,
                        },
                        &updated_by,
                    )?;
                    db.party()
                        .append_party_revision(&mut party_for_tx, &revision, &updated_by, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<Party, crate::errors::Error>(party_for_tx)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 软删除主体（单集合操作，无事务）。
    ///
    /// 只标记 `parties.deleted_at`，历史修订与已引用单据不受影响
    /// （§4.5.3：基础资料以停用/删除表示退出业务，不物理删除）。
    ///
    /// # 参数
    /// * `id` - 主体 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 删除成功返回 `Ok(())`。
    ///
    /// # 错误
    /// * `NotFound` - 主体不存在
    /// * `ConflictError` - 主体已被删除（版本冲突透出）
    pub async fn delete_party(&self, id: &str, actor: &AuditActor) -> Result<()> {
        let mut party = self.load_party(id).await?;
        ensure_outside_supplier_profile(&self.db, &PartyId::new(id)).await?;
        let audit = actor
            .clone()
            .resource_log("party.delete", "party", party.base.id.clone())?;
        crate::transaction::run_audited(&self.db, audit, move |db, session| {
            Box::pin(async move {
                db.parties().soft_delete(&mut party, session).await?;
                Ok(())
            })
        })
        .await?;
        Ok(())
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
        let (sort_by, sort_dir) =
            dto::normalize_sort(&params.sort_by, &params.sort_dir, dto::PARTY_REVISION_SORT_FIELDS)?;
        let filter = PartyRevisionFilter {
            party_id: Some(PartyId::new(party_id)),
            legal_name: normalized_text(params.legal_name.as_deref()),
            short_name: normalized_text(params.short_name.as_deref()),
            page: page_or_default(params.page),
            page_size: page_size_or_default(params.page_size),
            sort_by: Some(sort_by.to_string()),
            sort_ascending: matches!(sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .party_revisions()
            .search_party_revisions(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| PartyRevisionView {
                id: row.id,
                revision_no: row.revision_no,
                legal_name: row.legal_name,
                short_name: row.short_name,
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
    async fn load_party(&self, id: &str) -> Result<Party> {
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
        if let Some(existing) = self
            .db
            .parties()
            .find_by_party_no_including_deleted(party_no, executor)
            .await?
        {
            if !exclude_party_id.is_some_and(|id| existing.base.id == id) {
                return Err(Error::ConflictError(format!("主体编号「{party_no}」已存在")));
            }
        }

        let Some(credit_code) = unified_credit_code else {
            return Ok(());
        };
        if let Some(existing) = self
            .db
            .parties()
            .find_by_unified_credit_code_including_deleted(credit_code, executor)
            .await?
        {
            if !exclude_party_id.is_some_and(|id| existing.base.id == id) {
                return Err(Error::ConflictError(format!(
                    "统一社会信用代码「{credit_code}」已存在"
                )));
            }
        }

        Ok(())
    }
}

/// 拒绝通过共享 Party 子资源接口访问已挂供应商角色的主体。
///
/// 供应商的主体、联系人、地址、税务等事实必须由供应商资料根级命令统一维护，
/// 以确保 Party 与 Supplier 双版本及其子事实位于同一事务边界。
pub(super) async fn ensure_outside_supplier_profile(db: &Database, party_id: &PartyId) -> Result<()> {
    let has_supplier_role = db
        .supplier_accounts()
        .find_by_party(party_id, &mut NoTransaction)
        .await?
        .is_some();
    ensure_supplier_profile_boundary(has_supplier_role)
}

/// 将仓储查询结果转换为稳定的供应商资料边界错误。
fn ensure_supplier_profile_boundary(has_supplier_role: bool) -> Result<()> {
    if has_supplier_role {
        return Err(Error::BusinessLogicError(
            "供应商主体资料只能通过供应商资料根级接口维护".to_string(),
        ));
    }
    Ok(())
}

/// 分页默认值辅助（与 `crate::query` 对齐，供子模块复用）。
fn page_or_default(page: Option<u64>) -> u64 {
    page.unwrap_or(1)
}

/// 分页大小默认值辅助（与 `crate::query` 对齐，供子模块复用）。
fn page_size_or_default(page_size: Option<u32>) -> u32 {
    page_size.unwrap_or(20).clamp(1, 100)
}

/// 文本归一化（与 `crate::query` 对齐，供子模块复用）。
fn normalized_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
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
    use super::ensure_supplier_profile_boundary;

    #[test]
    fn supplier_party_rejects_shared_party_subresource_access() {
        assert!(ensure_supplier_profile_boundary(true).is_err());
        assert!(ensure_supplier_profile_boundary(false).is_ok());
    }
}
