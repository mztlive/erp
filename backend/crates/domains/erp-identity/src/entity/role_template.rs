//! 部署岗位模板的身份、权限分工及生成规则。

use erp_permissions::policy_permission::FINANCE_LEDGER_READ;
use serde::Serialize;

use crate::entity::{Permission, PermissionSet, Role};
use crate::{Error, Result};

/// 供部署时选择和显式生成的普通岗位角色模板，不包含超级管理员。
#[derive(Debug, Clone, Serialize)]
pub struct BuiltinRoleTemplate {
    /// 生成角色所用的稳定岗位 ID。
    pub id: String,
    /// 岗位模板及所生成角色的展示名称。
    pub name: String,
    /// 用于选岗预览的职责和能力边界。
    pub description: String,
    /// 模板生成时写入角色的直接权限，不含人员范围。
    pub permissions: Vec<Permission>,
    /// 预览时提示管理员另行配置的范围、任务或责任。
    pub setup_requirements: Vec<String>,
    /// 是否列入推荐模板；综合财务不推荐替代细分岗位。
    pub recommended: bool,
}

impl BuiltinRoleTemplate {
    /// 构造部署岗位模板，解析、筛选并去重共享权限；非法权限标识返回错误。
    /// 综合财务模板额外包含整账读取资格。
    pub fn new(id: &str, name: &str, description: &str, raw: &[&str]) -> Result<Self> {
        let mut permissions =
            raw.iter().map(Permission::parse).collect::<std::result::Result<Vec<_>, _>>()?;
        Self::retain_position_permissions(id, &mut permissions);
        if id == "role-finance" {
            permissions.push(FINANCE_LEDGER_READ);
        }

        Ok(Self {
            id: id.into(),
            name: name.into(),
            description: if id == "role-management" {
                "查看经营、履约、票款及待办责任，不修改业务或审批状态。".into()
            } else if id == "role-sysadmin" {
                "处理同步与集成异常、技术配置和组织治理；审批流程管理单独授权。".into()
            } else {
                description.into()
            },
            permissions: PermissionSet::new(permissions).into_vec(),
            setup_requirements: Self::setup_requirements(id).iter().map(|value| (*value).into()).collect(),
            recommended: id != "role-finance",
        })
    }

    // 生成模板时限制管理层为业务只读和待办管理，排除技术岗的审批实例与流程权限。
    fn retain_position_permissions(id: &str, permissions: &mut Vec<Permission>) {
        match id {
            "role-management" => permissions.retain(|permission| {
                matches!(permission.action(), "list" | "detail" | "read")
                    || (permission.resource() == "work_item" && permission.action() == "manage")
            }),
            "role-sysadmin" => permissions.retain(|permission| {
                !matches!(permission.resource(), "approval_process" | "approval_instance")
            }),
            _ => {},
        }
    }

    // 给出生成岗位后仍需管理员配置的范围、流程或责任；生成本身不写入这些配置。
    fn setup_requirements(id: &str) -> &'static [&'static str] {
        match id {
            "role-sales" => &[
                "本人客户与销售单可按默认责任访问；配置人员和仓库候选目录。",
                "分配客户负责人，发布销售及开票申请审批流程。",
            ],
            "role-sales-leader" => &[
                "为客户和销售单追加所负责部门的查看范围，并配置人员目录。",
                "在销售领导审批节点中指派具体人员。",
            ],
            "role-procurement" => &[
                "配置跨销售的采购协同范围及库存仓库范围；商品、供应商和采购业务按岗位权限共享。",
                "设置采购责任规则，配置人员和仓库候选目录。",
                "采购内部审批须指定申请人以外的采购审批人；付款申请另交财务审核。",
            ],
            "role-operations" => &[
                "商品按岗位权限共享；配置卡券销售协同范围及人员、仓库候选目录。",
                "在卡券运营审批节点中指派具体人员。",
            ],
            "role-warehouse" => &[
                "分别配置库存余额、流水、预占和库存调整的仓库范围。",
                "在仓库资料指定收货、发货经办人；配置仓库和人员候选目录。",
            ],
            "role-finance" => &[
                "综合财务包含审批、资金、发票和责任配置全部能力；分岗组织优先选择三个细分岗位。",
                "配置业务查看范围，分别指定审批、付款和开票负责人。",
            ],
            "role-management" => &[
                "为经营分析所需客户和销售追加全公司查看范围；采购及结算按岗位权限共享。",
                "查看他人待办须配置工作项管理范围；该模板不授予改派或审批操作。",
            ],
            _ => &[
                "配置组织管理、人员查询资格、异常单据及待办管理的范围。",
                "如承担审批流程维护，须另配通用管理动作、单据类型管理资格及业务来源访问权。",
            ],
        }
    }

    /// 从综合财务模板派生指定的审批、资金或开票岗位，保留通用能力和该岗写权限。
    /// 其他资源仅保留相应读取入口；非综合财务模板或无法解析的派生权限返回错误。
    pub fn finance(&self, kind: FinancePosition) -> Result<Self> {
        if self.id != "role-finance" {
            return Err(Error::ValidationError("财务岗位必须从综合财务模板派生".into()));
        }

        let (id, name, description, setup) = kind.metadata();
        let mut permissions = Vec::new();
        for permission in &self.permissions {
            if permission.resource() == "purchase_payment_request" {
                if kind.allows_payment_request(permission.action()) {
                    permissions.push(permission.clone());
                }
                continue;
            }
            if kind.allows_write(permission.resource()) || Self::shared_operation(permission) {
                permissions.push(permission.clone());
            } else if permission.action() == "*" {
                for action in Self::read_actions(permission.resource()) {
                    permissions.push(Permission::parse(format!("{}:{action}", permission.resource()))?);
                }
            } else if matches!(permission.action(), "list" | "detail" | "read" | "reveal") {
                permissions.push(permission.clone());
            }
        }

        Ok(Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            permissions: PermissionSet::new(permissions).into_vec(),
            setup_requirements: setup.iter().map(|value| (*value).into()).collect(),
            recommended: true,
        })
    }

    // 将不允许写入的资源通配权限收窄为实际读入口，不为冲正或供应商退款增加列表权限。
    fn read_actions(resource: &str) -> &'static [&'static str] {
        match resource {
            "purchase_invoice_allocation" | "supplier_settlement_difference" => &["list"],
            "supplier_refund" | "receipt_reversal" | "payment_reversal" => &["detail"],
            _ => &["list", "detail"],
        }
    }

    // 识别财务细分岗共同保留的撤销审批、附件和后台任务操作资格。
    fn shared_operation(permission: &Permission) -> bool {
        if permission.resource() == "approval_instance" && permission.action() == "cancel" {
            return true;
        }
        matches!(
            permission.resource(),
            "file_asset" | "document_attachment" | "background_job" | "background_job_item"
        )
    }

    /// 根据服务端目录按请求顺序解析要生成的模板；空列表、重复或未知 ID 整批拒绝。
    pub fn select(catalog: &[Self], ids: &[String]) -> Result<Vec<Self>> {
        if ids.is_empty() {
            return Err(Error::ValidationError("请选择有效的内建岗位".into()));
        }

        let mut selected = Vec::new();
        for id in ids {
            if selected.iter().any(|template: &Self| template.id == *id) {
                return Err(Error::ValidationError("内建岗位不能重复选择".into()));
            }
            selected.push(
                catalog
                    .iter()
                    .find(|template| template.id == *id)
                    .cloned()
                    .ok_or_else(|| Error::ValidationError(format!("未登记的内建岗位：{id}")))?,
            );
        }
        Ok(selected)
    }
}

/// 财务岗位只在生成模板时使用，不参与运行时业务授权。
#[derive(Debug, Clone, Copy)]
pub enum FinancePosition {
    Director,
    Cashier,
    Invoice,
}

impl FinancePosition {
    // 为财务分岗保留付款申请的列表和详情，仅财务总监可关闭、出纳可付款。
    fn allows_payment_request(self, action: &str) -> bool {
        matches!(action, "list" | "detail")
            || matches!((self, action), (Self::Director, "close") | (Self::Cashier, "pay"))
    }

    // 提供财务细分岗的固定 ID、展示名称、职责及需另行配置的责任。
    fn metadata(self) -> (&'static str, &'static str, &'static str, &'static [&'static str]) {
        match self {
            Self::Director => (
                "role-finance-director",
                "财务总监",
                "审核采购付款申请与票款单据，管理成本、结算及财务责任配置。",
                &[
                    "指派为财务审批节点负责人；不得审批本人提交。",
                    "配置客户和销售查看范围；采购及结算按岗位权限共享，库存复核配置仓库范围。",
                ],
            ),
            Self::Cashier => (
                "role-cashier",
                "出纳",
                "办理客户回款、供应商付款、退款与冲正，不承担审批和责任规则配置。",
                &["在财务责任配置中指定为付款负责人。", "审批流程指定其他人员。"],
            ),
            Self::Invoice => (
                "role-invoice-clerk",
                "开票",
                "按开票任务办理销项发票、进项发票及核销，不承担资金操作和审批。",
                &["在财务责任配置中指定为销项开票负责人。"],
            ),
        }
    }

    // 确定财务细分岗可保留完整权限的资源；其他非共享资源只保留读取。
    fn allows_write(self, resource: &str) -> bool {
        match self {
            Self::Director => matches!(
                resource,
                "approval_instance"
                    | "finance_responsibility"
                    | "party_bank_account"
                    | "cost_entry"
                    | "supplier_settlement_statement"
                    | "supplier_settlement_difference"
                    | "integration_task"
                    | "legacy_import_confirmation"
            ),
            Self::Cashier => matches!(
                resource,
                "customer_receipt"
                    | "supplier_payment"
                    | "sales_return_case"
                    | "customer_refund"
                    | "supplier_refund"
                    | "receipt_reversal"
                    | "payment_reversal"
            ),
            Self::Invoice => matches!(resource, "invoice" | "purchase_invoice_allocation"),
        }
    }
}

/// 既有角色的状态；任何既有身份都不得由生成动作覆盖或恢复。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinRoleState {
    Missing,
    Existing,
    Disabled,
    Deleted,
}

impl BuiltinRoleState {
    /// 将按固定 ID 读到的角色区分为缺失、软删除、停用或存在，软删除优先于停用。
    /// 仅缺失的岗位允许生成，已有角色不覆盖也不恢复。
    pub fn from_role(role: Option<&Role>) -> Self {
        match role {
            None => Self::Missing,
            Some(role) if role.base.is_deleted() => Self::Deleted,
            Some(role) if role.disabled => Self::Disabled,
            Some(_) => Self::Existing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 验证模板选择拒绝空集，并在选择过多时仍报告具体的重复或未知 ID。
    #[test]
    fn oversized_selection_reports_duplicate_or_unknown_template() {
        let catalog = [BuiltinRoleTemplate::new("role-sales", "销售", "销售岗位", &[]).unwrap()];
        for (ids, expected) in [
            (vec!["role-sales".into(), "role-sales".into()], "内建岗位不能重复选择"),
            (vec!["role-sales".into(), "unknown".into()], "未登记的内建岗位：unknown"),
        ] {
            assert!(matches!(BuiltinRoleTemplate::select(&catalog, &ids),
                Err(Error::ValidationError(message)) if message == expected));
        }

        assert!(BuiltinRoleTemplate::select(&[], &["unknown".into()]).is_err());
        assert!(BuiltinRoleTemplate::select(&catalog, &[]).is_err());

        let selected = BuiltinRoleTemplate::select(&catalog, &["role-sales".into()]).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, "role-sales");
    }
}
