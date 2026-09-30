//! 演示岗位、部门、审批链和默认责任规则。脚本与接口共用本目录的 `dev-foundation.json`。
//!
//! 规格必须放在 crate 内。web-api 镜像只复制 `backend/`，编译时读不到仓库根的 `scripts/`。

use std::collections::HashMap;
use std::sync::OnceLock;

use erp_identity::access_control::{DataScopeType, ScopeDimension, ScopeTargetMode};
use erp_identity::dto::person_scope::{PersonBusinessOption, PersonScopeGrant, SavePersonScopeRequest};
use erp_identity::entity::access_control::person_scope::{PersonDataScope, PersonScopeTerm};
use erp_party::dto::company::SaveCompanyRequest;
use erp_workflow::{DocumentType, FinanceResponsibilityOperation};
use serde::Deserialize;

use crate::{Error, Result};

#[derive(Deserialize)]
pub(super) struct FoundationFile {
    pub company: SaveCompanyRequest,
    pub password: String,
    pub root_department: String,
    #[cfg(test)]
    pub sales_department: String,
    #[cfg(test)]
    pub sales_leader_account: String,
    #[cfg(test)]
    pub sales_leader_role_id: String,
    pub customer_owner_account: String,
    pub supplier_maintainer_account: String,
    pub product_maintainer_account: String,
    pub warehouse_handler_account: String,
    /// 默认采购调度人，取值是岗位规格键。
    pub procurement_responsibility_owner: String,
    pub finance_responsibilities: Vec<FinanceResponsibilitySpec>,
    pub accounts: Vec<AccountSpec>,
    pub departments: Vec<DepartmentSpec>,
    pub approvals: Vec<ApprovalSpec>,
    pub required_permissions: HashMap<String, Vec<String>>,
    pub person_scope_defaults: PersonScopeDefaults,
}

#[derive(Deserialize)]
pub(super) struct PersonScopeDefaults {
    pub self_accounts: Vec<String>,
    pub department_accounts: Vec<String>,
    pub company_resources: HashMap<String, Vec<String>>,
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

/// 演示范围配置的固定输入选择。
pub(super) struct DemoPersonScope;
impl DemoPersonScope {
    /// 只为未配置的准入动作生成追加范围，保留撤权和旧表达式。
    /// # 参数
    /// 账号、服务端业务准入、已有记录与本次策略版本。
    /// # 返回
    /// 需要写入的正式规范化请求；本人基础或已有配置返回 None。
    /// # 错误
    /// 维度、动作或范围违反当前合同则失败。
    pub(super) fn request(
        login: &str,
        business: &PersonBusinessOption,
        items: &[PersonDataScope],
        policy_version: u64,
    ) -> Result<Option<SavePersonScopeRequest>> {
        let actions = business
            .configurable_actions
            .iter()
            .filter(|action| {
                !items.iter().any(|row| row.resource == business.resource && row.action == **action)
            })
            .cloned()
            .collect::<Vec<_>>();
        if actions.is_empty() {
            return Ok(None);
        }
        let term = Self::term(login, &business.resource, business.default_self, &business.dimensions)?;
        if business.default_self && term.scope_type == DataScopeType::SelfOwned {
            return Ok(None);
        }
        let request = SavePersonScopeRequest {
            resource: business.resource.clone(),
            actions: actions.clone(),
            grants: vec![PersonScopeGrant { actions, terms: vec![term] }],
            replace_legacy: false,
            expected_policy_version: policy_version,
        }
        .normalized(&business.dimensions)?;
        Ok(Some(request))
    }

    /// 演示岗位初始选择；跨维度业务明确使用公司范围。
    /// # 参数
    /// 登录名、资源、本人基础标记及登记维度。
    /// # 返回
    /// 声明的本人、部门或公司范围条件。
    /// # 错误
    /// 未登记维度时返回错误。
    pub(super) fn term(
        login: &str,
        resource: &str,
        default_self: bool,
        dimensions: &[ScopeDimension],
    ) -> Result<PersonScopeTerm> {
        let defaults = &foundation_spec().person_scope_defaults;
        let dimension = dimensions
            .first()
            .copied()
            .ok_or_else(|| Error::ValidationError(format!("演示范围 {resource} 缺少维度")))?;
        let internal = dimensions == [ScopeDimension::InternalOrg];
        let company = defaults
            .company_resources
            .get(login)
            .is_some_and(|resources| resources.iter().any(|value| value == resource));
        let own =
            !company && internal && default_self && defaults.self_accounts.iter().any(|value| value == login);
        let department =
            !company && internal && defaults.department_accounts.iter().any(|value| value == login);
        Ok(PersonScopeTerm {
            scope_type: if own {
                DataScopeType::SelfOwned
            } else if department {
                DataScopeType::Organization
            } else {
                DataScopeType::Company
            },
            target_dimension: dimension,
            target_mode: department.then_some(ScopeTargetMode::OwnOrg),
            include_descendants: department.then_some(true),
            scope_targets: vec![],
        })
    }
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
        let product_maintainer = account_named(&spec.product_maintainer_account).expect("商品维护岗位");
        assert_eq!(product_maintainer.account, "caigou");
        assert_eq!(product_maintainer.role_id, "role-procurement");
        assert!(spec.departments.iter().any(|department| {
            department.accounts.iter().any(|account| account == &spec.product_maintainer_account)
        }));
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
