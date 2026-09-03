//! 域 D09 `supplier` 服务编排。
//!
//! 供应商资料通过 [`profile::SupplierProfileService`] 根级命令维护；本模块仅
//! 提供列表、完整详情和停用入口，不保留拆分的供应商、商务版本、能力、资质
//! 或评级写接口。

use std::{collections::HashMap, sync::Arc};

use database::{NoTransaction, SupplierExt};
use entities::common::time::BusinessDate;
use entities::supplier::{SupplierAccount, SupplierAccountId, SupplierCommercialProfileRevision};
use mongodb::Database;
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

/// 供应商列表业务查询参数的仓储搜索输入组织（保留在 Service）。
///
/// # 参数
/// * `query` - 已校验的供应商列表业务筛选条件
/// * `as_of` - 当前业务日字符串
///
/// # 返回
/// 返回仓储侧列表事实束搜索输入。
///
/// # 错误
/// 无；仅做枚举与分页映射。
///
/// # 约束
/// Service 拥有查询参数组织，仓储拥有持久化过滤与分页执行。
fn supplier_list_search_input(
    query: &SupplierListQuery,
    as_of: String,
) -> <mongodb::Database as SupplierExt>::SupplierListSearchInput {
    type HealthFilter = <mongodb::Database as SupplierExt>::SupplierQualificationHealthFilter;
    type SearchInput = <mongodb::Database as SupplierExt>::SupplierListSearchInput;
    let qualification_health = match query.qualification_health {
        None => None,
        Some(SupplierQualificationHealth::Valid) => Some(HealthFilter::Valid),
        Some(SupplierQualificationHealth::Expiring30) => Some(HealthFilter::Expiring30),
        Some(SupplierQualificationHealth::Expired) => Some(HealthFilter::Expired),
        Some(SupplierQualificationHealth::NotRegistered) => Some(HealthFilter::NotRegistered),
    };
    SearchInput {
        keyword: query.keyword.clone(),
        party_id: query.party_id.clone(),
        status: query.status,
        capability_codes: query.capability_codes.clone(),
        qualification_types: query.qualification_types.clone(),
        qualification_health,
        as_of,
        page: query.paging.page,
        page_size: query.paging.page_size,
        sort_by: Some(query.paging.sort_by.to_string()),
        sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
    }
}
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
        let as_of = BusinessDate::today().to_string();
        let input = supplier_list_search_input(&query, as_of);
        let bundle = self
            .db
            .supplier()
            .load_supplier_list_bundle(&input, &mut NoTransaction)
            .await?;
        let total = bundle.page.total;
        let items = assemble_supplier_views(
            bundle.page.items,
            bundle.parties,
            bundle.revisions,
            bundle.profiles,
        );

        Ok(PageView {
            items,
            total,
            page: input.page,
            page_size: input.page_size,
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
        let supplier_id = SupplierAccountId::new(id);
        let bundle = self
            .db
            .supplier()
            .load_supplier_detail_bundle(&supplier_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
        let party = bundle
            .party
            .as_ref()
            .ok_or_else(|| Error::NotFound("供应商关联的企业主体不存在".to_string()))?;
        let row = database::repository::SupplierAccountRow {
            id: bundle.supplier.base.id.clone(),
            party_id: bundle.supplier.party_id.to_string(),
            supplier_no: bundle.supplier.supplier_no.clone(),
            default_payment_term_id: bundle.supplier.default_payment_term_id.clone(),
            current_commercial_profile_revision_id: bundle
                .supplier
                .current_commercial_profile_revision_id
                .as_ref()
                .map(ToString::to_string),
            status: bundle.supplier.stable.status,
            version: bundle.supplier.base.version,
            created_at: bundle.supplier.base.created_at,
        };
        let party_for_view = party.clone();
        let revision_for_view = bundle.party_revision.clone();
        let mut account = assemble_supplier_views(
            vec![row],
            bundle.party.into_iter().collect(),
            revision_for_view.into_iter().collect(),
            bundle
                .commercial_profiles
                .iter()
                .find(|profile| {
                    Some(profile.base.id.as_str())
                        == bundle
                            .supplier
                            .current_commercial_profile_revision_id
                            .as_ref()
                            .map(ToString::to_string)
                            .as_deref()
                })
                .cloned()
                .into_iter()
                .collect(),
        )
        .into_iter()
        .next()
        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
        let contacts: Vec<crate::party::PartyContactView> =
            bundle.contacts.into_iter().map(Into::into).collect();
        let addresses: Vec<crate::party::PartyAddressView> =
            bundle.addresses.into_iter().map(Into::into).collect();
        let tax_profiles = bundle.tax_profiles.into_iter().map(Into::into).collect();
        let bank_accounts: Vec<crate::party::PartyBankAccountView> =
            bundle.bank_accounts.into_iter().map(Into::into).collect();
        let capabilities: Vec<SupplierCapabilityView> =
            bundle.capabilities.into_iter().map(Into::into).collect();
        let qualifications = assemble_qualification_views(bundle.qualifications, bundle.qualification_links);
        let ratings = bundle.ratings.into_iter().map(Into::into).collect();
        let mut commercial_profiles: Vec<CommercialProfileView> =
            bundle.commercial_profiles.into_iter().map(Into::into).collect();
        for profile in &mut commercial_profiles {
            profile.signing_entity_name = profile
                .signing_entity_party_id
                .as_ref()
                .and_then(|party_id| bundle.commercial_party_names.get(party_id))
                .cloned();
            profile.payment_entity_name = profile
                .payment_entity_party_id
                .as_ref()
                .and_then(|party_id| bundle.commercial_party_names.get(party_id))
                .cloned();
        }
        if let Some(current_id) = account.current_commercial_profile_revision_id.as_deref() {
            account.current_profile = commercial_profiles
                .iter()
                .find(|profile| profile.id == current_id)
                .cloned();
        }
        let sensitive_fields = self.sensitive_field_views(id, &contacts, &addresses, &bank_accounts)?;
        Ok(SupplierDetailView {
            account,
            party_status: party_for_view.stable.status,
            unified_credit_code: party_for_view.unified_credit_code,
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
        if let Some(contact) = entities::party::select_current_default(
            contacts,
            |item| item.is_default,
            |item| item.status.is_active(),
        ) {
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
        if let Some(address) = entities::party::select_current_default(
            addresses,
            |item| item.is_default,
            |item| item.status.is_active(),
        ) {
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
        if let Some(account) = entities::party::select_current_default(
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
        crate::transaction::run_audited(&self.db, audit, move |db, session| {
            Box::pin(async move {
                db.supplier_accounts().soft_delete(&mut supplier, session).await?;
                Ok(())
            })
        })
        .await?;
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
            .supplier()
            .account(&SupplierAccountId::new(id), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))
    }
}

/// 将详情事实束中的资质与适用能力关联装配为视图（纯 View 映射）。
///
/// # 参数
/// * `qualifications` - 仓储事实束中的资质集合
/// * `links` - 仓储事实束中的适用能力关联集合
///
/// # 返回
/// 返回带适用能力 ID 的资质视图集合。
///
/// # 错误
/// 无。
///
/// # 约束
/// 纯内存映射，不触及 I/O；关联缺失时视为空集合。
fn assemble_qualification_views(
    qualifications: Vec<entities::supplier::SupplierQualification>,
    links: Vec<entities::supplier::SupplierQualificationCapability>,
) -> Vec<SupplierQualificationView> {
    let mut links_by_qualification: HashMap<String, Vec<entities::ids::SupplierCapabilityId>> =
        HashMap::new();
    for link in links {
        links_by_qualification
            .entry(link.qualification_id.to_string())
            .or_default()
            .push(link.capability_id);
    }
    qualifications
        .into_iter()
        .map(|qualification| {
            let id = qualification.base.id.clone();
            let mut view: SupplierQualificationView = qualification.into();
            view.capability_ids = links_by_qualification.remove(&id).unwrap_or_default();
            view
        })
        .collect()
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
