//! 资源动作的静态范围合同；配置、表达式和运行解析共用唯一目录。
use super::authorization_policy::AuthorizationPolicy::{
    self, Business, Directory, Governance, Role, Shared, SourceInherited, Task,
};
use super::{DataScopeType, ScopeBinding, ScopeDimension};
use crate::{Error, Permission, Result};

/// 同次解析需要的组织成员事实；不授予访问资格。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OrganizationFacts {
    Actor,
    AllMembers,
}

/// 范围条款的来源；不替代入口动作权限与对象责任证明。
#[derive(Debug, Clone, Copy)]
enum ScopeSource {
    Current,
    Fixed { resource: &'static str, action: &'static str },
    SameAction { resource: &'static str },
}

/// 不可变动作规格；所有字段只由本模块的静态目录声明。
#[derive(Debug, Clone, Copy)]
pub struct ActionSpec {
    action: &'static str,
    policy: AuthorizationPolicy,
    default_self: bool,
    supported_dimensions: &'static [ScopeDimension],
    required_dimensions: &'static [ScopeDimension],
    allows_history: bool,
}

/// 资源共用约束与逐动作规格。
pub(crate) struct ConsumerSpec {
    resource: &'static str,
    actions: &'static [ActionSpec],
    scope_source: ScopeSource,
    scope_types: &'static [DataScopeType],
    organization_facts: OrganizationFacts,
}

const INTERNAL_ORG: &[ScopeDimension] = &[ScopeDimension::InternalOrg];
const WAREHOUSE: &[ScopeDimension] = &[ScopeDimension::Warehouse];
const SETTLEMENT_PARTY: &[ScopeDimension] = &[ScopeDimension::SettlementParty];
const APPROVAL_DIMENSIONS: &[ScopeDimension] =
    &[ScopeDimension::InternalOrg, ScopeDimension::Warehouse, ScopeDimension::SettlementParty];
const DEPOSIT_DIMENSIONS: &[ScopeDimension] = &[ScopeDimension::InternalOrg, ScopeDimension::SettlementParty];
const ALL_TYPES: &[DataScopeType] = &[
    DataScopeType::Company,
    DataScopeType::Organization,
    DataScopeType::Team,
    DataScopeType::SelfOwned,
    DataScopeType::Collaborative,
];
const PERSON_TYPES: &[DataScopeType] =
    &[DataScopeType::Company, DataScopeType::Organization, DataScopeType::Team, DataScopeType::SelfOwned];
const OBJECT_TYPES: &[DataScopeType] = &[DataScopeType::Company, DataScopeType::Organization];

/// 仅声明已接入范围消费者的动作；实际对象端口与查询映射由组合根登记。
pub(crate) const CONSUMERS: &[ConsumerSpec] = &[
    ConsumerSpec::new(
        "approval_instance",
        &[
            ActionSpec::new("read", Task, APPROVAL_DIMENSIONS).with_optional_dimensions(),
            ActionSpec::new("decide", Task, APPROVAL_DIMENSIONS).with_optional_dimensions(),
            ActionSpec::new("resume", Task, APPROVAL_DIMENSIONS).with_optional_dimensions(),
            ActionSpec::new("cancel", Task, APPROVAL_DIMENSIONS).with_optional_dimensions(),
            ActionSpec::new("cancel_blocked", Task, APPROVAL_DIMENSIONS).with_optional_dimensions(),
            ActionSpec::new("upgrade_binding", Task, APPROVAL_DIMENSIONS).with_optional_dimensions(),
        ],
    ),
    ConsumerSpec::new(
        "stock_adjustment",
        &[
            ActionSpec::new("list", Business, WAREHOUSE),
            ActionSpec::new("detail", Business, WAREHOUSE),
            ActionSpec::new("create", Business, WAREHOUSE),
            ActionSpec::new("update", Business, WAREHOUSE),
            ActionSpec::new("submit", Business, WAREHOUSE),
        ],
    ),
    ConsumerSpec::new(
        "stock_balance",
        &[ActionSpec::new("list", Business, WAREHOUSE), ActionSpec::new("detail", Business, WAREHOUSE)],
    ),
    ConsumerSpec::new("stock_movement", &[ActionSpec::new("list", Business, WAREHOUSE)]),
    ConsumerSpec::new("stock_reservation", &[ActionSpec::new("list", Business, WAREHOUSE)]),
    ConsumerSpec::new("customer_refund", &[ActionSpec::new("submit", SourceInherited, SETTLEMENT_PARTY)]),
    ConsumerSpec::new("supplier_refund", &[ActionSpec::new("submit", SourceInherited, SETTLEMENT_PARTY)]),
    ConsumerSpec::new(
        "supplier_settlement_statement",
        &[
            ActionSpec::new("list", Shared, INTERNAL_ORG),
            ActionSpec::new("detail", Shared, INTERNAL_ORG),
            ActionSpec::new("create", Shared, INTERNAL_ORG),
            ActionSpec::new("update", Shared, INTERNAL_ORG),
            ActionSpec::new("submit", Shared, INTERNAL_ORG),
            ActionSpec::new("confirm", Task, INTERNAL_ORG),
        ],
    ),
    ConsumerSpec::new(
        "work_item",
        &[
            ActionSpec::new("list", Governance, INTERNAL_ORG).with_self(),
            ActionSpec::new("detail", Governance, INTERNAL_ORG).with_self(),
            ActionSpec::new("manage", Governance, INTERNAL_ORG),
        ],
    )
    .with_all_members(),
    ConsumerSpec::new(
        "org_unit",
        &[
            ActionSpec::new("list", Governance, INTERNAL_ORG),
            ActionSpec::new("manage", Governance, INTERNAL_ORG),
        ],
    )
    .with_all_members(),
    ConsumerSpec::new(
        "customer",
        &[
            ActionSpec::new("list", Business, INTERNAL_ORG).with_self().with_history(),
            ActionSpec::new("detail", Business, INTERNAL_ORG).with_self().with_history(),
            ActionSpec::new("create", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("update", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("delete", Business, INTERNAL_ORG).with_self(),
        ],
    ),
    ConsumerSpec::new(
        "contract",
        &[
            ActionSpec::new("list", SourceInherited, INTERNAL_ORG).with_history(),
            ActionSpec::new("detail", SourceInherited, INTERNAL_ORG).with_history(),
            ActionSpec::new("create", SourceInherited, INTERNAL_ORG),
            ActionSpec::new("update", SourceInherited, INTERNAL_ORG),
        ],
    )
    .with_source(ScopeSource::Fixed { resource: "customer", action: "list" }),
    ConsumerSpec::new(
        "sales_order",
        &[
            ActionSpec::new("list", Business, INTERNAL_ORG).with_self().with_history(),
            ActionSpec::new("detail", Business, INTERNAL_ORG).with_self().with_history(),
            ActionSpec::new("create", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("update", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("delete", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("submit", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("cancel_approval", Business, INTERNAL_ORG).with_self(),
        ],
    ),
    ConsumerSpec::new(
        "sales_inquiry",
        &[
            ActionSpec::new("list", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("detail", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("create", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("update", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("submit", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("accept", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("reinquire", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("close", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("import", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("convert_sales", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("recover", Business, INTERNAL_ORG).with_self(),
        ],
    ),
    ConsumerSpec::new(
        "procurement_inquiry",
        &[
            ActionSpec::new("list", Task, INTERNAL_ORG),
            ActionSpec::new("detail", Task, INTERNAL_ORG),
            ActionSpec::new("claim", Task, INTERNAL_ORG),
            ActionSpec::new("supplement", Task, INTERNAL_ORG),
            ActionSpec::new("reassign", Task, INTERNAL_ORG),
            ActionSpec::new("internal_quote", Task, INTERNAL_ORG),
            ActionSpec::new("save_feedback", Task, INTERNAL_ORG),
            ActionSpec::new("publish", Task, INTERNAL_ORG),
            ActionSpec::new("convert_expenses", Task, INTERNAL_ORG),
            ActionSpec::new("recover", Task, INTERNAL_ORG),
        ],
    ),
    ConsumerSpec::new(
        "purchase_order",
        &[
            ActionSpec::new("list", Shared, INTERNAL_ORG).with_history(),
            ActionSpec::new("detail", Shared, INTERNAL_ORG).with_history(),
            ActionSpec::new("create", Shared, INTERNAL_ORG),
            ActionSpec::new("update", Shared, INTERNAL_ORG),
            ActionSpec::new("delete", Shared, INTERNAL_ORG),
            ActionSpec::new("submit", Shared, INTERNAL_ORG),
            ActionSpec::new("cancel_approval", Shared, INTERNAL_ORG),
            ActionSpec::new("maintain_source", Shared, INTERNAL_ORG),
        ],
    ),
    ConsumerSpec::new(
        "cost_entry",
        &[
            ActionSpec::new("list", SourceInherited, INTERNAL_ORG),
            ActionSpec::new("detail", SourceInherited, INTERNAL_ORG),
        ],
    ),
    ConsumerSpec::new("cost_allocation", &[ActionSpec::new("list", SourceInherited, INTERNAL_ORG)]),
    ConsumerSpec::new(
        "sales_selection_booklet",
        &[
            ActionSpec::new("list", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("get", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("create", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("maintain", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("prepare", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("publish", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("copy_link", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("rotate_link", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("close", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("revoke", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("void", Business, INTERNAL_ORG).with_self(),
        ],
    ),
    ConsumerSpec::new(
        "sales_selection_proposal",
        &[
            ActionSpec::new("list", SourceInherited, INTERNAL_ORG),
            ActionSpec::new("get", SourceInherited, INTERNAL_ORG),
        ],
    )
    .with_source(ScopeSource::SameAction { resource: "sales_selection_booklet" }),
    ConsumerSpec::new(
        "receivable_account",
        &[
            ActionSpec::new("list", SourceInherited, INTERNAL_ORG),
            ActionSpec::new("detail", SourceInherited, INTERNAL_ORG),
        ],
    ),
    ConsumerSpec::new(
        "customer_receipt",
        &[
            ActionSpec::new("list", SourceInherited, INTERNAL_ORG),
            ActionSpec::new("detail", SourceInherited, INTERNAL_ORG),
        ],
    ),
    ConsumerSpec::new(
        "invoice",
        &[
            ActionSpec::new("list", SourceInherited, INTERNAL_ORG),
            ActionSpec::new("detail", SourceInherited, INTERNAL_ORG),
        ],
    ),
    ConsumerSpec::new(
        "sales_invoice_request",
        &[
            ActionSpec::new("list", SourceInherited, INTERNAL_ORG),
            ActionSpec::new("detail", SourceInherited, INTERNAL_ORG),
        ],
    ),
    ConsumerSpec::new(
        "payable_account",
        &[
            ActionSpec::new("list", SourceInherited, INTERNAL_ORG),
            ActionSpec::new("detail", SourceInherited, INTERNAL_ORG),
        ],
    ),
    ConsumerSpec::new(
        "purchase_payment_request",
        &[
            ActionSpec::new("list", SourceInherited, INTERNAL_ORG),
            ActionSpec::new("detail", SourceInherited, INTERNAL_ORG),
            ActionSpec::new("create", SourceInherited, INTERNAL_ORG),
            ActionSpec::new("update", SourceInherited, INTERNAL_ORG),
            ActionSpec::new("submit", SourceInherited, INTERNAL_ORG),
            ActionSpec::new("withdraw", SourceInherited, INTERNAL_ORG),
            ActionSpec::new("close", SourceInherited, INTERNAL_ORG),
            ActionSpec::new("pay", Task, INTERNAL_ORG),
        ],
    ),
    ConsumerSpec::new(
        "supplier_payment",
        &[
            ActionSpec::new("list", SourceInherited, INTERNAL_ORG),
            ActionSpec::new("detail", SourceInherited, INTERNAL_ORG),
        ],
    ),
    ConsumerSpec::new(
        "supplier_deposit",
        &[
            ActionSpec::new("list", Business, DEPOSIT_DIMENSIONS),
            ActionSpec::new("detail", Business, DEPOSIT_DIMENSIONS),
            ActionSpec::new("create", Business, DEPOSIT_DIMENSIONS),
            ActionSpec::new("recharge_request", Business, DEPOSIT_DIMENSIONS),
            ActionSpec::new("approve_recharge", Business, DEPOSIT_DIMENSIONS),
            ActionSpec::new("pay_recharge", Business, DEPOSIT_DIMENSIONS),
            ActionSpec::new("receive_recharge", Business, DEPOSIT_DIMENSIONS),
            ActionSpec::new("authorize", Business, DEPOSIT_DIMENSIONS),
            ActionSpec::new("revoke", Business, DEPOSIT_DIMENSIONS),
            ActionSpec::new("reconcile", Business, DEPOSIT_DIMENSIONS),
            ActionSpec::new("resolve", Business, DEPOSIT_DIMENSIONS),
            ActionSpec::new("release", Business, DEPOSIT_DIMENSIONS),
            ActionSpec::new("reserve", Business, DEPOSIT_DIMENSIONS),
            ActionSpec::new("use", Business, DEPOSIT_DIMENSIONS),
            ActionSpec::new("refund", Business, DEPOSIT_DIMENSIONS),
            ActionSpec::new("bind", Business, DEPOSIT_DIMENSIONS),
        ],
    ),
    ConsumerSpec::new(
        "purchase_invoice_allocation",
        &[ActionSpec::new("list", SourceInherited, INTERNAL_ORG)],
    ),
    ConsumerSpec::new(
        "integration_error_task",
        &[
            ActionSpec::new("list", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("detail", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("create", Role, INTERNAL_ORG),
        ],
    ),
    ConsumerSpec::new(
        "reconciliation_difference",
        &[
            ActionSpec::new("list", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("detail", Business, INTERNAL_ORG).with_self(),
            ActionSpec::new("create", Role, INTERNAL_ORG),
            ActionSpec::new("decide", Task, INTERNAL_ORG),
        ],
    ),
    ConsumerSpec::new(
        "supplier",
        &[
            ActionSpec::new("list", Shared, INTERNAL_ORG),
            ActionSpec::new("detail", Shared, INTERNAL_ORG),
            ActionSpec::new("create", Shared, INTERNAL_ORG),
            ActionSpec::new("update", Shared, INTERNAL_ORG),
            ActionSpec::new("delete", Shared, INTERNAL_ORG),
        ],
    ),
    ConsumerSpec::new(
        "product",
        &[
            ActionSpec::new("list", Shared, INTERNAL_ORG),
            ActionSpec::new("detail", Shared, INTERNAL_ORG),
            ActionSpec::new("create", Shared, INTERNAL_ORG),
            ActionSpec::new("update", Shared, INTERNAL_ORG),
        ],
    ),
    ConsumerSpec::new(
        "supplier_offering",
        &[
            ActionSpec::new("list", Shared, INTERNAL_ORG),
            ActionSpec::new("create", Shared, INTERNAL_ORG),
            ActionSpec::new("update", Shared, INTERNAL_ORG),
        ],
    ),
    ConsumerSpec::new(
        "supplier_fulfillment_order",
        &[
            ActionSpec::new("list", Shared, INTERNAL_ORG),
            ActionSpec::new("detail", Shared, INTERNAL_ORG),
            ActionSpec::new("investigate", Shared, INTERNAL_ORG),
            ActionSpec::new("complete", Shared, INTERNAL_ORG),
            ActionSpec::new("submit", Shared, INTERNAL_ORG),
            ActionSpec::new("cancel", Shared, INTERNAL_ORG),
            ActionSpec::new("refund", Shared, INTERNAL_ORG),
            ActionSpec::new("reject", Shared, INTERNAL_ORG),
            ActionSpec::new("handover", Shared, INTERNAL_ORG),
        ],
    ),
    ConsumerSpec::new("person_query_qualification", &[ActionSpec::new("manage", Governance, INTERNAL_ORG)])
        .with_scope_types(PERSON_TYPES),
    ConsumerSpec::new("warehouse", &[ActionSpec::new("list", Directory, WAREHOUSE)])
        .with_scope_types(OBJECT_TYPES),
    ConsumerSpec::new("business_person", &[ActionSpec::new("list", Directory, INTERNAL_ORG)])
        .with_scope_types(PERSON_TYPES),
    ConsumerSpec::new("sales_person", &[ActionSpec::new("list", Directory, INTERNAL_ORG)])
        .with_scope_types(PERSON_TYPES),
    ConsumerSpec::new("procurement_person", &[ActionSpec::new("list", Directory, INTERNAL_ORG)])
        .with_scope_types(PERSON_TYPES),
];

impl ActionSpec {
    /// 声明动作基础规则；本人基础和历史读取必须显式开启。
    const fn new(
        action: &'static str,
        policy: AuthorizationPolicy,
        dimensions: &'static [ScopeDimension],
    ) -> Self {
        Self {
            action,
            policy,
            default_self: false,
            supported_dimensions: dimensions,
            required_dimensions: dimensions,
            allows_history: false,
        }
    }
    /// 声明已证明动作权限后的本人基础。
    const fn with_self(mut self) -> Self {
        self.default_self = true;
        self
    }
    /// 声明读取可以由合法历史参与补充。
    const fn with_history(mut self) -> Self {
        self.allows_history = true;
        self
    }
    /// 声明各支持维度均为可选事实。
    const fn with_optional_dimensions(mut self) -> Self {
        self.required_dimensions = &[];
        self
    }
    /// 取得动作代码。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前动作的动作代码。
    /// # 错误
    /// 不返回错误。
    pub fn action(&self) -> &'static str {
        self.action
    }
    /// 取得授权策略。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前动作的授权策略。
    /// # 错误
    /// 不返回错误。
    pub fn policy(&self) -> AuthorizationPolicy {
        self.policy
    }
    /// 取得本人基础是否生效。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前动作的本人基础是否生效。
    /// # 错误
    /// 不返回错误。
    pub fn default_self(&self) -> bool {
        self.default_self
    }
    /// 取得支持的范围维度。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前动作的支持的范围维度。
    /// # 错误
    /// 不返回错误。
    pub fn supported_dimensions(&self) -> &'static [ScopeDimension] {
        self.supported_dimensions
    }
    /// 取得解析必须满足的维度。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前动作的解析必须满足的维度。
    /// # 错误
    /// 不返回错误。
    pub fn required_dimensions(&self) -> &'static [ScopeDimension] {
        self.required_dimensions
    }
    /// 取得是否允许合法历史参与补充读取。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前动作的是否允许合法历史参与补充读取。
    /// # 错误
    /// 不返回错误。
    pub fn allows_history(&self) -> bool {
        self.allows_history
    }
}

impl ConsumerSpec {
    /// 声明资源，默认使用本资源范围、全部范围类型及操作人成员事实。
    const fn new(resource: &'static str, actions: &'static [ActionSpec]) -> Self {
        Self {
            resource,
            actions,
            scope_source: ScopeSource::Current,
            scope_types: ALL_TYPES,
            organization_facts: OrganizationFacts::Actor,
        }
    }
    /// 声明范围条款来源。
    const fn with_source(mut self, source: ScopeSource) -> Self {
        self.scope_source = source;
        self
    }
    /// 声明可接受的范围类型。
    const fn with_scope_types(mut self, types: &'static [DataScopeType]) -> Self {
        self.scope_types = types;
        self
    }
    /// 声明治理解析需要全员组织关系。
    const fn with_all_members(mut self) -> Self {
        self.organization_facts = OrganizationFacts::AllMembers;
        self
    }
    /// 查找已纳入范围目录的资源；用于区分未知资源和已知资源的未知动作。
    /// # 参数
    /// `resource` 为资源代码。
    /// # 返回
    /// 已登记资源的静态规格；未知资源返回 None。
    /// # 错误
    /// 不返回错误。
    pub(crate) fn find(resource: &str) -> Option<&'static Self> {
        CONSUMERS.iter().find(|entry| entry.resource == resource)
    }
    /// 取得资源规格；未登记资源失败关闭。
    /// # 参数
    /// `resource` 为资源代码。
    /// # 返回
    /// 已登记资源的静态规格。
    /// # 错误
    /// 未知资源返回校验错误。
    pub(crate) fn get(resource: &str) -> Result<&'static Self> {
        Self::find(resource).ok_or_else(|| Error::ValidationError(format!("{resource} 未登记授权策略")))
    }
    /// 取得资源代码。
    /// # 参数
    /// 无。
    /// # 返回
    /// 资源代码。
    /// # 错误
    /// 不返回错误。
    pub(crate) fn resource(&self) -> &'static str {
        self.resource
    }
    /// 取得全部动作规格。
    /// # 参数
    /// 无。
    /// # 返回
    /// 逐动作规格的只读切片。
    /// # 错误
    /// 不返回错误。
    pub(crate) fn actions(&self) -> &'static [ActionSpec] {
        self.actions
    }
    /// 取得解析需要的组织事实。
    /// # 参数
    /// 无。
    /// # 返回
    /// 成员事实加载要求。
    /// # 错误
    /// 不返回错误。
    pub(crate) fn organization_facts(&self) -> OrganizationFacts {
        self.organization_facts
    }
    /// 取得资源全部动作支持维度的并集，仅供配置候选展示。
    /// # 参数
    /// 无。
    /// # 返回
    /// 去重后的展示维度。
    /// # 错误
    /// 不返回错误。
    pub(crate) fn dimensions(&self) -> Vec<ScopeDimension> {
        let mut dimensions = Vec::new();
        for action in self.actions {
            for dimension in action.supported_dimensions {
                if !dimensions.contains(dimension) {
                    dimensions.push(*dimension);
                }
            }
        }
        dimensions
    }
    /// 解析已登记动作的范围条款来源，固定动作与沿用动作分别处理。
    /// # 参数
    /// `permission` 为入口权限。
    /// # 返回
    /// 已登记的来源权限。
    /// # 错误
    /// 入口或来源未登记时返回校验错误。
    pub(crate) fn scope_source(permission: &Permission) -> Result<Permission> {
        registration(permission.resource(), permission.action())?;
        let source = match Self::get(permission.resource())?.scope_source {
            ScopeSource::Current => permission.clone(),
            ScopeSource::Fixed { resource, action } => Permission::parse(format!("{resource}:{action}"))?,
            ScopeSource::SameAction { resource } => {
                Permission::parse(format!("{resource}:{}", permission.action()))?
            },
        };
        registration(source.resource(), source.action())?;
        Ok(source)
    }
}

/// 按真实消费者登记查找资源动作。
///
/// # 参数
/// * `resource` - 业务资源
/// * `action` - 已注册动作
///
/// # 返回
/// 返回该资源动作的消费者登记，含默认本人范围、支持维度、必需维度和历史参与。
///
/// # 错误
/// 未接线资源或动作返回校验错误。
///
/// # 关键业务约束
/// 不得把初始化清单当作消费者已接入的证据。
pub fn registration(resource: &str, action: &str) -> Result<&'static ActionSpec> {
    ConsumerSpec::find(resource)
        .and_then(|entry| entry.actions.iter().find(|entry| entry.action == action))
        .ok_or_else(|| Error::ValidationError(format!("{resource}:{action} 尚未接入 DataScope v2")))
}

/// 拒绝对象目录和人员目录不接受的范围类型。
///
/// # 参数
/// * `resource` - 业务资源
/// * `scope_type` - 待写入或已存储的范围类型
///
/// # 返回
/// 范围类型被该资源接受时无返回值。
///
/// # 错误
/// `warehouse` 在类型不是 `Company` 或 `Organization` 时返回校验错误。
/// `sales_person`、`procurement_person`、`business_person` 与 `person_query_qualification` 配置 `Collaborative` 时返回校验错误。
///
/// # 关键业务约束
/// 协作参与不能从业务单据扩张出人员读取权。配置入口和解析都必须拒绝。
pub fn validate_scope_type(resource: &str, scope_type: DataScopeType) -> Result<()> {
    let spec = ConsumerSpec::get(resource)?;
    if !spec.scope_types.contains(&scope_type) {
        return Err(Error::ValidationError(format!("{resource} 不接受 {scope_type:?} 范围")));
    }
    Ok(())
}

/// 对配置、初始化及已存储规则执行相同的消费者准入。
///
/// # 参数
/// * `binding` - 已通过模型形态校验的资源动作绑定。
///
/// # 返回
/// 全部动作及目标维度已接线时无返回值。
///
/// # 错误
/// 任一动作未接线或维度不支持时返回校验错误，停用规则也不能绕过准入。
pub fn validate_binding(binding: &ScopeBinding) -> Result<()> {
    for action in &binding.actions {
        let consumer = registration(&binding.resource, action)?;
        if !consumer.supported_dimensions.contains(&binding.target_dimension) {
            return Err(Error::ValidationError(format!(
                "{}:{} 不支持 {:?} 范围维度",
                binding.resource, action, binding.target_dimension
            )));
        }
    }
    Ok(())
}

/// 校验独立范围配置准入；运行消费者登记不等于可编辑配置。
/// # 参数
/// `resource` 与 `action` 为待写入的资源动作。
/// # 返回
/// 可配置动作的消费者登记。
/// # 错误
/// 未接线或采用来源、任务、角色授权的动作拒绝。
pub fn configurable_registration(resource: &str, action: &str) -> Result<&'static ActionSpec> {
    let consumer = registration(resource, action)?;
    consumer.policy.ensure_configurable()?;
    Ok(consumer)
}

/// 运行时范围来源接受人员范围及岗位共享，任务和来源继承必须由业务提供真实来源。
/// # 参数
/// `resource` 与 `action` 为本次授权的实际范围来源。
/// # 返回
/// 已登记且能直接解析的消费者。
/// # 错误
/// 未登记、任务、角色或间接来源策略拒绝，不以共享范围兜底。
pub(crate) fn scope_registration(resource: &str, action: &str) -> Result<&'static ActionSpec> {
    let consumer = registration(resource, action)?;
    let policy = consumer.policy;
    if policy != AuthorizationPolicy::Shared {
        policy.ensure_configurable()?;
    }
    Ok(consumer)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    /// 全部动作均可唯一查找，规格之间不存在矛盾，来源引用可解析。
    #[test]
    fn catalog_is_unique_and_all_sources_are_registered() {
        let mut resources = BTreeSet::new();
        for spec in CONSUMERS {
            assert!(resources.insert(spec.resource));
            assert!(!spec.actions.is_empty());
            let mut actions = BTreeSet::new();
            for action in spec.actions {
                assert!(actions.insert(action.action));
                assert!(!action.supported_dimensions.is_empty());
                assert!(
                    action
                        .required_dimensions
                        .iter()
                        .all(|dimension| { action.supported_dimensions.contains(dimension) })
                );
                if action.default_self {
                    assert!(action.policy.configurable());
                    assert_eq!(action.required_dimensions, INTERNAL_ORG);
                }
                if action.allows_history {
                    assert!(matches!(action.action, "list" | "detail"));
                    assert_eq!(action.required_dimensions, INTERNAL_ORG);
                }
                let permission = Permission::parse(format!("{}:{}", spec.resource, action.action)).unwrap();
                let source = ConsumerSpec::scope_source(&permission).unwrap();
                let registered = registration(source.resource(), source.action()).unwrap();
                if !matches!(spec.scope_source, ScopeSource::Current) {
                    assert!(registered.policy.configurable());
                    assert_eq!(ConsumerSpec::scope_source(&source).unwrap(), source);
                }
            }
        }
    }

    /// 同资源策略按动作生效，未知动作不继承其他动作的策略。
    #[test]
    fn mixed_policies_and_unknown_actions_remain_distinct() {
        for (resource, action, expected) in [
            ("supplier_settlement_statement", "list", Shared),
            ("supplier_settlement_statement", "confirm", Task),
            ("purchase_payment_request", "submit", SourceInherited),
            ("purchase_payment_request", "pay", Task),
            ("reconciliation_difference", "list", Business),
            ("reconciliation_difference", "create", Role),
            ("reconciliation_difference", "decide", Task),
        ] {
            assert_eq!(registration(resource, action).unwrap().policy(), expected);
            assert_eq!(configurable_registration(resource, action).is_ok(), expected.configurable());
            assert!(registration(resource, "unknown").is_err());
            assert!(AuthorizationPolicy::for_action(resource, "unknown").is_err());
        }
        assert!(registration("unknown", "list").is_err());
        assert!(validate_scope_type("unknown", DataScopeType::Company).is_err());
    }

    /// 组织事实加载与范围类型限制按各自合同声明。
    #[test]
    fn fact_requirements_do_not_imply_scope_type_permissions() {
        for resource in ["org_unit", "work_item"] {
            assert_eq!(
                ConsumerSpec::get(resource).unwrap().organization_facts(),
                OrganizationFacts::AllMembers
            );
        }
        assert_eq!(ConsumerSpec::get("customer").unwrap().organization_facts(), OrganizationFacts::Actor);
        assert!(validate_scope_type("warehouse", DataScopeType::Organization).is_ok());
        assert!(validate_scope_type("warehouse", DataScopeType::SelfOwned).is_err());
        assert!(validate_scope_type("person_query_qualification", DataScopeType::SelfOwned).is_ok());
        assert!(validate_scope_type("person_query_qualification", DataScopeType::Collaborative).is_err());
        assert!(validate_scope_type("customer", DataScopeType::Collaborative).is_ok());
        let approval = registration("approval_instance", "read").unwrap();
        assert_eq!(approval.supported_dimensions(), APPROVAL_DIMENSIONS);
        assert!(approval.required_dimensions().is_empty());
    }
}
