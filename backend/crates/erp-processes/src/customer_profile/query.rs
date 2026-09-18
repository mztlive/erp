//! 客户资料详情查询与视图映射。

use erp_core::common::time::BusinessDate;
use erp_core::ids::PartyId;
use erp_customer::repository::prelude::*;
use erp_customer::{
    AssignmentRole, CustomerAccount, CustomerAccountId, CustomerAccountStatus, CustomerActionBlockerView,
    CustomerAssignment, CustomerAssignmentView, CustomerExt, CustomerProfileDetailView,
    CustomerSensitiveFieldView, CustomerView,
};
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_party::repository::prelude::*;
use erp_party::{Party, PartyAddress, PartyBankAccount, PartyContact, PartyExt, PartyRevision};
use persistence_core::NoTransaction;

use super::CustomerProfileService;
use crate::{Error, Result};

impl CustomerProfileService {
    /// 查询客户资料对象中心的当前事实、历史版本与敏感字段揭示入口。
    ///
    /// # Errors
    /// 客户、Party 或当前名称修订不存在，或任一仓储查询失败时返回错误。
    pub async fn detail(&self, customer_id: &str) -> Result<CustomerProfileDetailView> {
        let account = self.load_customer(customer_id).await?;
        let party = self.load_party(&account.party_id).await?;
        let revisions =
            self.db.party_revisions().list_revision_history(&account.party_id, &mut NoTransaction).await?;
        let current_revision =
            party.current_revision(&revisions).map_err(|error| Error::Internal(error.to_string()))?.clone();
        let assignments = self
            .db
            .customer_assignments()
            .list_history_for_customer(&CustomerAccountId::new(customer_id), &mut NoTransaction)
            .await?;
        let account_ids: Vec<String> =
            assignments.iter().map(|assignment| assignment.user_id.clone()).collect();
        let account_names = self.db.accounts().names_by_ids(&account_ids, &mut NoTransaction).await?;
        let (contacts, addresses, tax_profiles, bank_accounts) = self
            .db
            .party()
            .load_current_facts(&account.party_id, BusinessDate::today(), &mut NoTransaction)
            .await?;
        let sensitive_fields = self.sensitive_fields(customer_id, &contacts, &addresses, &bank_accounts)?;
        let mut detail = build_detail(ProfileDetailParts {
            account,
            party,
            current_revision,
            revisions,
            assignments,
            contacts,
            addresses,
            tax_profiles,
            bank_accounts,
            sensitive_fields,
        });
        for assignment in &mut detail.assignments {
            assignment.user_name =
                account_names.get(&assignment.user_id).cloned().unwrap_or_else(|| assignment.user_id.clone());
        }
        Ok(detail)
    }

    /// 加载客户角色。
    pub(super) async fn load_customer(&self, id: &str) -> Result<CustomerAccount> {
        self.db
            .customer_accounts()
            .find_customer(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户不存在".to_string()))
    }

    /// 加载客户关联 Party。
    pub(super) async fn load_party(&self, party_id: &PartyId) -> Result<Party> {
        self.db
            .parties()
            .find_party(party_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户关联主体不存在".to_string()))
    }
}

/// 客户资料完整详情视图的已加载事实。
struct ProfileDetailParts {
    account: CustomerAccount,
    party: Party,
    current_revision: PartyRevision,
    revisions: Vec<PartyRevision>,
    assignments: Vec<CustomerAssignment>,
    contacts: Vec<PartyContact>,
    addresses: Vec<PartyAddress>,
    tax_profiles: Vec<erp_party::PartyTaxProfile>,
    bank_accounts: Vec<PartyBankAccount>,
    sensitive_fields: Vec<CustomerSensitiveFieldView>,
}

/// 构造客户资料完整详情视图。
fn build_detail(parts: ProfileDetailParts) -> CustomerProfileDetailView {
    let ProfileDetailParts {
        account,
        party,
        current_revision,
        mut revisions,
        assignments,
        contacts,
        addresses,
        tax_profiles,
        bank_accounts,
        sensitive_fields,
    } = parts;
    let today = BusinessDate::today();
    let owner_user_id = assignments
        .iter()
        .find(|item| item.assignment_role == AssignmentRole::Owner && item.is_active_on(today))
        .map(|item| item.user_id.clone());
    let collaborator_count = assignments
        .iter()
        .filter(|item| item.assignment_role == AssignmentRole::Collaborator && item.is_active_on(today))
        .count() as u32;
    let mut customer = CustomerView::from(account);
    customer.party_no = Some(party.party_no.clone());
    customer.legal_name = Some(current_revision.legal_name.clone());
    customer.short_name = current_revision.short_name.clone();
    customer.owner_user_id = owner_user_id;
    customer.collaborator_count = collaborator_count;
    revisions.sort_by_key(|item| std::cmp::Reverse(item.revision.revision_no));
    let action_blockers = customer_status_blockers(customer.status);
    CustomerProfileDetailView {
        account: customer,
        party_status: super::views::party_status(party.stable.status),
        party_version: party.base.version,
        unified_credit_code: party.unified_credit_code,
        current_revision: super::views::party_revision_view(current_revision),
        revisions: revisions.into_iter().map(super::views::party_revision_view).collect(),
        assignments: assignments.into_iter().map(CustomerAssignmentView::from).collect(),
        contacts: contacts.into_iter().map(super::views::party_contact_view).collect(),
        addresses: addresses.into_iter().map(super::views::party_address_view).collect(),
        tax_profiles: tax_profiles.into_iter().map(super::views::party_tax_profile_view).collect(),
        bank_accounts: bank_accounts.into_iter().map(super::views::party_bank_account_view).collect(),
        sensitive_fields,
        allowed_actions: Vec::new(),
        action_blockers,
    }
}

/// 返回客户状态产生的业务动作阻断原因。
pub(super) fn customer_status_blockers(status: CustomerAccountStatus) -> Vec<CustomerActionBlockerView> {
    if status.is_active() {
        return Vec::new();
    }
    ["UPLOAD_CONTRACT_PDF", "CREATE_SALES_ORDER"]
        .into_iter()
        .map(|action| CustomerActionBlockerView {
            action: action.to_string(),
            code: "CUSTOMER_DISABLED".to_string(),
            message: "客户已停用，请先恢复客户后再发起新业务".to_string(),
        })
        .collect()
}
