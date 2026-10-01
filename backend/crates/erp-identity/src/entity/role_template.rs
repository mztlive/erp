//! 部署岗位模板的身份、权限分工及生成规则。

use serde::Serialize;

use crate::entity::policy_permission::FINANCE_LEDGER_READ;
use crate::entity::{Permission, PermissionSet, Role};
use crate::{Error, Result};

/// 内建岗位只提供普通可编辑角色，不包含超级管理员。
#[derive(Debug, Clone, Serialize)]
pub struct BuiltinRoleTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub permissions: Vec<Permission>,
    pub setup_requirements: Vec<String>,
    pub recommended: bool,
}

impl BuiltinRoleTemplate {
    /// 根据共享岗位权限构造模板；财务整账资格随显式生成一同提供。
    /// # 参数
    /// 稳定岗位身份、名称、职责及共享权限。
    /// # 返回
    /// 去重后的普通岗位模板。
    /// # 错误
    /// 任一权限标识非法时拒绝。
    pub fn new(id: &str, name: &str, description: &str, raw: &[&str]) -> Result<Self> {
        let mut permissions =
            raw.iter().map(Permission::parse).collect::<std::result::Result<Vec<_>, _>>()?;
        Self::retain_position_permissions(id, &mut permissions);
        if id == "role-finance" {
            permissions.push(Permission::parse(FINANCE_LEDGER_READ)?);
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

    /// 管理层只读，技术管理员不从技术岗位自动取得业务审批及流程配置资格。
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

    /// 岗位上岗前必须明确的人员范围与任务配置，不在角色生成时写人员数据。
    fn setup_requirements(id: &str) -> &'static [&'static str] {
        match id {
            "role-sales" => &[
                "本人客户与销售单可按默认责任访问；配置人员、仓库和结算主体候选目录。",
                "分配客户负责人，发布销售及开票申请审批流程。",
            ],
            "role-sales-leader" => &[
                "为客户和销售单追加所负责部门的查看范围，并配置人员与结算主体目录。",
                "在销售领导审批节点中指派具体人员。",
            ],
            "role-procurement" => &[
                "配置跨销售的采购协同范围、商品供应商共享范围及库存仓库范围。",
                "设置采购责任规则，配置人员、仓库和结算主体候选目录。",
            ],
            "role-operations" => &[
                "为商品及卡券销售协同配置适用范围；配置人员和仓库候选目录。",
                "在卡券运营审批节点中指派具体人员。",
            ],
            "role-warehouse" => &[
                "分别配置库存余额、流水、预占和库存调整的仓库范围。",
                "在仓库资料指定收货、发货经办人；配置仓库和人员候选目录。",
            ],
            "role-finance" => &[
                "综合财务包含审批、资金、发票和责任配置全部能力；分岗组织优先选择三个细分岗位。",
                "配置业务查看范围及结算主体目录，分别指定审批、付款和开票负责人。",
            ],
            "role-management" => &[
                "为经营分析所需客户、销售、采购及结算追加全公司查看范围。",
                "查看他人待办须配置工作项管理范围；该模板不授予改派或审批操作。",
            ],
            _ => &[
                "配置组织管理、人员查询资格、异常单据及待办管理的范围。",
                "如承担审批流程维护，须另配通用管理动作、单据类型管理资格及业务来源访问权。",
            ],
        }
    }

    /// 从综合财务模板派生审批、资金、发票三个互相独立的岗位。
    /// # 参数
    /// `kind` 为明确的财务岗位。
    /// # 返回
    /// 保留共同读取与附件能力、只开放岗位写操作的模板。
    /// # 错误
    /// 来源不是综合财务或静态权限错误时拒绝。
    pub fn finance(&self, kind: FinancePosition) -> Result<Self> {
        if self.id != "role-finance" {
            return Err(Error::ValidationError("财务岗位必须从综合财务模板派生".into()));
        }
        let (id, name, description, setup) = kind.metadata();
        let mut permissions = Vec::new();
        for permission in &self.permissions {
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

    /// 财务资源真实读取动作；冲正及供应商退款没有独立列表入口。
    fn read_actions(resource: &str) -> &'static [&'static str] {
        match resource {
            "purchase_invoice_allocation" | "supplier_settlement_difference" => &["list"],
            "supplier_refund" | "receipt_reversal" | "payment_reversal" => &["detail"],
            _ => &["list", "detail"],
        }
    }

    /// 附件、本人后台任务及单据上下文能力供财务各岗位共同使用。
    fn shared_operation(permission: &Permission) -> bool {
        if permission.resource() == "approval_instance" && permission.action() == "cancel" {
            return true;
        }
        matches!(
            permission.resource(),
            "file_asset" | "document_attachment" | "background_job" | "background_job_item"
        )
    }

    /// 解析一批模板选择，拒绝空集、重复和未知身份，保持用户选择顺序。
    /// # 参数
    /// 服务端目录与客户端选择的模板标识。
    /// # 返回
    /// 仅包含服务端定义的模板。
    /// # 错误
    /// 不合法选择整批拒绝。
    pub fn select(catalog: &[Self], ids: &[String]) -> Result<Vec<Self>> {
        if ids.is_empty() || ids.len() > catalog.len() {
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
    /// 返回岗位固定身份、职责和必须由管理员配置的责任。
    fn metadata(self) -> (&'static str, &'static str, &'static str, &'static [&'static str]) {
        match self {
            Self::Director => (
                "role-finance-director",
                "财务总监",
                "审批采购与票款单据，管理成本、结算及财务责任配置。",
                &[
                    "指派为财务审批节点负责人；不得审批本人提交。",
                    "配置客户、销售、采购及结算查看范围；库存复核配置仓库范围。",
                ],
            ),
            Self::Cashier => (
                "role-cashier",
                "出纳",
                "办理客户回款、供应商付款、退款与冲正，不承担审批和责任规则配置。",
                &["在财务责任配置中指定为付款负责人。", "配置结算主体候选目录，审批流程指定其他人员。"],
            ),
            Self::Invoice => (
                "role-invoice-clerk",
                "开票",
                "按开票任务办理销项发票、进项发票及核销，不承担资金操作和审批。",
                &["在财务责任配置中指定为销项开票负责人。", "配置结算主体候选目录。"],
            ),
        }
    }

    /// 判定本岗位拥有写操作的业务，其他财务业务保留读取。
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
    /// 将包含软删除记录的角色读取结果映射为生成状态。
    /// # 参数
    /// 按固定模板身份读取的角色。
    /// # 返回
    /// 只有 Missing 允许新建。
    /// # 错误
    /// 无。
    pub fn from_role(role: Option<&Role>) -> Self {
        match role {
            None => Self::Missing,
            Some(role) if role.base.is_deleted() => Self::Deleted,
            Some(role) if role.disabled => Self::Disabled,
            Some(_) => Self::Existing,
        }
    }
}
