//! 域 D07 `party_tax_profile` 服务编排。
//!
//! 税务资料按「有效期事实追加」维护（§6.2），税号变更不原地修改；
//! 原地更新只允许切换启停状态、结束有效期与调整默认标记；同一主体默认
//! 税务资料唯一（跨行约束，事务内校验，§6.2）。

use database::{AccessControlExt, NoTransaction, PartyExt, Transactional};
use entities::field_update::FieldUpdate;
use entities::party::{
    EffectiveRecordStatus, PartyId, PartyTaxProfile, PartyTaxProfileData, PartyTaxProfileId,
    PartyTaxProfileUpdate,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

use super::dto::{
    normalize_sort, CreatePartyTaxProfileRequest, PageView, PartyTaxProfileListParams, PartyTaxProfileView,
    SortDir, UpdatePartyTaxProfileRequest, PARTY_TAX_PROFILE_SORT_FIELDS,
};
use super::{clear_default_marks, page_or_default, page_size_or_default};

/// 税务资料列表筛选条件类型（经 `PartyExt` 关联类型跨 crate 可达）。
type PartyTaxProfileFilter = <mongodb::Database as PartyExt>::PartyTaxProfileFilter;

/// 税务资料服务。
pub struct PartyTaxProfileService {
    db: Database,
}

impl PartyTaxProfileService {
    /// 创建税务资料服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 分页查询税务资料列表。
    ///
    /// # 参数
    /// * `party_id` - 所属企业主体 ID
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn party_tax_profile_list(
        &self,
        party_id: &str,
        params: &PartyTaxProfileListParams,
    ) -> Result<PageView<PartyTaxProfileView>> {
        params.validate()?;
        let (sort_by, sort_dir) =
            normalize_sort(&params.sort_by, &params.sort_dir, PARTY_TAX_PROFILE_SORT_FIELDS)?;
        let filter = PartyTaxProfileFilter {
            party_id: Some(PartyId::new(party_id)),
            status: params.status,
            is_default: params.is_default,
            page: page_or_default(params.page),
            page_size: page_size_or_default(params.page_size),
            sort_by: Some(sort_by.to_string()),
            sort_ascending: matches!(sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .party_tax_profiles()
            .search_party_tax_profiles(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| PartyTaxProfileView {
                id: row.id,
                party_id: row.party_id.to_string(),
                tax_no: row.tax_no,
                valid_from: row.valid_from,
                valid_to: row.valid_to,
                is_default: row.is_default,
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

    /// 创建税务资料（跨行事务：默认资料唯一 + 新建 + 审计原子写入）。
    ///
    /// # 参数
    /// * `party_id` - 所属企业主体 ID
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建税务资料的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 主体不存在
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_party_tax_profile(
        &self,
        party_id: &str,
        req: CreatePartyTaxProfileRequest,
        actor: &AuditActor,
    ) -> Result<PartyTaxProfileView> {
        req.validate()?;
        self.ensure_party_exists(party_id).await?;
        let profile = PartyTaxProfile::new(
            PartyTaxProfileId::new(next_id()),
            PartyTaxProfileData {
                party_id: PartyId::new(party_id),
                tax_no: req.tax_no,
                valid_from: req.valid_from,
                valid_to: req.valid_to,
                is_default: req.is_default,
                status: req.status.unwrap_or(EffectiveRecordStatus::Active),
            },
            actor.id(),
        )?;
        let audit = actor.clone().resource_log(
            "party_tax_profile.create",
            "party_tax_profile",
            profile.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let profile_for_tx = profile.clone();
        let party_id_for_tx = profile.party_id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    if profile_for_tx.is_default {
                        clear_default_marks!(db, party_tax_profiles, party_id_for_tx, None, session);
                    }
                    db.party_tax_profiles().create(&profile_for_tx, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(profile.into())
    }

    /// 更新税务资料（仅生命周期字段；默认标记跨行事务）。
    ///
    /// # 参数
    /// * `id` - 税务资料 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后税务资料的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 税务资料不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn update_party_tax_profile(
        &self,
        id: &str,
        req: UpdatePartyTaxProfileRequest,
        actor: &AuditActor,
    ) -> Result<PartyTaxProfileView> {
        req.validate()?;
        let mut profile = self
            .db
            .party_tax_profiles()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("税务资料不存在".to_string()))?;
        if profile.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        profile.update(
            PartyTaxProfileUpdate {
                status: req.status,
                valid_to: req.valid_to.map_or(FieldUpdate::Unchanged, FieldUpdate::Set),
                is_default: req.is_default,
            },
            actor.id(),
        )?;
        let audit = actor.clone().resource_log(
            "party_tax_profile.update",
            "party_tax_profile",
            profile.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let mut profile_for_tx = profile.clone();
        let party_id_for_tx = profile.party_id.clone();
        let exclude_id = profile.base.id.clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    if profile_for_tx.is_default {
                        clear_default_marks!(
                            db,
                            party_tax_profiles,
                            party_id_for_tx,
                            Some(&exclude_id),
                            session
                        );
                    }
                    db.party_tax_profiles()
                        .update(&mut profile_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<PartyTaxProfile, crate::errors::Error>(profile_for_tx)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 校验主体存在。
    ///
    /// # 参数
    /// * `party_id` - 主体 ID
    ///
    /// # 返回
    /// 主体存在返回 `Ok(())`。
    ///
    /// # 错误
    /// * `NotFound` - 主体不存在
    async fn ensure_party_exists(&self, party_id: &str) -> Result<()> {
        self.db
            .parties()
            .find_by_id(party_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("主体不存在".to_string()))?;
        Ok(())
    }
}
