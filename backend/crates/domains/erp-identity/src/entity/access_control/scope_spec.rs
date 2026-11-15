//! 按权限保存的内部范围定义。配置、解析和导入导出共用这一份。
mod catalog;

use erp_permissions::MODULES;

use crate::entity::access_control::{DataScopeType, ScopeBinding, ScopeDimension};
use crate::{Error, Permission, Result};

/// 范围配置的取得方式。不表达任务、功能资格或真实来源关系。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScopeSource {
    /// 使用本权限自己的人员配置。
    Current,
    /// 复用一个 `Current` 权限的配置；本权限不接受独立配置。
    Permission(Permission),
    /// 功能资格成立后采用公司范围，不读取人员配置。
    Company,
}

/// 一个权限的范围合同。字段只由静态目录填写。
#[derive(Debug, Clone)]
pub(crate) struct ScopeSpec {
    permission: Permission,
    source: ScopeSource,
    default_self: bool,
    scope_types: &'static [DataScopeType],
    supported_dimensions: &'static [ScopeDimension],
    required_dimensions: &'static [ScopeDimension],
    allows_history: bool,
}

/// 没有人员范围的已接入动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActionKind {
    Scope,
    Task,
    Function,
    Source,
}

pub(super) struct FixedAction {
    permission: Permission,
    kind: ActionKind,
}

/// 组织事实加载由本次范围计算决定，不写进范围定义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OrganizationLoad {
    None,
    Actor,
    AllMembers,
}

pub(super) fn scope(
    permission: &Permission,
    source: ScopeSource,
    default_self: bool,
    scope_types: &'static [DataScopeType],
    dimensions: &'static [ScopeDimension],
    allows_history: bool,
) -> ScopeSpec {
    ScopeSpec {
        permission: permission.clone(),
        source,
        default_self,
        scope_types,
        supported_dimensions: dimensions,
        required_dimensions: dimensions,
        allows_history,
    }
}

impl ScopeSpec {
    pub(crate) fn permission(&self) -> &Permission {
        &self.permission
    }

    pub(crate) fn source(&self) -> &ScopeSource {
        &self.source
    }

    pub(crate) fn default_self(&self) -> bool {
        self.default_self
    }

    pub(crate) fn scope_types(&self) -> &'static [DataScopeType] {
        self.scope_types
    }

    pub(crate) fn supported_dimensions(&self) -> &'static [ScopeDimension] {
        self.supported_dimensions
    }

    pub(crate) fn required_dimensions(&self) -> &'static [ScopeDimension] {
        self.required_dimensions
    }

    pub(crate) fn allows_history(&self) -> bool {
        self.allows_history
    }

    pub(crate) fn configurable(&self) -> bool {
        matches!(self.source, ScopeSource::Current)
    }
}

fn raw_specs() -> &'static [ScopeSpec] {
    catalog::SPECS.as_slice()
}

pub(crate) fn specs() -> &'static [ScopeSpec] {
    static CHECKED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    CHECKED.get_or_init(|| {
        validate_catalog().expect("范围目录来源、维度或分类无效");
    });
    raw_specs()
}

pub(crate) fn find(permission: &Permission) -> Option<&'static ScopeSpec> {
    raw_specs().iter().find(|spec| spec.permission == *permission)
}

pub(crate) fn find_action(resource: &str, action: &str) -> Option<&'static ScopeSpec> {
    raw_specs()
        .iter()
        .find(|spec| spec.permission.resource() == resource && spec.permission.action() == action)
}

/// 已接入但没有范围定义时说明原因；完全未知的动作保持原接入错误。
pub(crate) fn require_scope(permission: &Permission) -> Result<&'static ScopeSpec> {
    let _ = specs();
    if let Some(spec) = find(permission) {
        return Ok(spec);
    }
    if kind_of(permission).is_some() {
        return Err(independent_scope_rejected(permission));
    }
    Err(unwired(permission))
}

pub(crate) fn require_action(resource: &str, action: &str) -> Result<&'static ScopeSpec> {
    let _ = specs();
    if let Some(spec) = find_action(resource, action) {
        return Ok(spec);
    }
    if action_kind(resource, action).is_some() {
        return Err(Error::ValidationError(format!(
            "此操作不接受独立数据范围：{}",
            action_description(resource, action)?
        )));
    }
    Err(Error::ValidationError(format!("{resource}:{action} 尚未接入 DataScope v2")))
}

/// 运行时直接解析的来源：本权限配置或公司范围。来源复用必须先跳到 `Current`。
pub(crate) fn require_runtime(permission: &Permission) -> Result<&'static ScopeSpec> {
    let spec = require_scope(permission)?;
    runtime_only(spec)
}

pub(crate) fn require_runtime_action(resource: &str, action: &str) -> Result<&'static ScopeSpec> {
    runtime_only(require_action(resource, action)?)
}

fn runtime_only(spec: &'static ScopeSpec) -> Result<&'static ScopeSpec> {
    match &spec.source {
        ScopeSource::Current | ScopeSource::Company => Ok(spec),
        ScopeSource::Permission(_) => Err(independent_scope_rejected(&spec.permission)),
    }
}

/// 解析实际读取人员配置或公司范围的定义。来源必须是 `Current`，且不能成环。
pub(crate) fn runtime_source(spec: &ScopeSpec) -> Result<&'static ScopeSpec> {
    match &spec.source {
        ScopeSource::Current | ScopeSource::Company => find(&spec.permission)
            .ok_or_else(|| Error::ValidationError(format!("{} 的范围定义不在目录中", spec.permission))),
        ScopeSource::Permission(permission) => {
            let source = find(permission)
                .ok_or_else(|| Error::ValidationError(format!("{} 的范围来源未登记", spec.permission)))?;
            if !matches!(source.source, ScopeSource::Current) || source.permission == spec.permission {
                return Err(Error::ValidationError(format!("{} 的范围来源无效或成环", spec.permission)));
            }
            Ok(source)
        },
    }
}

pub(crate) fn action_kind(resource: &str, action: &str) -> Option<ActionKind> {
    if find_action(resource, action).is_some() {
        return Some(ActionKind::Scope);
    }
    catalog::FIXED_ACTIONS
        .iter()
        .find(|entry| entry.permission.resource() == resource && entry.permission.action() == action)
        .map(|entry| entry.kind)
}

pub(crate) fn kind(permission: &Permission) -> Result<ActionKind> {
    kind_of(permission).ok_or_else(|| unwired(permission))
}

pub(crate) fn configurable_action(resource: &str, action: &str) -> Result<&'static ScopeSpec> {
    let spec = require_action(resource, action)?;
    if spec.configurable() {
        return Ok(spec);
    }
    Err(independent_scope_rejected(spec.permission()))
}

pub(crate) fn is_configurable(resource: &str, action: &str) -> bool {
    find_action(resource, action).is_some_and(ScopeSpec::configurable)
}

pub(crate) fn resource_is_known(resource: &str) -> bool {
    raw_specs().iter().any(|spec| spec.permission.resource() == resource)
        || catalog::FIXED_ACTIONS.iter().any(|entry| entry.permission.resource() == resource)
}

pub(crate) fn actions(resource: &str) -> Vec<&'static Permission> {
    let mut actions = raw_specs()
        .iter()
        .filter(|spec| spec.permission.resource() == resource)
        .map(|spec| spec.permission())
        .collect::<Vec<_>>();
    actions.extend(
        catalog::FIXED_ACTIONS
            .iter()
            .filter(|entry| entry.permission.resource() == resource)
            .map(|entry| &entry.permission),
    );
    actions
}

/// 人员范围界面只展示公共目录动作，内部固定入口不参与配置候选。
pub(crate) fn display_actions(resource: &str) -> Vec<&'static Permission> {
    actions(resource)
        .into_iter()
        .filter(|permission| {
            MODULES.iter().flat_map(|module| module.entries).any(|(_, entry)| entry.permission == *permission)
        })
        .collect()
}

pub(crate) fn resources() -> Vec<&'static str> {
    let _ = specs();
    let mut resources = Vec::new();
    for permission in raw_specs()
        .iter()
        .map(|spec| spec.permission())
        .chain(catalog::FIXED_ACTIONS.iter().map(|entry| &entry.permission))
    {
        if !resources.contains(&permission.resource()) {
            resources.push(permission.resource());
        }
    }
    resources
}

pub(crate) fn configurable_dimensions(resource: &str, actions: &[String]) -> Vec<ScopeDimension> {
    let mut dimensions = Vec::new();
    for action in actions {
        let Some(spec) = find_action(resource, action).filter(|spec| spec.configurable()) else {
            continue;
        };
        for dimension in spec.supported_dimensions {
            if !dimensions.contains(dimension) {
                dimensions.push(*dimension);
            }
        }
    }
    dimensions
}

pub(crate) fn validate_scope_type(resource: &str, scope_type: DataScopeType) -> Result<()> {
    let spec = raw_specs()
        .iter()
        .find(|spec| spec.permission.resource() == resource)
        .ok_or_else(|| Error::ValidationError(format!("{resource} 未登记授权策略")))?;
    if !spec.scope_types().contains(&scope_type) {
        return Err(Error::ValidationError(format!("{resource} 不接受 {scope_type:?} 范围")));
    }
    Ok(())
}

pub(crate) fn validate_binding(binding: &ScopeBinding) -> Result<()> {
    for action in &binding.actions {
        let spec = require_action(&binding.resource, action)?;
        if !spec.supported_dimensions.contains(&binding.target_dimension) {
            return Err(Error::ValidationError(format!(
                "{}:{} 不支持 {:?} 范围维度",
                binding.resource, action, binding.target_dimension
            )));
        }
    }
    Ok(())
}

/// 公司范围不加载组织；治理委派需要全员关系，其余只加载操作人目录。
pub(crate) fn organization_load(spec: &ScopeSpec) -> OrganizationLoad {
    if matches!(&spec.source, ScopeSource::Company) {
        return OrganizationLoad::None;
    }
    if matches!(spec.permission.resource(), "org_unit" | "work_item") {
        OrganizationLoad::AllMembers
    } else {
        OrganizationLoad::Actor
    }
}

pub(crate) fn action_description(resource: &str, action: &str) -> Result<&'static str> {
    let kind = action_kind(resource, action)
        .ok_or_else(|| Error::ValidationError(format!("{resource}:{action} 尚未接入 DataScope v2")))?;
    Ok(describe(resource, find_action(resource, action), kind))
}

pub(crate) fn policy_description(resource: &str, actions: &[String]) -> Result<String> {
    if actions.is_empty() {
        return Err(Error::ValidationError("缺少操作权限".into()));
    }
    let texts = actions
        .iter()
        .map(|action| action_description(resource, action).map(|text| (action.as_str(), text)))
        .collect::<Result<Vec<_>>>()?;
    let first = texts[0].1;
    if texts.iter().all(|(_, text)| *text == first) {
        return Ok(first.to_owned());
    }
    Ok(texts.iter().map(|(action, text)| format!("{action}：{text}")).collect::<Vec<_>>().join("；"))
}

pub(crate) fn validate_catalog() -> Result<()> {
    let mut seen = Vec::new();
    for spec in raw_specs() {
        let key = (spec.permission.resource(), spec.permission.action());
        if seen.contains(&key) {
            return Err(Error::ValidationError(format!("{} 范围定义重复", spec.permission)));
        }
        seen.push(key);
        validate_scope(spec)?;
    }
    for entry in catalog::FIXED_ACTIONS.iter() {
        if find(&entry.permission).is_some() {
            return Err(Error::ValidationError(format!("{} 同时声明了范围和无范围动作", entry.permission)));
        }
        if entry.kind == ActionKind::Scope {
            return Err(Error::ValidationError(format!("{} 的无范围分类无效", entry.permission)));
        }
    }
    validate_sources_acyclic(raw_specs())
}

/// 校验单项范围定义的维度、读取补充与来源合同。
fn validate_scope(spec: &ScopeSpec) -> Result<()> {
    if spec.supported_dimensions.is_empty() || spec.scope_types.is_empty() {
        return Err(Error::ValidationError(format!("{} 缺少范围维度或类型", spec.permission)));
    }
    if spec.required_dimensions.iter().any(|dimension| !spec.supported_dimensions.contains(dimension)) {
        return Err(Error::ValidationError(format!("{} 的必需维度超出支持维度", spec.permission)));
    }
    if spec.default_self && !matches!(&spec.source, ScopeSource::Current) {
        return Err(Error::ValidationError(format!("{} 不能声明本人基础", spec.permission)));
    }
    if spec.default_self && spec.required_dimensions != [ScopeDimension::InternalOrg] {
        return Err(Error::ValidationError(format!("{} 的本人基础只能使用内部组织", spec.permission)));
    }
    if spec.allows_history
        && !matches!(
            spec.permission.action(),
            "list" | "detail" | "financial_expense_source" | "profit_loss_read" | "extra_cost_read"
        )
    {
        return Err(Error::ValidationError(format!("{} 的历史读取只适用于读取", spec.permission)));
    }
    if spec.allows_history && spec.required_dimensions != [ScopeDimension::InternalOrg] {
        return Err(Error::ValidationError(format!("{} 的历史读取只能使用内部组织", spec.permission)));
    }
    let source = runtime_source(spec)?;
    if let ScopeSource::Permission(_) = &spec.source
        && source.supported_dimensions.iter().any(|dimension| !spec.supported_dimensions.contains(dimension))
    {
        return Err(Error::ValidationError(format!(
            "{} 不支持来源 {} 的范围维度",
            spec.permission, source.permission
        )));
    }
    validate_scope_types(spec)
}

/// 同资源的范围类型保持一致，逐动作维度仍独立校验。
fn validate_scope_types(spec: &ScopeSpec) -> Result<()> {
    let siblings =
        raw_specs().iter().filter(|other| other.permission.resource() == spec.permission.resource());
    if siblings.clone().any(|other| other.scope_types != spec.scope_types) {
        return Err(Error::ValidationError(format!("{} 的范围类型不一致", spec.permission.resource())));
    }
    Ok(())
}

fn kind_of(permission: &Permission) -> Option<ActionKind> {
    action_kind(permission.resource(), permission.action())
}

fn unwired(permission: &Permission) -> Error {
    Error::ValidationError(format!("{permission} 尚未接入 DataScope v2"))
}

fn independent_scope_rejected(permission: &Permission) -> Error {
    let text = action_description(permission.resource(), permission.action())
        .unwrap_or("此动作没有独立数据范围配置");
    Error::ValidationError(format!("此操作不接受独立数据范围：{text}"))
}

fn describe(resource: &str, spec: Option<&ScopeSpec>, kind: ActionKind) -> &'static str {
    match resource {
        "work_item" => {
            "列表和详情默认包含本人负责的任务，扩大读取范围需追加授权；任务管理必须显式配置范围，各动作仍需具备对应操作权限。"
        },
        "stock_balance" | "stock_movement" | "stock_reservation" => {
            "采用库存仓库范围政策；余额、流水、预占及各操作的已有范围分别保留，不自动合并，也不使用仓库目录的可见范围。"
        },
        "contract" => {
            "复用客户的范围条款设置，按合同自己的负责销售、组织及明确共享解释；共同客户不授予合同访问权，仍需具备合同操作权限，不单独配置合同范围。"
        },
        "sales_selection_proposal" => {
            "继承当前所属选品册的责任范围，仍需具备选品方案操作权限；不单独配置方案范围。"
        },
        _ => match (spec.map(ScopeSpec::source), kind) {
            (Some(ScopeSource::Company), _) => {
                "具备对应操作权限的岗位共享公司业务，不按人员或部门配置范围；维护权限、敏感信息、单据状态及任务责任分别校验。"
            },
            (Some(ScopeSource::Permission(_)), _) | (None, ActionKind::Source) => {
                "继承来源业务的可见范围；来源、财务职责或关联任务由业务核验，不单独配置范围。"
            },
            (Some(ScopeSource::Current), _)
                if matches!(resource, "org_unit" | "person_query_qualification") =>
            {
                "配置管理委派边界；部门关系和业务数据权限不能替代管理授权。"
            },
            (Some(ScopeSource::Current), _)
                if matches!(
                    resource,
                    "warehouse" | "business_person" | "sales_person" | "procurement_person"
                ) =>
            {
                "仅配置目录候选的可见范围；不授予关联业务的数据或操作权限。"
            },
            (Some(ScopeSource::Current), _) => "按业务责任和附加范围授权；仍需具备对应操作权限。",
            (None, ActionKind::Task) => "由流程任务、参与关系及管理资格核验；不单独配置数据范围。",
            (None, ActionKind::Function) => "由操作权限及业务责任校验授权；此动作没有独立数据范围配置。",
            (None, ActionKind::Scope) => "当前资源未登记独立范围策略。",
        },
    }
}

fn validate_sources_acyclic(entries: &[ScopeSpec]) -> Result<()> {
    for spec in entries {
        let mut cursor = spec;
        let mut guard = 0;
        while let ScopeSource::Permission(permission) = &cursor.source {
            guard += 1;
            if guard > entries.len() || permission == &spec.permission {
                return Err(Error::ValidationError(format!("{} 的范围来源成环", spec.permission)));
            }
            cursor = entries
                .iter()
                .find(|entry| entry.permission == *permission)
                .ok_or_else(|| Error::ValidationError(format!("{} 的范围来源缺失", spec.permission)))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::access_control::ScopeDimension;

    #[test]
    fn catalog_sources_are_unique_current_and_acyclic() {
        validate_catalog().unwrap();
        assert!(specs().len() > 100);
        let contract = find_action("contract", "detail").unwrap();
        let source = runtime_source(contract).unwrap();
        assert_eq!(source.permission(), &erp_permissions::customer::LIST);
        assert!(source.configurable());
        assert!(!contract.configurable());
        assert!(!contract.default_self());
        let proposal = find_action("sales_selection_proposal", "get").unwrap();
        assert_eq!(runtime_source(proposal).unwrap().permission().action(), "get");
        assert!(find_action("approval_instance", "decide").is_none());
        assert_eq!(action_kind("approval_instance", "decide"), Some(ActionKind::Task));
        assert!(require_action("unknown", "list").is_err());
        assert!(require_action("approval_instance", "unknown").is_err());
    }

    #[test]
    fn missing_and_cyclic_sources_are_rejected() {
        const LEFT: Permission = Permission::new("scope_cycle_left", "list");
        const RIGHT: Permission = Permission::new("scope_cycle_right", "list");
        const MISSING: Permission = Permission::new("scope_cycle_missing", "list");
        let org: &[ScopeDimension] = &[ScopeDimension::InternalOrg];
        let types: &[DataScopeType] = &[DataScopeType::Company];
        let cycle = [
            scope(&LEFT, ScopeSource::Permission(RIGHT.clone()), false, types, org, false),
            scope(&RIGHT, ScopeSource::Permission(LEFT.clone()), false, types, org, false),
        ];
        assert!(validate_sources_acyclic(&cycle).is_err());
        let missing = [scope(&LEFT, ScopeSource::Permission(MISSING.clone()), false, types, org, false)];
        assert!(validate_sources_acyclic(&missing).is_err());
        let chained = scope(
            &LEFT,
            ScopeSource::Permission(erp_permissions::contract::LIST.clone()),
            false,
            types,
            org,
            false,
        );
        assert!(runtime_source(find(&erp_permissions::contract::LIST).unwrap()).is_ok());
        assert!(runtime_source(&chained).is_err());
    }

    #[test]
    fn configuration_admission_follows_current_source_only() {
        assert!(configurable_action("sales_inquiry", "detail").unwrap().default_self());
        assert!(configurable_action("sales_order", "list").unwrap().default_self());
        assert!(!configurable_action("work_item", "manage").unwrap().default_self());
        assert!(configurable_action("work_item", "list").unwrap().default_self());
        for (resource, action) in [
            ("product", "list"),
            ("supplier", "list"),
            ("purchase_order", "list"),
            ("supplier_settlement_statement", "list"),
            ("contract", "list"),
            ("approval_instance", "decide"),
            ("procurement_inquiry", "publish"),
            ("cost_entry", "list"),
            ("integration_error_task", "create"),
            ("settlement_party", "list"),
        ] {
            assert!(configurable_action(resource, action).is_err(), "{resource}:{action}");
        }
        assert!(matches!(
            require_runtime(&erp_permissions::purchase_order::LIST).unwrap().source(),
            ScopeSource::Company
        ));
        assert!(require_runtime(&erp_permissions::contract::LIST).is_err());
        assert!(validate_scope_type("warehouse", DataScopeType::Organization).is_ok());
        assert!(validate_scope_type("warehouse", DataScopeType::SelfOwned).is_err());
        assert!(validate_scope_type("sales_person", DataScopeType::Collaborative).is_err());
        assert!(validate_scope_type("sales_person", DataScopeType::SelfOwned).is_ok());
        assert!(validate_scope_type("customer", DataScopeType::Collaborative).is_ok());
        let normalized = Permission::parse("SALES_ORDER:DETAIL").unwrap();
        assert_eq!(find(&normalized).unwrap().permission(), &erp_permissions::sales_order::DETAIL);
    }

    #[test]
    fn company_scope_skips_members_and_governance_loads_all() {
        let purchase = find_action("purchase_order", "detail").unwrap();
        assert_eq!(organization_load(purchase), OrganizationLoad::None);
        assert_eq!(organization_load(find_action("customer", "list").unwrap()), OrganizationLoad::Actor);
        assert_eq!(
            organization_load(find_action("work_item", "manage").unwrap()),
            OrganizationLoad::AllMembers
        );
        assert_eq!(organization_load(find_action("org_unit", "list").unwrap()), OrganizationLoad::AllMembers);
        assert_eq!(
            find_action("supplier_deposit", "resolve").unwrap().required_dimensions(),
            &[ScopeDimension::InternalOrg, ScopeDimension::SettlementParty]
        );
    }
}
