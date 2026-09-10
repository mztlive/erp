//! 域 D09 `supplier` 服务编排。
//!
//! 供应商资料根命令由 `erp-processes::supplier_profile` 持有事务；本模块仅
//! 提供列表、完整详情和单域读取入口，主体事实经 [`PartyFactsPort`] 组装。

use std::{collections::HashMap, sync::Arc};

use crate::entity::supplier::{SupplierAccount, SupplierAccountId, SupplierCommercialProfileRevision};
use crate::ports::{
    select_current_default, PartyFactsPort, PartyListFact, PartyRevisionFact, SensitiveFieldKindFact,
    SensitiveTokenPort,
};
use crate::repository::{SupplierAccountRow, SupplierExt, SupplierListSearchInput};
use erp_core::common::time::BusinessDate;
use mongodb::Database;
use persistence_core::NoTransaction;
use validator::Validate;

use crate::dto::supplier::{
    CommercialProfileView, PageView, SortDir, SupplierCapabilityView, SupplierDetailView, SupplierListParams,
    SupplierListQuery, SupplierQualificationHealth, SupplierQualificationView, SupplierSensitiveFieldView,
    SupplierView,
};
use crate::error::{Error, Result};

pub mod eligibility;
mod profile;

pub use profile::command_view;

/// 供应商列表业务查询参数的仓储搜索输入组织（保留在 Service）。
///
/// # 参数
/// * `query` - 已校验的供应商列表业务筛选条件
/// * `as_of` - 当前业务日字符串
/// * `keyword_party_ids` - 关键词命中的主体 ID，由 PartyFactsPort 预先解析
///
/// # 返回
/// 返回仓储侧列表事实束搜索输入。
fn supplier_list_search_input(
    query: &SupplierListQuery,
    as_of: String,
    keyword_party_ids: Option<Vec<erp_core::ids::PartyId>>,
) -> SupplierListSearchInput {
    type HealthFilter = crate::repository::SupplierQualificationHealthFilter;
    let qualification_health = match query.qualification_health {
        None => None,
        Some(SupplierQualificationHealth::Unverified) => Some(HealthFilter::Unverified),
        Some(SupplierQualificationHealth::Valid) => Some(HealthFilter::Valid),
        Some(SupplierQualificationHealth::Expiring30) => Some(HealthFilter::Expiring30),
        Some(SupplierQualificationHealth::Expired) => Some(HealthFilter::Expired),
        Some(SupplierQualificationHealth::NotRegistered) => Some(HealthFilter::NotRegistered),
    };
    SupplierListSearchInput {
        keyword: query.keyword.clone(),
        party_id: query.party_id.clone(),
        keyword_party_ids,
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
/// 提供供应商列表、完整详情与单域读取；所有资料创建和修订统一走根级资料流程。
pub struct SupplierService {
    db: Database,
    party: Arc<dyn PartyFactsPort>,
    sensitive_data: Option<Arc<dyn SensitiveTokenPort>>,
}

impl SupplierService {
    /// 创建供应商服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `party` - 主体只读事实端口
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database, party: Arc<dyn PartyFactsPort>) -> Self {
        Self {
            db,
            party,
            sensitive_data: None,
        }
    }

    /// 创建可签发敏感字段短时揭示令牌的详情查询服务。
    pub fn with_sensitive_data(
        db: Database,
        party: Arc<dyn PartyFactsPort>,
        sensitive_data: Arc<dyn SensitiveTokenPort>,
    ) -> Self {
        Self {
            db,
            party,
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
        let keyword_party_ids = match query.keyword.as_deref() {
            Some(keyword) => Some(
                self.party
                    .matching_current_party_ids_by_name(keyword, &mut NoTransaction)
                    .await?,
            ),
            None => None,
        };
        let input = supplier_list_search_input(&query, as_of, keyword_party_ids);
        let bundle = self
            .db
            .supplier()
            .load_supplier_list_bundle(&input, &mut NoTransaction)
            .await?;
        let total = bundle.page.total;
        let (parties, revisions) = self
            .party
            .list_with_current_revisions(&bundle.party_ids, &mut NoTransaction)
            .await?;
        let items = assemble_supplier_views(bundle.page.items, parties, revisions, bundle.profiles);

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
        let (party, party_revision) = self
            .party
            .find_with_current_revision(&bundle.party_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商关联的企业主体不存在".to_string()))?;
        let contacts = self
            .party
            .list_contacts(&bundle.party_id, &mut NoTransaction)
            .await?;
        let addresses = self
            .party
            .list_addresses(&bundle.party_id, &mut NoTransaction)
            .await?;
        let tax_profiles = self
            .party
            .list_tax_profiles(&bundle.party_id, &mut NoTransaction)
            .await?;
        let bank_accounts = self
            .party
            .list_bank_accounts(&bundle.party_id, &mut NoTransaction)
            .await?;
        let commercial_party_names = self
            .party
            .current_legal_names_by_party_ids(&bundle.commercial_party_ids, &mut NoTransaction)
            .await?;
        let row = SupplierAccountRow {
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
        let mut account = assemble_supplier_views(
            vec![row],
            vec![party],
            party_revision.into_iter().collect(),
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
                .and_then(|party_id| commercial_party_names.get(party_id))
                .cloned();
            profile.payment_entity_name = profile
                .payment_entity_party_id
                .as_ref()
                .and_then(|party_id| commercial_party_names.get(party_id))
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
            party_status: party_for_view.status,
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
        contacts: &[crate::ports::PartyContactFact],
        addresses: &[crate::ports::PartyAddressFact],
        bank_accounts: &[crate::ports::PartyBankAccountFact],
    ) -> Result<Vec<SupplierSensitiveFieldView>> {
        let Some(codec) = &self.sensitive_data else {
            return Ok(Vec::new());
        };
        let expires_at = u64::try_from(erp_core::common::time::Instant::now().unix_secs())
            .map_err(|_| Error::Internal("系统时间非法".to_string()))?
            + 60;
        let mut fields = Vec::new();
        if let Some(contact) =
            select_current_default(contacts, |item| item.is_default, |item| item.status.is_active())
        {
            fields.push(sensitive_field(
                codec.as_ref(),
                SensitiveFieldKindFact::ContactMobile,
                &contact.id,
                supplier_id,
                "联系电话",
                &contact.mobile_masked,
                expires_at,
            )?);
        }
        if let Some(address) =
            select_current_default(addresses, |item| item.is_default, |item| item.status.is_active())
        {
            fields.push(sensitive_field(
                codec.as_ref(),
                SensitiveFieldKindFact::Address,
                &address.id,
                supplier_id,
                "经营地址",
                "********",
                expires_at,
            )?);
        }
        if let Some(account) = select_current_default(
            bank_accounts,
            |item| item.is_default,
            |item| item.status.is_active(),
        ) {
            fields.push(sensitive_field(
                codec.as_ref(),
                SensitiveFieldKindFact::BankAccountNumber,
                &account.id,
                supplier_id,
                "银行账号",
                &account.account_number_masked,
                expires_at,
            )?);
        }
        Ok(fields)
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
fn assemble_qualification_views(
    qualifications: Vec<crate::entity::supplier::SupplierQualification>,
    links: Vec<crate::entity::supplier::SupplierQualificationCapability>,
) -> Vec<SupplierQualificationView> {
    let mut links_by_qualification: HashMap<String, Vec<erp_core::ids::SupplierCapabilityId>> =
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
    rows: Vec<SupplierAccountRow>,
    parties: Vec<PartyListFact>,
    revisions: Vec<PartyRevisionFact>,
    profiles: Vec<SupplierCommercialProfileRevision>,
) -> Vec<SupplierView> {
    let parties: HashMap<String, PartyListFact> = parties
        .into_iter()
        .map(|party| (party.id.clone(), party))
        .collect();
    let revisions: HashMap<String, PartyRevisionFact> = revisions
        .into_iter()
        .map(|revision| (revision.id.clone(), revision))
        .collect();
    let profiles: HashMap<String, SupplierCommercialProfileRevision> = profiles
        .into_iter()
        .map(|profile| (profile.base.id.clone(), profile))
        .collect();

    rows.into_iter()
        .map(|row| {
            let party = parties.get(&row.party_id);
            let revision = party
                .and_then(|party| party.current_revision_id.as_ref())
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
                party_version: party.map(|party| party.version),
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
    codec: &dyn SensitiveTokenPort,
    kind: SensitiveFieldKindFact,
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
