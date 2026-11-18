//! 域 D09 `supplier` 服务编排。
//!
//! 供应商资料根命令由 `erp-processes::supplier_profile` 持有事务；本模块仅
//! 提供列表、完整详情和单域读取入口，主体事实经 [`PartyFactsPort`] 组装。

use std::collections::HashMap;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::SupplierCapabilityId;
use erp_identity::AccountDirectoryService;
use erp_permissions::supplier;
use list_view::{SupplierViewAssembleInput, assemble_supplier_views};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::dto::supplier::{
    CommercialProfileView, SupplierCapabilityView, SupplierDetailView, SupplierQualificationView,
    SupplierSensitiveFieldView, SupplierView,
};
use crate::entity::supplier::{SupplierAccountId, SupplierQualification, SupplierQualificationCapability};
use crate::error::{Error, Result};
use crate::ports::{
    PartyAddressFact, PartyBankAccountFact, PartyContactFact, PartyFactsPort, PartyListFact,
    PartyRevisionFact, PartyTaxProfileFact, SensitiveFieldKindFact, SensitiveTokenPort,
    select_current_default,
};
use crate::repository::{SupplierAccountRow, SupplierDetailBundle, SupplierExt};

pub mod access;
pub mod eligibility;
mod handover_identity;
mod list;
mod list_view;
mod profile;
mod scope;

pub use access::SupplierAccess;
pub use handover_identity::{capability_handover_fingerprint, supplier_handover_fingerprint};
pub use profile::command_view;
pub use scope::SupplierListView;

/// 供应商服务。
///
/// 提供供应商列表、完整详情与单域读取；所有资料创建和修订统一走根级资料流程。
#[derive(Clone)]
pub struct SupplierService {
    db: Database,
    party: Arc<dyn PartyFactsPort>,
    sensitive_data: Option<Arc<dyn SensitiveTokenPort>>,
    data_scope: SupplierAccess,
    accounts: AccountDirectoryService,
}

impl SupplierService {
    /// 组合数据库、主体只读事实端口和授权访问器，供供应商列表及详情读取使用；此构造不接入敏感字段揭示令
    /// 牌。
    pub fn new(db: Database, party: Arc<dyn PartyFactsPort>, data_scope: SupplierAccess) -> Self {
        Self {
            db: db.clone(),
            party,
            sensitive_data: None,
            data_scope,
            accounts: AccountDirectoryService::new(db),
        }
    }

    /// 在供应商读取服务上接入短时敏感字段令牌端口，供授权详情展示可按字段揭示的引用；构造不读取业务事实，
    /// 也不签发令牌。
    pub fn with_sensitive_data(
        db: Database,
        party: Arc<dyn PartyFactsPort>,
        sensitive_data: Arc<dyn SensitiveTokenPort>,
        data_scope: SupplierAccess,
    ) -> Self {
        Self {
            db: db.clone(),
            party,
            sensitive_data: Some(sensitive_data),
            data_scope,
            accounts: AccountDirectoryService::new(db),
        }
    }

    /// 复用服务已接入的供应商授权访问器，供调用方按各入口的固定动作核验资格；取得访问器不等于已授权。
    pub fn access(&self) -> SupplierAccess {
        self.data_scope.clone()
    }

    /// 在同一事务内先以供应商详情权限核验对象，再装配当前商务资料、主体、能力、资质与评级；缺供应商或关
    /// 联主体时返回 `NotFound`，授权、读取或签发令牌失败时向上传播错误。
    pub async fn supplier_detail(&self, id: &str, actor: &AuditActor) -> Result<SupplierDetailView> {
        let service = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service.data_scope.require_with(&actor, &supplier::DETAIL, &id, executor).await?;

                    let view = service.load_supplier_detail(&id, executor).await?;
                    Ok(view)
                })
            })
            .await
    }

    // 在调用方已授权的原读取事务内读取并装配供应商详情，不重新解释数据范围。缺供应商或关联主体时返回
    // `NotFound`，其余读库或令牌签发错误向上传播。
    async fn load_supplier_detail(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDetailView> {
        let loaded = self.load_detail_facts(id, executor).await?;
        let account = self.assemble_detail_account(&loaded, executor).await?;
        self.finish_detail_view(loaded, account).await
    }

    // 为供应商详情加载本域事实束、关联企业主体及其联系人、地址、税务、账户和商务主体名称。使用原执行器；
    // 供应商或关联主体缺失时返回 `NotFound`。
    async fn load_detail_facts(&self, id: &str, executor: &mut dyn Executor) -> Result<LoadedSupplierDetail> {
        let supplier_id = SupplierAccountId::new(id);
        let bundle = self
            .db
            .supplier()
            .load_supplier_detail_bundle(&supplier_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;

        let (party, party_revision) = self
            .party
            .find_with_current_revision(&bundle.party_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商关联的企业主体不存在".to_string()))?;

        let contacts = self.party.list_contacts(&bundle.party_id, executor).await?;
        let addresses = self.party.list_addresses(&bundle.party_id, executor).await?;
        let tax_profiles = self.party.list_tax_profiles(&bundle.party_id, executor).await?;
        let bank_accounts = self.party.list_bank_accounts(&bundle.party_id, executor).await?;

        let commercial_party_names =
            self.party.current_legal_names_by_party_ids(&bundle.commercial_party_ids, executor).await?;

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

    // 把详情事实束的账户按列表相同口径投影，并补维护人名称。商务历史由后续详情步骤回填；装配无结果时返回
    // `NotFound`。
    async fn assemble_detail_account(
        &self,
        loaded: &LoadedSupplierDetail,
        executor: &mut dyn Executor,
    ) -> Result<SupplierView> {
        let row = SupplierAccountRow::from_account(&loaded.bundle.supplier);
        let maintainer_names = self
            .accounts
            .names(std::slice::from_ref(&loaded.bundle.supplier.maintainer_user_id), executor)
            .await?;

        assemble_supplier_views(SupplierViewAssembleInput {
            rows: vec![row],
            parties: vec![loaded.party.clone()],
            revisions: loaded.party_revision.clone(),
            profiles: Vec::new(),
            capabilities: loaded.bundle.capabilities.clone(),
            qualifications: loaded.bundle.qualifications.clone(),
            entity_names: HashMap::new(),
            maintainer_names,
            as_of: BusinessDate::today(),
        })
        .into_iter()
        .next()
        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))
    }

    // 完成资质/评级/商务与敏感字段装配（erp-supplier-003）。
    // 返回完整详情视图。
    // 敏感令牌签发失败时返回错误。
    async fn finish_detail_view(
        &self,
        loaded: LoadedSupplierDetail,
        mut account: SupplierView,
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
        for profile in &mut commercial_profiles {
            profile.fill_entity_names(&commercial_party_names);
        }
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
    bundle: SupplierDetailBundle,
    /// 主体列表事实。
    party: PartyListFact,
    /// 主体当前修订。
    party_revision: Vec<PartyRevisionFact>,
    /// 联系人事实行。
    contacts: Vec<PartyContactFact>,
    /// 地址事实行。
    addresses: Vec<PartyAddressFact>,
    /// 税务事实行。
    tax_profiles: Vec<PartyTaxProfileFact>,
    /// 银行账户摘要。
    bank_accounts: Vec<PartyBankAccountFact>,
    /// 商务签约/付款主体名称。
    commercial_party_names: HashMap<String, String>,
}

impl SupplierService {
    // 为当前默认敏感事实签发一分钟有效的字段级令牌。
    fn sensitive_field_views(
        &self,
        supplier_id: &str,
        contacts: &[PartyContactFact],
        addresses: &[PartyAddressFact],
        bank_accounts: &[PartyBankAccountFact],
    ) -> Result<Vec<SupplierSensitiveFieldView>> {
        let Some(codec) = &self.sensitive_data else {
            return Ok(Vec::new());
        };
        let expires_at = u64::try_from(Instant::now().unix_secs())
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
}

// 把详情事实束中的资质与适用能力关联装配为视图（纯 View 映射）。
fn assemble_qualification_views(
    qualifications: Vec<SupplierQualification>,
    links: Vec<SupplierQualificationCapability>,
) -> Vec<SupplierQualificationView> {
    let mut links_by_qualification: HashMap<String, Vec<SupplierCapabilityId>> = HashMap::new();
    for link in links {
        links_by_qualification.entry(link.qualification_id.to_string()).or_default().push(link.capability_id);
    }

    qualifications
        .into_iter()
        .map(|qualification| {
            let mut view: SupplierQualificationView = qualification.into();
            view.capability_ids = links_by_qualification.remove(&view.id).unwrap_or_default();
            view
        })
        .collect()
}

// 组装带短时揭示令牌的敏感字段视图。
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
