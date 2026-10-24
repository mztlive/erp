//! 客户资料详情查询与视图映射。

use std::collections::HashMap;

use erp_core::common::time::BusinessDate;
use erp_core::ids::PartyId;
use erp_customer::dto::customer::CustomerOwnerView;
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

use super::CustomerProfileProcess;
use crate::{Error, Result};

impl CustomerProfileProcess {
    /// 查询客户资料对象中心的当前事实、历史版本与敏感字段揭示入口。
    ///
    /// # 参数
    /// * `customer_id` - 客户角色主键。
    ///
    /// # 返回
    /// 返回客户、主体、修订、归属、当前事实和敏感字段令牌入口。归属人没有显示名时回退为账号 ID。
    ///
    /// # 错误
    /// 客户或关联主体不存在、当前名称修订无法解析、敏感令牌无法签发，或任一仓储查询失败时返回错误。
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
        let detail = build_detail(
            ProfileDetailParts {
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
            },
            &account_names,
        );
        Ok(detail)
    }

    /// 按主键加载客户角色。
    ///
    /// # 参数
    /// * `id` - 客户角色主键。
    ///
    /// # 返回
    /// 返回找到的客户角色。
    ///
    /// # 错误
    /// 客户不存在时返回未找到；查询失败时返回仓储错误。
    pub(super) async fn load_customer(&self, id: &str) -> Result<CustomerAccount> {
        self.db
            .customer_accounts()
            .find_customer(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户不存在".to_string()))
    }

    /// 加载客户关联的主体。
    ///
    /// # 参数
    /// * `party_id` - 客户角色上的主体主键。
    ///
    /// # 返回
    /// 返回对应主体。
    ///
    /// # 错误
    /// 主体不存在时返回未找到；查询失败时返回仓储错误。
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
fn build_detail(parts: ProfileDetailParts, names: &HashMap<String, String>) -> CustomerProfileDetailView {
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
    let mut customer = CustomerView::from(account);
    customer.party_no = Some(party.party_no.clone());
    customer.legal_name = Some(current_revision.legal_name.clone());
    customer.short_name = current_revision.short_name.clone();
    populate_owners(&mut customer, &assignments, names, BusinessDate::today());
    revisions.sort_by_key(|item| std::cmp::Reverse(item.revision.revision_no));
    let action_blockers = customer_status_blockers(customer.status);
    CustomerProfileDetailView {
        account: customer,
        party_status: super::views::party_status(party.stable.status),
        party_version: party.base.version,
        unified_credit_code: party.unified_credit_code,
        current_revision: super::views::party_revision_view(current_revision),
        revisions: revisions.into_iter().map(super::views::party_revision_view).collect(),
        assignments: assignments
            .into_iter()
            .map(|assignment| {
                let mut view = CustomerAssignmentView::from(assignment);
                view.user_name = names.get(&view.user_id).cloned().unwrap_or_else(|| view.user_id.clone());
                view
            })
            .collect(),
        contacts: contacts.into_iter().map(super::views::party_contact_view).collect(),
        addresses: addresses.into_iter().map(super::views::party_address_view).collect(),
        tax_profiles: tax_profiles.into_iter().map(super::views::party_tax_profile_view).collect(),
        bank_accounts: bank_accounts.into_iter().map(super::views::party_bank_account_view).collect(),
        sensitive_fields,
        allowed_actions: Vec::new(),
        action_blockers,
    }
}

/// 所有当前负责销售与单负责人兼容字段共享同一有效期口径。
fn populate_owners(
    customer: &mut CustomerView,
    assignments: &[CustomerAssignment],
    names: &HashMap<String, String>,
    today: BusinessDate,
) {
    let active = assignments
        .iter()
        .filter(|item| !item.base.is_deleted() && item.is_active_on(today))
        .cloned()
        .collect::<Vec<_>>();
    customer.owners = CustomerAssignment::owner_ids(&active)
        .into_iter()
        .map(|user_id| CustomerOwnerView { user_name: names.get(&user_id).cloned(), user_id })
        .collect();
    customer.owner_user_id = customer.owners.first().map(|owner| owner.user_id.clone());
    customer.owner_user_name = customer.owners.first().and_then(|owner| owner.user_name.clone());
    customer.collaborator_count =
        active.iter().filter(|item| item.assignment_role == AssignmentRole::Collaborator).count() as u32;
}

/// 返回客户状态产生的业务动作阻断原因。
///
/// # 参数
/// * `status` - 客户角色状态。
///
/// # 返回
/// 客户有效时返回空列表；已停用时返回上传合同和创建销售单两条阻断。
///
/// # 错误
/// 不返回错误。
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

#[cfg(test)]
mod tests {
    use erp_core::ids::{CustomerAssignmentId, PartyRevisionId};
    use erp_customer::{CustomerAccountData, CustomerAssignmentData};
    use erp_party::{PartyData, PartyKind, PartyRevisionData, PartyStatus};

    use super::*;

    #[test]
    fn canonical_profile_detail_includes_every_current_owner_and_display_name() {
        let party_id = PartyId::new("shared-party");
        let account = CustomerAccount::new(
            CustomerAccountId::new("customer"),
            CustomerAccountData {
                party_id: party_id.clone(),
                customer_no: "KH-1".into(),
                default_payment_term_id: None,
                status: CustomerAccountStatus::Active,
            },
            "admin",
        )
        .unwrap();
        let party = Party::new(
            party_id.clone(),
            PartyData {
                party_no: "P-1".into(),
                party_kind: PartyKind::Enterprise,
                unified_credit_code: None,
                status: PartyStatus::Active,
            },
            "admin",
        )
        .unwrap();
        let revision = PartyRevision::new(
            PartyRevisionId::new("revision"),
            PartyRevisionData {
                party_id,
                revision_no: 1,
                legal_name: "共享客户".into(),
                short_name: None,
                change_reason: "建档".into(),
            },
        )
        .unwrap();
        let today = BusinessDate::today();
        let start = BusinessDate::from_ymd(2000, 1, 1).unwrap();
        let assignments = ["sales-b", "sales-a", "future", "ended", "deleted", "collaborator"]
            .into_iter()
            .map(|id| {
                let mut assignment = CustomerAssignment::new(
                    CustomerAssignmentId::new(id),
                    CustomerAssignmentData {
                        customer_id: CustomerAccountId::new("customer"),
                        user_id: id.into(),
                        assignment_role: if id == "collaborator" {
                            AssignmentRole::Collaborator
                        } else {
                            AssignmentRole::Owner
                        },
                        valid_from: if id == "future" {
                            BusinessDate::from_ymd(9999, 1, 1).unwrap()
                        } else {
                            start
                        },
                        valid_to: (id == "ended").then_some(today),
                        change_reason: "分配".into(),
                    },
                )
                .unwrap();
                if id == "deleted" {
                    assignment.base.deleted_at = 1;
                }
                assignment
            })
            .collect();
        let names =
            [("sales-a".into(), "张三".into()), ("sales-b".into(), "李四".into())].into_iter().collect();
        let detail = build_detail(
            ProfileDetailParts {
                account,
                party,
                current_revision: revision.clone(),
                revisions: vec![revision],
                assignments,
                contacts: vec![],
                addresses: vec![],
                tax_profiles: vec![],
                bank_accounts: vec![],
                sensitive_fields: vec![],
            },
            &names,
        );
        assert_eq!(
            detail.account.owners.iter().map(|owner| owner.user_id.as_str()).collect::<Vec<_>>(),
            vec!["sales-a", "sales-b"]
        );
        assert_eq!(detail.account.owners[1].user_name.as_deref(), Some("李四"));
        assert_eq!(detail.account.owner_user_name.as_deref(), Some("张三"));
        assert_eq!(detail.account.collaborator_count, 1);
        let wire = serde_json::to_value(detail).unwrap();
        assert_eq!(wire["owners"].as_array().unwrap().len(), 2);
        assert_eq!(wire["owners"][0]["user_name"], "张三");
    }
}
