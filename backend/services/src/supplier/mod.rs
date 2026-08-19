//! 域 D09 `supplier` 服务编排。
//!
//! 供应商资料通过 [`profile::SupplierProfileService`] 根级命令维护；本模块仅
//! 提供列表、完整详情和停用入口，不保留拆分的供应商、商务版本、能力、资质
//! 或评级写接口。

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use chrono::Days;
use database::{AccessControlExt, NoTransaction, PartyExt, SupplierExt};
use entities::supplier::{SupplierAccount, SupplierAccountId, SupplierCommercialProfileRevision};
use entities::{common::time::BusinessDate, ids::PartyId};
use mongodb::{bson::doc, Database};
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::party::{SensitiveDataCodec, SensitiveFieldKind};

mod dto;
pub(crate) mod eligibility;
pub mod profile;

pub use self::dto::{
    CommercialProfileView, PageView, RevealSupplierSensitiveRequest, SaveSupplierProfileRequest,
    SupplierCapabilityView, SupplierDetailView, SupplierListParams, SupplierProfileAddressInput,
    SupplierProfileBankAccountInput, SupplierProfileContactInput, SupplierProfileMutationView,
    SupplierProfileQualificationInput, SupplierProfileRatingInput, SupplierQualificationHealth,
    SupplierQualificationView, SupplierRatingView, SupplierSensitiveFieldView, SupplierSensitiveRevealView,
    SupplierView,
};

use self::dto::{SortDir, SupplierListQuery};

/// 供应商角色列表筛选条件类型（经 `SupplierExt` 关联类型跨 crate 可达）。
type SupplierAccountFilter = <mongodb::Database as SupplierExt>::SupplierAccountFilter;
/// 供应商服务。
///
/// 提供供应商列表、完整详情与停用；所有资料创建和修订统一走根级资料服务。
pub struct SupplierService {
    db: Database,
    sensitive_data: Option<Arc<SensitiveDataCodec>>,
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
        Self {
            db,
            sensitive_data: None,
        }
    }

    /// 创建可签发敏感字段短时揭示令牌的详情查询服务。
    pub fn with_sensitive_data(db: Database, sensitive_data: Arc<SensitiveDataCodec>) -> Self {
        Self {
            db,
            sensitive_data: Some(sensitive_data),
        }
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
        let party_ids = match query.keyword.as_deref() {
            Some(keyword) => Some(self.matching_party_ids(keyword).await?),
            None => None,
        };
        let (supplier_ids, excluded_supplier_ids) = self.supplier_id_constraints(&query).await?;
        let filter = SupplierAccountFilter {
            keyword: query.keyword,
            party_id: query.party_id,
            party_ids,
            status: query.status,
            supplier_ids,
            excluded_supplier_ids,
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
        let items = self.hydrate_supplier_rows(page.items).await?;

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 组装供应商能力和资质条件对应的角色 ID 约束。
    async fn supplier_id_constraints(
        &self,
        query: &SupplierListQuery,
    ) -> Result<(Option<Vec<SupplierAccountId>>, Option<Vec<SupplierAccountId>>)> {
        let as_of = BusinessDate::today().to_string();
        let capability_ids = self.matching_capability_supplier_ids(query, &as_of).await?;
        let (qualification_ids, excluded_qualification_ids) =
            self.matching_qualification_supplier_ids(query, &as_of).await?;
        Ok((
            intersect_supplier_ids(capability_ids, qualification_ids),
            excluded_qualification_ids,
        ))
    }

    /// 查询命中任一当前有效供应能力的供应商角色 ID。
    async fn matching_capability_supplier_ids(
        &self,
        query: &SupplierListQuery,
        as_of: &str,
    ) -> Result<Option<Vec<SupplierAccountId>>> {
        if query.capability_codes.is_empty() {
            return Ok(None);
        }
        let ids = self
            .db
            .supplier_capabilities()
            .find_supplier_ids_by_active_capability_codes(&query.capability_codes, as_of, &mut NoTransaction)
            .await?;
        Ok(Some(ids))
    }

    /// 查询资质类型和资料状态对应的供应商角色 ID 约束。
    async fn matching_qualification_supplier_ids(
        &self,
        query: &SupplierListQuery,
        as_of: &str,
    ) -> Result<(Option<Vec<SupplierAccountId>>, Option<Vec<SupplierAccountId>>)> {
        if query.qualification_types.is_empty() && query.qualification_health.is_none() {
            return Ok((None, None));
        }
        let repository = self.db.supplier_qualifications();
        let included = match query.qualification_health {
            None => Some(
                repository
                    .find_supplier_ids_by_qualification_types(&query.qualification_types, &mut NoTransaction)
                    .await?,
            ),
            Some(SupplierQualificationHealth::Valid) => Some(
                repository
                    .find_supplier_ids_by_valid_qualifications(
                        &query.qualification_types,
                        as_of,
                        &mut NoTransaction,
                    )
                    .await?,
            ),
            Some(SupplierQualificationHealth::Expiring30) => {
                let expires_by = qualification_expiry_cutoff(as_of)?;
                Some(
                    repository
                        .find_supplier_ids_by_expiring_qualifications(
                            &query.qualification_types,
                            as_of,
                            &expires_by,
                            &mut NoTransaction,
                        )
                        .await?,
                )
            }
            Some(SupplierQualificationHealth::Expired) => Some(
                repository
                    .find_supplier_ids_by_expired_qualifications(
                        &query.qualification_types,
                        as_of,
                        &mut NoTransaction,
                    )
                    .await?,
            ),
            Some(SupplierQualificationHealth::NotRegistered) => None,
        };
        let excluded = if matches!(
            query.qualification_health,
            Some(SupplierQualificationHealth::NotRegistered)
        ) {
            Some(
                repository
                    .find_supplier_ids_by_qualification_types(&query.qualification_types, &mut NoTransaction)
                    .await?,
            )
        } else {
            None
        };
        Ok((included, excluded))
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
        let party = self
            .db
            .parties()
            .find_by_id(&supplier.party_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商关联的企业主体不存在".to_string()))?;
        let row = database::repository::SupplierAccountRow {
            id: supplier.base.id,
            party_id: supplier.party_id.to_string(),
            supplier_no: supplier.supplier_no,
            default_payment_term_id: supplier.default_payment_term_id,
            current_commercial_profile_revision_id: supplier
                .current_commercial_profile_revision_id
                .map(|id| id.to_string()),
            status: supplier.stable.status,
            version: supplier.base.version,
            created_at: supplier.base.created_at,
        };
        let mut account = self
            .hydrate_supplier_rows(vec![row])
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
        let party_id = supplier.party_id.to_string();
        let supplier_id = SupplierAccountId::new(id);
        let contacts: Vec<crate::party::PartyContactView> = self
            .db
            .party_contacts()
            .find_many_sorted(
                doc! { "party_id": &party_id },
                doc! { "is_default": -1, "created_at": -1 },
                &mut NoTransaction,
            )
            .await?
            .into_iter()
            .map(Into::into)
            .collect();
        let addresses: Vec<crate::party::PartyAddressView> = self
            .db
            .party_addresses()
            .find_many_sorted(
                doc! { "party_id": &party_id },
                doc! { "is_default": -1, "created_at": -1 },
                &mut NoTransaction,
            )
            .await?
            .into_iter()
            .map(Into::into)
            .collect();
        let tax_profiles = self
            .db
            .party_tax_profiles()
            .find_many_sorted(
                doc! { "party_id": &party_id },
                doc! { "is_default": -1, "created_at": -1 },
                &mut NoTransaction,
            )
            .await?
            .into_iter()
            .map(Into::into)
            .collect();
        let bank_accounts: Vec<crate::party::PartyBankAccountView> = self
            .db
            .party_bank_accounts()
            .find_many_sorted(
                doc! { "party_id": &party_id },
                doc! { "is_default": -1, "created_at": -1 },
                &mut NoTransaction,
            )
            .await?
            .into_iter()
            .map(Into::into)
            .collect();
        let capabilities: Vec<SupplierCapabilityView> = self
            .db
            .supplier_capabilities()
            .find_many_sorted(
                doc! { "supplier_id": id },
                doc! { "created_at": 1 },
                &mut NoTransaction,
            )
            .await?
            .into_iter()
            .map(Into::into)
            .collect();
        let qualifications = self.supplier_qualification_views(&supplier_id).await?;
        let ratings = self
            .db
            .supplier_rating_revisions()
            .find_many_sorted(
                doc! { "supplier_id": id },
                doc! { "revision_no": -1 },
                &mut NoTransaction,
            )
            .await?
            .into_iter()
            .map(Into::into)
            .collect();
        let mut commercial_profiles: Vec<CommercialProfileView> = self
            .db
            .supplier_commercial_profile_revisions()
            .find_many_sorted(
                doc! { "supplier_id": id },
                doc! { "revision_no": -1 },
                &mut NoTransaction,
            )
            .await?
            .into_iter()
            .map(Into::into)
            .collect();
        self.hydrate_commercial_party_names(&mut commercial_profiles)
            .await?;
        if let Some(current_id) = account.current_commercial_profile_revision_id.as_deref() {
            account.current_profile = commercial_profiles
                .iter()
                .find(|profile| profile.id == current_id)
                .cloned();
        }
        let sensitive_fields = self.sensitive_field_views(id, &contacts, &addresses, &bank_accounts)?;
        Ok(SupplierDetailView {
            account,
            party_status: party.stable.status,
            unified_credit_code: party.unified_credit_code,
            contacts,
            addresses,
            tax_profiles,
            bank_accounts,
            capabilities,
            qualifications,
            ratings,
            commercial_profiles,
            sensitive_fields,
        })
    }

    /// 为当前默认敏感事实签发一分钟有效的字段级令牌。
    fn sensitive_field_views(
        &self,
        supplier_id: &str,
        contacts: &[crate::party::PartyContactView],
        addresses: &[crate::party::PartyAddressView],
        bank_accounts: &[crate::party::PartyBankAccountView],
    ) -> Result<Vec<SupplierSensitiveFieldView>> {
        let Some(codec) = &self.sensitive_data else {
            return Ok(Vec::new());
        };
        let expires_at = u64::try_from(entities::common::time::Instant::now().unix_secs())
            .map_err(|_| Error::Internal("系统时间非法".to_string()))?
            + 60;
        let mut fields = Vec::new();
        if let Some(contact) =
            current_default(contacts, |item| item.is_default, |item| item.status.is_active())
        {
            fields.push(sensitive_field(
                codec,
                SensitiveFieldKind::ContactMobile,
                &contact.id,
                supplier_id,
                "联系电话",
                &contact.mobile_masked,
                expires_at,
            )?);
        }
        if let Some(address) =
            current_default(addresses, |item| item.is_default, |item| item.status.is_active())
        {
            fields.push(sensitive_field(
                codec,
                SensitiveFieldKind::Address,
                &address.id,
                supplier_id,
                "经营地址",
                "********",
                expires_at,
            )?);
        }
        if let Some(account) = current_default(
            bank_accounts,
            |item| item.is_default,
            |item| item.status.is_active(),
        ) {
            fields.push(sensitive_field(
                codec,
                SensitiveFieldKind::BankAccountNumber,
                &account.id,
                supplier_id,
                "银行账号",
                &account.account_number_masked,
                expires_at,
            )?);
        }
        Ok(fields)
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

    /// 查找法定名称或简称命中的主体 ID，用于与供应商编号组成统一关键词搜索。
    async fn matching_party_ids(&self, keyword: &str) -> Result<Vec<PartyId>> {
        let escaped = regex::escape(keyword);
        let revisions = self
            .db
            .party_revisions()
            .find_many(
                doc! {
                    "$or": [
                        { "legal_name": { "$regex": &escaped, "$options": "i" } },
                        { "short_name": { "$regex": &escaped, "$options": "i" } },
                    ]
                },
                &mut NoTransaction,
            )
            .await?;
        let mut ids: Vec<PartyId> = revisions.into_iter().map(|revision| revision.party_id).collect();
        ids.sort_by_key(ToString::to_string);
        ids.dedup();
        Ok(ids)
    }

    /// 批量补齐当前主体名称与商务资料，固定为三次批量读取，避免逐供应商 N+1。
    async fn hydrate_supplier_rows(
        &self,
        rows: Vec<database::repository::SupplierAccountRow>,
    ) -> Result<Vec<SupplierView>> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let party_ids: Vec<String> = rows.iter().map(|row| row.party_id.clone()).collect();
        let parties = self
            .db
            .parties()
            .find_many(doc! { "id": { "$in": party_ids } }, &mut NoTransaction)
            .await?;
        let revision_ids: Vec<String> = parties
            .iter()
            .filter_map(|party| party.stable.current_revision_id.clone())
            .collect();
        let revisions = self
            .db
            .party_revisions()
            .find_many(doc! { "id": { "$in": revision_ids } }, &mut NoTransaction)
            .await?;
        let profile_ids: Vec<String> = rows
            .iter()
            .filter_map(|row| row.current_commercial_profile_revision_id.clone())
            .collect();
        let profiles = self
            .db
            .supplier_commercial_profile_revisions()
            .find_many(doc! { "id": { "$in": profile_ids } }, &mut NoTransaction)
            .await?;
        Ok(assemble_supplier_views(rows, parties, revisions, profiles))
    }

    /// 批量装配供应商资质与适用能力，避免每份资质单独查询关联集合。
    async fn supplier_qualification_views(
        &self,
        supplier_id: &SupplierAccountId,
    ) -> Result<Vec<SupplierQualificationView>> {
        let qualifications = self
            .db
            .supplier_qualifications()
            .find_many_sorted(
                doc! { "supplier_id": supplier_id.to_string() },
                doc! { "created_at": -1 },
                &mut NoTransaction,
            )
            .await?;
        let qualification_ids: Vec<entities::ids::SupplierQualificationId> = qualifications
            .iter()
            .map(|qualification| entities::ids::SupplierQualificationId::new(&qualification.base.id))
            .collect();
        let links = self
            .db
            .supplier_qualification_capabilities()
            .list_by_qualification_ids(&qualification_ids, &mut NoTransaction)
            .await?;
        let mut links_by_qualification: HashMap<String, Vec<entities::ids::SupplierCapabilityId>> =
            HashMap::new();
        for link in links {
            links_by_qualification
                .entry(link.qualification_id.to_string())
                .or_default()
                .push(link.capability_id);
        }
        Ok(qualifications
            .into_iter()
            .map(|qualification| {
                let id = qualification.base.id.clone();
                let mut view: SupplierQualificationView = qualification.into();
                view.capability_ids = links_by_qualification.remove(&id).unwrap_or_default();
                view
            })
            .collect())
    }

    /// 批量补齐商务版本引用的签约与付款主体当前名称。
    async fn hydrate_commercial_party_names(&self, profiles: &mut [CommercialProfileView]) -> Result<()> {
        let party_ids: Vec<String> = profiles
            .iter()
            .flat_map(|profile| {
                [
                    profile.signing_entity_party_id.clone(),
                    profile.payment_entity_party_id.clone(),
                ]
            })
            .flatten()
            .collect();
        let parties = self
            .db
            .parties()
            .find_many(doc! { "id": { "$in": party_ids } }, &mut NoTransaction)
            .await?;
        let revision_ids: Vec<String> = parties
            .iter()
            .filter_map(|party| party.stable.current_revision_id.clone())
            .collect();
        let revisions = self
            .db
            .party_revisions()
            .find_many(doc! { "id": { "$in": revision_ids } }, &mut NoTransaction)
            .await?;
        let revisions: HashMap<String, entities::party::PartyRevision> = revisions
            .into_iter()
            .map(|revision| (revision.base.id.clone(), revision))
            .collect();
        let names: HashMap<String, String> = parties
            .into_iter()
            .filter_map(|party| {
                let revision_id = party.stable.current_revision_id?;
                let name = revisions.get(&revision_id)?.legal_name.clone();
                Some((party.base.id, name))
            })
            .collect();
        for profile in profiles {
            profile.signing_entity_name = profile
                .signing_entity_party_id
                .as_ref()
                .and_then(|id| names.get(id))
                .cloned();
            profile.payment_entity_name = profile
                .payment_entity_party_id
                .as_ref()
                .and_then(|id| names.get(id))
                .cloned();
        }
        Ok(())
    }
}

/// 合并两个供应商角色候选集合；两个条件同时存在时取交集。
fn intersect_supplier_ids(
    current: Option<Vec<SupplierAccountId>>,
    matched: Option<Vec<SupplierAccountId>>,
) -> Option<Vec<SupplierAccountId>> {
    let (current, matched) = match (current, matched) {
        (Some(current), Some(matched)) => (current, matched),
        (Some(current), None) => return Some(current),
        (None, Some(matched)) => return Some(matched),
        (None, None) => return None,
    };
    let matched: HashSet<String> = matched.into_iter().map(|id| id.to_string()).collect();
    Some(
        current
            .into_iter()
            .filter(|id| matched.contains(&id.to_string()))
            .collect(),
    )
}

/// 计算“30 天内到期”筛选窗口的结束业务日。
fn qualification_expiry_cutoff(as_of: &str) -> Result<String> {
    let as_of = as_of.parse::<BusinessDate>()?;
    as_of
        .as_naive_date()
        .checked_add_days(Days::new(30))
        .map(|date| date.to_string())
        .ok_or_else(|| Error::Internal("无法计算资质到期筛选窗口".to_string()))
}

/// 将批量读取结果按稳定 ID 装配为供应商列表/详情统一视图。
fn assemble_supplier_views(
    rows: Vec<database::repository::SupplierAccountRow>,
    parties: Vec<entities::party::Party>,
    revisions: Vec<entities::party::PartyRevision>,
    profiles: Vec<SupplierCommercialProfileRevision>,
) -> Vec<SupplierView> {
    let parties: HashMap<String, entities::party::Party> = parties
        .into_iter()
        .map(|party| (party.base.id.clone(), party))
        .collect();
    let revisions: HashMap<String, entities::party::PartyRevision> = revisions
        .into_iter()
        .map(|revision| (revision.base.id.clone(), revision))
        .collect();
    let profiles: HashMap<String, SupplierCommercialProfileRevision> = profiles
        .into_iter()
        .map(|profile| (profile.base.id.clone(), profile))
        .collect();

    rows.into_iter()
        .map(|row| {
            let party = parties.get(&row.party_id);
            let revision = party
                .and_then(|party| party.stable.current_revision_id.as_ref())
                .and_then(|id| revisions.get(id));
            let current_profile = row
                .current_commercial_profile_revision_id
                .as_ref()
                .and_then(|id| profiles.get(id))
                .cloned()
                .map(Into::into);
            SupplierView {
                id: row.id,
                party_id: row.party_id,
                party_no: party.map(|party| party.party_no.clone()),
                legal_name: revision.map(|revision| revision.legal_name.clone()),
                short_name: revision.and_then(|revision| revision.short_name.clone()),
                party_version: party.map(|party| party.base.version),
                supplier_no: row.supplier_no,
                default_payment_term_id: row.default_payment_term_id,
                current_commercial_profile_revision_id: row.current_commercial_profile_revision_id,
                status: row.status,
                version: row.version,
                created_at: row.created_at,
                current_profile,
            }
        })
        .collect()
}

/// 从列表中优先选择启用的默认事实，否则选择首个启用事实。
fn current_default<T>(
    items: &[T],
    is_default: impl Fn(&T) -> bool,
    is_active: impl Fn(&T) -> bool,
) -> Option<&T> {
    items
        .iter()
        .find(|item| is_default(item) && is_active(item))
        .or_else(|| items.iter().find(|item| is_active(item)))
}

/// 构造带短时揭示令牌的敏感字段视图。
fn sensitive_field(
    codec: &SensitiveDataCodec,
    kind: SensitiveFieldKind,
    record_id: &str,
    supplier_id: &str,
    label: &str,
    masked_value: &str,
    expires_at: u64,
) -> Result<SupplierSensitiveFieldView> {
    Ok(SupplierSensitiveFieldView {
        label: label.to_string(),
        masked_value: masked_value.to_string(),
        reveal_token: codec.issue_reveal_token(kind, record_id, supplier_id, expires_at)?,
        expires_at,
    })
}
