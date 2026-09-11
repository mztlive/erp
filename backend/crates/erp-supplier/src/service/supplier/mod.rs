//! 域 D09 `supplier` 服务编排。
//!
//! 供应商资料根命令由 `erp-processes::supplier_profile` 持有事务；本模块仅
//! 提供列表、完整详情和单域读取入口，主体事实经 [`PartyFactsPort`] 组装。

use std::{collections::HashMap, sync::Arc};

use crate::entity::supplier::{SupplierAccount, SupplierAccountId};
use crate::ports::{select_current_default, PartyFactsPort, SensitiveFieldKindFact, SensitiveTokenPort};
use crate::repository::{SupplierAccountRow, SupplierExt};
use erp_core::common::time::BusinessDate;
use list_view::{assemble_supplier_views, SupplierViewAssembleInput};
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::dto::supplier::{
    CommercialProfileView, SupplierCapabilityView, SupplierDetailView, SupplierQualificationView,
    SupplierSensitiveFieldView,
};
use crate::error::{Error, Result};

pub mod eligibility;
mod list;
mod list_view;
mod profile;

pub use profile::command_view;

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
        let current_profiles = bundle
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
            .collect();
        let mut account = assemble_supplier_views(SupplierViewAssembleInput {
            rows: vec![row],
            parties: vec![party],
            revisions: party_revision.into_iter().collect(),
            profiles: current_profiles,
            capabilities: bundle.capabilities.clone(),
            qualifications: bundle.qualifications.clone(),
            entity_names: commercial_party_names.clone(),
            as_of: BusinessDate::today(),
        })
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
