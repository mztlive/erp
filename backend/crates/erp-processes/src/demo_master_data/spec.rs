//! 演示岗位、部门、审批链和默认责任规则。脚本与接口共用本目录的 `dev-foundation.json`。
//!
//! 规格必须放在 crate 内。web-api 镜像只复制 `backend/`，编译时读不到仓库根的 `scripts/`。

use std::sync::OnceLock;

use erp_party::dto::company::SaveCompanyRequest;
use erp_workflow::{DocumentType, FinanceResponsibilityOperation};
use serde::Deserialize;

use crate::{Error, Result};

#[derive(Deserialize)]
pub(super) struct FoundationFile {
    pub company: SaveCompanyRequest,
    pub password: String,
    pub root_department: String,
    pub sales_department: String,
    pub sales_leader_account: String,
    pub sales_leader_role_id: String,
    pub customer_owner_account: String,
    pub supplier_maintainer_account: String,
    pub warehouse_handler_account: String,
    /// 默认采购调度人，取值是岗位规格键。
    pub procurement_responsibility_owner: String,
    pub finance_responsibilities: Vec<FinanceResponsibilitySpec>,
    pub accounts: Vec<AccountSpec>,
    pub departments: Vec<DepartmentSpec>,
    pub approvals: Vec<ApprovalSpec>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FinanceResponsibilitySpec {
    pub operation: FinanceResponsibilityOperation,
    /// 岗位规格键。
    pub owner: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct AccountSpec {
    pub key: String,
    pub account: String,
    pub name: String,
    pub role_id: String,
    /// 种子脚本展示用的名称。
    #[allow(dead_code)]
    pub label: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct DepartmentSpec {
    pub key: String,
    pub name: String,
    pub accounts: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ApprovalSpec {
    pub document_type: String,
    pub name: String,
    /// 约定提交人。种子脚本用来校验岗位分离。
    #[allow(dead_code)]
    pub submitter: String,
    pub nodes: Vec<ApprovalNodeSpec>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ApprovalNodeSpec {
    pub name: String,
    pub assignee: String,
}

/// 返回进程内只解析一次的演示基础规格。
pub(super) fn foundation_spec() -> &'static FoundationFile {
    static SPEC: OnceLock<FoundationFile> = OnceLock::new();
    SPEC.get_or_init(|| {
        serde_json::from_str(include_str!("dev-foundation.json")).expect("演示基础规格无法解析")
    })
}

/// 把规格里的单据类型代码还原成工作流类型。
///
/// # 参数
/// * `code` - 规格中的单据类型代码
///
/// # 返回
/// 返回对应的单据类型。
///
/// # 错误
/// 代码不在单据类型表中时返回错误。
pub(super) fn document_type(code: &str) -> Result<DocumentType> {
    DocumentType::try_from_code(code).map_err(|error| Error::Internal(error.to_string()))
}

#[cfg(test)]
mod tests {
    use erp_workflow::FinanceResponsibilityOperation;

    use super::{document_type, foundation_spec};

    #[test]
    fn approval_chains_keep_submitters_off_their_own_nodes() {
        let spec = foundation_spec();
        assert_eq!(spec.approvals.len(), 12);
        for approval in &spec.approvals {
            assert!(document_type(&approval.document_type).is_ok(), "{}", approval.document_type);
            assert!(account_by_login_key(&approval.submitter).is_some(), "{}", approval.submitter);
            assert!(!approval.nodes.is_empty());
            assert!(approval.nodes.iter().all(|node| node.assignee != approval.submitter));
            for node in &approval.nodes {
                assert!(account_by_login_key(&node.assignee).is_some(), "{}", node.assignee);
            }
        }
    }

    #[test]
    fn account_display_names_are_people() {
        let titles = ["销售", "采购", "财务", "运营", "仓储", "出纳", "开票", "管理", "系统"];
        for account in &foundation_spec().accounts {
            assert!(account.name.chars().count() >= 2, "{}", account.account);
            assert_eq!(account.label, account.name);
            assert!(
                titles.iter().all(|title| !account.name.contains(title)),
                "{} 的姓名是岗位称呼: {}",
                account.account,
                account.name
            );
        }
    }

    #[test]
    fn departments_cover_the_accounts_that_open_demo_documents() {
        let spec = foundation_spec();
        assert!(spec.departments.iter().any(|department| {
            department.name == spec.sales_department
                && department.accounts.iter().any(|account| account == &spec.customer_owner_account)
                && department.accounts.iter().any(|account| account == &spec.sales_leader_account)
        }));
        assert!(spec.departments.iter().any(|department| {
            department.accounts.iter().any(|account| account == &spec.supplier_maintainer_account)
        }));
        assert!(account_named(&spec.customer_owner_account).is_some());
        assert!(account_named(&spec.supplier_maintainer_account).is_some());
        assert_eq!(
            account_named(&spec.sales_leader_account).map(|account| account.role_id.as_str()),
            Some(spec.sales_leader_role_id.as_str())
        );
    }

    #[test]
    fn default_responsibility_owners_are_the_procurement_and_finance_operators() {
        let spec = foundation_spec();
        let procurement =
            account_by_login_key(&spec.procurement_responsibility_owner).expect("默认采购调度人");
        assert_eq!(procurement.account, spec.supplier_maintainer_account);
        assert_eq!(procurement.role_id, "role-procurement");

        let operations = spec.finance_responsibilities.iter().map(|rule| rule.operation).collect::<Vec<_>>();
        assert_eq!(
            operations,
            vec![
                FinanceResponsibilityOperation::SupplierPayment,
                FinanceResponsibilityOperation::SalesInvoice,
            ]
        );
        let owners = spec
            .finance_responsibilities
            .iter()
            .map(|rule| account_by_login_key(&rule.owner).expect("财务责任人"))
            .collect::<Vec<_>>();
        assert!(owners.iter().all(|owner| owner.role_id == "role-finance"));
        assert_eq!(owners.iter().map(|owner| owner.key.as_str()).collect::<Vec<_>>(), ["payment", "invoice"]);
        assert!(owners.iter().all(|owner| owner.key != "finance"));
    }

    fn account_named(login: &str) -> Option<&'static super::AccountSpec> {
        foundation_spec().accounts.iter().find(|account| account.account == login)
    }

    fn account_by_login_key(key: &str) -> Option<&'static super::AccountSpec> {
        foundation_spec().accounts.iter().find(|account| account.key == key)
    }
}
