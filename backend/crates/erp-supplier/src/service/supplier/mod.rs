//! 域 D09 `supplier` 服务编排。
//!
//! 供应商资料根命令由 `erp-processes::supplier_profile` 持有事务；本模块仅
//! 提供列表、完整详情和单域读取入口，主体事实经 [`PartyFactsPort`] 组装。

use std::collections::HashMap;
use std::sync::Arc;

use erp_core::common::time::BusinessDate;
use list_view::{SupplierViewAssembleInput, assemble_supplier_views};
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::dto::supplier::{
    CommercialProfileView, SupplierCapabilityView, SupplierDetailView, SupplierQualificationView,
    SupplierSensitiveFieldView,
};
use crate::entity::supplier::{SupplierAccount, SupplierAccountId};
use crate::error::{Error, Result};
use crate::ports::{
    AccountFactPort, FailClosedAccountFactPort, FailClosedSupplierDataScopePort, PartyFactsPort,
    SensitiveFieldKindFact, SensitiveTokenPort, SupplierDataScopePort, select_current_default,
};
use crate::repository::{SupplierAccountRow, SupplierExt};

pub mod access;
pub mod eligibility;
mod handover_identity;
mod list;
mod list_view;
mod profile;
mod scope;

pub use access::{SupplierAccess, supplier_scope};
pub use handover_identity::{
    capability_handover_audit_id, capability_handover_fingerprint, supplier_handover_audit_id,
    supplier_handover_audit_message, supplier_handover_fingerprint, supplier_handover_fingerprint_matches,
};
pub use profile::command_view;
pub use scope::SupplierListView;

/// 供应商服务。
///
/// 提供供应商列表、完整详情与单域读取；所有资料创建和修订统一走根级资料流程。
pub struct SupplierService {
    db: Database,
    party: Arc<dyn PartyFactsPort>,
    sensitive_data: Option<Arc<dyn SensitiveTokenPort>>,
    data_scope: Arc<dyn SupplierDataScopePort>,
    accounts: Arc<dyn AccountFactPort>,
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
            data_scope: FailClosedSupplierDataScopePort::shared(),
            accounts: Arc::new(FailClosedAccountFactPort),
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
            data_scope: FailClosedSupplierDataScopePort::shared(),
            accounts: Arc::new(FailClosedAccountFactPort),
        }
    }

    /// 注入范围 Port 与账号事实。
    ///
    /// # 参数
    /// * `data_scope` - 已接线供应商范围 Port
    /// * `accounts` - 账号显示名与登录资格
    ///
    /// # 返回
    /// 返回可解析范围的服务。
    ///
    /// # 错误
    /// 无。
    pub fn with_scope(
        mut self,
        data_scope: Arc<dyn SupplierDataScopePort>,
        accounts: Arc<dyn AccountFactPort>,
    ) -> Self {
        self.data_scope = data_scope;
        self.accounts = accounts;
        self
    }

    /// 构造供应商对象访问器。
    pub fn access(&self) -> SupplierAccess {
        SupplierAccess::new(self.db.clone(), self.data_scope.clone())
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
    pub async fn supplier_detail(
        &self,
        id: &str,
        actor: &application_core::AuditActor,
    ) -> Result<SupplierDetailView> {
        let expected = self.access().require(actor, "detail", id).await?;
        let view = self.load_supplier_detail(id).await?;
        let current = self.access().require(actor, "detail", id).await?;
        scope::ensure_stable_snapshot(&expected.scope_version, &current.scope_version)?;
        Ok(view)
    }

    /// 装载详情展示字段，不解释数据范围。
    pub async fn load_supplier_detail(&self, id: &str) -> Result<SupplierDetailView> {
        let loaded = self.load_detail_facts(id).await?;
        let current_profiles = current_profile_subset(&loaded.bundle);
        let account = self.assemble_detail_account(&loaded, current_profiles).await?;
        self.finish_detail_view(loaded, account).await
    }

    /// 一次性装载详情所需的事实束与主体事实（erp-supplier-003）。
    ///
    /// # 参数
    /// * `id` - 供应商角色 ID
    ///
    /// # 返回
    /// 返回未经装配的详情事实束。
    ///
    /// # 错误
    /// 供应商或关联主体缺失时返回 `NotFound`。
    async fn load_detail_facts(&self, id: &str) -> Result<LoadedSupplierDetail> {
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
        let contacts = self.party.list_contacts(&bundle.party_id, &mut NoTransaction).await?;
        let addresses = self.party.list_addresses(&bundle.party_id, &mut NoTransaction).await?;
        let tax_profiles = self.party.list_tax_profiles(&bundle.party_id, &mut NoTransaction).await?;
        let bank_accounts = self.party.list_bank_accounts(&bundle.party_id, &mut NoTransaction).await?;
        let commercial_party_names = self
            .party
            .current_legal_names_by_party_ids(&bundle.commercial_party_ids, &mut NoTransaction)
            .await?;
        Ok(LoadedSupplierDetail {
            id: id.to_string(),
            bundle,
            party,
            party_revision: party_revision.into_iter().collect(),
            contacts,
            addresses,
            tax_profiles,
            bank_accounts,
            commercial_party_names,
        })
    }

    /// 装配详情账户视图（erp-supplier-003）。
    ///
    /// # 参数
    /// * `loaded` - 已装载的详情事实
    /// * `current_profiles` - 当前商务版本子集
    ///
    /// # 返回
    /// 返回已水合主体与商务名称的账户视图。
    ///
    /// # 错误
    /// 装配结果为空时返回 `NotFound`。
    async fn assemble_detail_account(
        &self,
        loaded: &LoadedSupplierDetail,
        current_profiles: Vec<crate::entity::supplier::SupplierCommercialProfileRevision>,
    ) -> Result<crate::dto::supplier::SupplierView> {
        let row = account_row_from(&loaded.bundle.supplier);
        let maintainer_names = self
            .accounts
            .names_by_ids(std::slice::from_ref(&loaded.bundle.supplier.maintainer_user_id))
            .await?;
        assemble_supplier_views(SupplierViewAssembleInput {
            rows: vec![row],
            parties: vec![loaded.party.clone()],
            revisions: loaded.party_revision.clone(),
            profiles: current_profiles,
            capabilities: loaded.bundle.capabilities.clone(),
            qualifications: loaded.bundle.qualifications.clone(),
            entity_names: loaded.commercial_party_names.clone(),
            maintainer_names,
            as_of: BusinessDate::today(),
        })
        .into_iter()
        .next()
        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))
    }

    /// 完成资质/评级/商务与敏感字段装配（erp-supplier-003）。
    ///
    /// # 参数
    /// * `loaded` - 已装载的详情事实
    /// * `account` - 已装配的账户视图
    ///
    /// # 返回
    /// 返回完整详情视图。
    ///
    /// # 错误
    /// 敏感令牌签发失败时返回错误。
    async fn finish_detail_view(
        &self,
        loaded: LoadedSupplierDetail,
        mut account: crate::dto::supplier::SupplierView,
    ) -> Result<SupplierDetailView> {
        let LoadedSupplierDetail {
            id,
            bundle,
            party,
            contacts,
            addresses,
            tax_profiles,
            bank_accounts,
            commercial_party_names,
            ..
        } = loaded;
        let capabilities: Vec<SupplierCapabilityView> =
            bundle.capabilities.into_iter().map(Into::into).collect();
        let qualifications = assemble_qualification_views(bundle.qualifications, bundle.qualification_links);
        let ratings = bundle.ratings.into_iter().map(Into::into).collect();
        let mut commercial_profiles: Vec<CommercialProfileView> =
            bundle.commercial_profiles.into_iter().map(Into::into).collect();
        fill_commercial_names(&mut commercial_profiles, &commercial_party_names);
        if let Some(current_id) = account.current_commercial_profile_revision_id.as_deref() {
            account.current_profile =
                commercial_profiles.iter().find(|profile| profile.id == current_id).cloned();
        }
        let sensitive_fields = self.sensitive_field_views(&id, &contacts, &addresses, &bank_accounts)?;
        Ok(SupplierDetailView {
            account,
            party_status: party.status,
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
}

/// 详情装载的事实束（erp-supplier-003）。
///
/// `load_detail_facts` 一次性装载，后续装配只引用或移动其字段；
///
/// # 参数
/// 各字段均为装载时的快照，不再发起 I/O。
struct LoadedSupplierDetail {
    /// 供应商角色 ID。
    id: String,
    /// 仓储返回的详情事实束。
    bundle: crate::repository::SupplierDetailBundle,
    /// 主体列表事实。
    party: crate::ports::PartyListFact,
    /// 主体当前修订。
    party_revision: Vec<crate::ports::PartyRevisionFact>,
    /// 联系人事实行。
    contacts: Vec<crate::ports::PartyContactFact>,
    /// 地址事实行。
    addresses: Vec<crate::ports::PartyAddressFact>,
    /// 税务事实行。
    tax_profiles: Vec<crate::ports::PartyTaxProfileFact>,
    /// 银行账户摘要。
    bank_accounts: Vec<crate::ports::PartyBankAccountFact>,
    /// 商务签约/付款主体名称。
    commercial_party_names: HashMap<String, String>,
}

/// 由供应商实体构造投影行（erp-supplier-003）。
///
/// 生产装配一律从实体取值，避免整束克隆；字段与原内联构造一致。
///
/// # 参数
/// * `supplier` - 供应商角色实体
///
/// # 返回
/// 返回列表投影行。
fn account_row_from(supplier: &SupplierAccount) -> SupplierAccountRow {
    SupplierAccountRow {
        id: supplier.base.id.clone(),
        party_id: supplier.party_id.to_string(),
        supplier_no: supplier.supplier_no.clone(),
        maintainer_user_id: supplier.maintainer_user_id.clone(),
        business_org_unit_id: supplier.business_org_unit_id.clone(),
        default_payment_term_id: supplier.default_payment_term_id.clone(),
        current_commercial_profile_revision_id: supplier
            .current_commercial_profile_revision_id
            .as_ref()
            .map(ToString::to_string),
        status: supplier.stable.status,
        version: supplier.base.version,
        created_at: supplier.base.created_at,
    }
}

/// 取出当前商务版本子集（erp-supplier-003）。
///
/// 只克隆命中的单个版本，避免整束 `commercial_profiles` 克隆。
///
/// # 参数
/// * `bundle` - 详情事实束
///
/// # 返回
/// 返回当前版本（命中时单个元素），未命中时为空。
fn current_profile_subset(
    bundle: &crate::repository::SupplierDetailBundle,
) -> Vec<crate::entity::supplier::SupplierCommercialProfileRevision> {
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
        .collect()
}

/// 回填商务版本的签约/付款主体名称（erp-supplier-003）。
///
/// # 参数
/// * `profiles` - 已映射的商务视图
/// * `names` - 主体名称映射
fn fill_commercial_names(profiles: &mut [CommercialProfileView], names: &HashMap<String, String>) {
    for profile in profiles {
        profile.fill_entity_names(names);
    }
}

impl SupplierService {
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
        if let Some(account) =
            select_current_default(bank_accounts, |item| item.is_default, |item| item.status.is_active())
        {
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
        links_by_qualification.entry(link.qualification_id.to_string()).or_default().push(link.capability_id);
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
