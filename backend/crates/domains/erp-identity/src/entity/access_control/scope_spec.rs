//! 按权限保存的内部范围定义。配置、解析和导入导出共用这一份。
mod catalog;

pub(crate) use catalog::INVENTORY_WAREHOUSE_ENTRIES;
use erp_permissions::MODULES;

use crate::entity::access_control::{DataScopeType, ScopeDimension};
use crate::{Error, Permission, Result};

/// 范围配置的取得方式。不表达任务、功能资格或真实来源关系。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScopeSource {
    /// 使用本权限自己的人员配置。
    Current,
    /// 复用一个 `Current` 权限的配置；本权限不接受独立配置。
    Permission(
        /// 提供人员范围配置的已登记权限。
        Permission,
    ),
    /// 功能资格成立后采用公司范围，不读取人员配置。
    Company,
}

/// 静态目录为单个权限登记的范围合同，供配置、运行时解析及导入导出共用。
#[derive(Debug, Clone)]
pub(crate) struct ScopeSpec {
    // 范围规格所绑定的精确权限。
    permission: Permission,
    // 决定读取当前动作配置、复用其他权限范围或采用公司范围。
    source: ScopeSource,
    // 是否为默认配置和附加授权保留本人负责分支。
    default_self: bool,
    // 该业务资源允许选择的范围类型；同资源的动作须一致。
    scope_types: &'static [DataScopeType],
    // 该动作接受的范围维度，显式条款须覆盖全部维度。
    dimensions: &'static [ScopeDimension],
    // 是否允许历史参与作为读取范围的补充分支。
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

/// 不登记范围规格的固定入口，用于区分任务、功能资格和真实来源。
pub(super) struct FixedAction {
    // 固定入口对应的精确权限。
    permission: Permission,
    // 入口的校验类别；不得再次登记为范围入口。
    kind: ActionKind,
}

/// 组织事实加载由本次范围计算决定，不写进范围定义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OrganizationLoad {
    None,
    Actor,
    AllMembers,
}

/// 用固定目录数据构造范围规格。
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
        dimensions,
        allows_history,
    }
}

impl ScopeSpec {
    /// 返回规格绑定的权限，供目录查找和授权入口对照。
    pub(crate) fn permission(&self) -> &Permission {
        &self.permission
    }

    /// 返回范围取得方式，供解析器选择人员配置、复用动作或公司范围。
    pub(crate) fn source(&self) -> &ScopeSource {
        &self.source
    }

    /// 查询此入口是否有本人基础，供无显式人员配置时解析。
    pub(crate) fn default_self(&self) -> bool {
        self.default_self
    }

    /// 返回该资源允许的范围类型，供配置校验。
    pub(crate) fn scope_types(&self) -> &'static [DataScopeType] {
        self.scope_types
    }

    /// 返回该动作支持且要求显式条款覆盖的维度。
    pub(crate) fn dimensions(&self) -> &'static [ScopeDimension] {
        self.dimensions
    }

    /// 查询该入口是否允许把历史参与加入读取条件。
    pub(crate) fn allows_history(&self) -> bool {
        self.allows_history
    }

    /// 仅在使用当前权限的人员配置时允许独立配置。
    pub(crate) fn configurable(&self) -> bool {
        matches!(self.source, ScopeSource::Current)
    }
}

// 借用尚未验证的静态范围目录，供目录验证自身时使用。
fn raw_specs() -> &'static [ScopeSpec] {
    catalog::SPECS.as_slice()
}

/// 首次枚举范围目录时验证登记合同，之后复用已验证的目录；目录错误会终止调用。
pub(crate) fn specs() -> &'static [ScopeSpec] {
    static CHECKED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    CHECKED.get_or_init(|| {
        validate_catalog().expect("范围目录来源、维度或分类无效");
    });
    raw_specs()
}

/// 按完整权限查找范围规格；不含任务等无范围入口。
pub(crate) fn find(permission: &Permission) -> Option<&'static ScopeSpec> {
    raw_specs().iter().find(|spec| spec.permission == *permission)
}

/// 按资源和动作代码查找范围规格，不把无范围入口当作可配置动作。
pub(crate) fn find_action(resource: &str, action: &str) -> Option<&'static ScopeSpec> {
    raw_specs()
        .iter()
        .find(|spec| spec.permission.resource() == resource && spec.permission.action() == action)
}

/// 取得权限的范围规格；区分已接入却不定义范围的入口与未登记入口。
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

/// 从资源和动作取得范围规格；无范围入口返回原因，未知入口返回未开放错误。
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
    Err(scope_unavailable(resource, action))
}

/// 按权限加载可直接计算的范围规格，只接受当前人员配置或公司范围。
pub(crate) fn require_runtime(permission: &Permission) -> Result<&'static ScopeSpec> {
    let spec = require_scope(permission)?;
    match &spec.source {
        ScopeSource::Current | ScopeSource::Company => Ok(spec),
        ScopeSource::Permission(_) => Err(independent_scope_rejected(&spec.permission)),
    }
}

/// 找到实际读取人员配置或采用公司共享的规格；复用只允许指向另一条当前配置，不得成环。
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

/// 按资源和动作识别已登记的范围、任务、功能资格或来源入口；未知动作返回 `None`。
pub(crate) fn action_kind(resource: &str, action: &str) -> Option<ActionKind> {
    if find_action(resource, action).is_some() {
        return Some(ActionKind::Scope);
    }
    catalog::FIXED_ACTIONS
        .iter()
        .find(|entry| entry.permission.resource() == resource && entry.permission.action() == action)
        .map(|entry| entry.kind)
}

/// 按完整权限识别入口分类；未登记时不允许继续授权。
pub(crate) fn kind(permission: &Permission) -> Result<ActionKind> {
    kind_of(permission).ok_or_else(|| unwired(permission))
}

/// 编辑人员范围前确认动作允许独立配置，否则返回范围不可配置的原因。
pub(crate) fn configurable_action(resource: &str, action: &str) -> Result<&'static ScopeSpec> {
    let spec = require_action(resource, action)?;
    if spec.configurable() {
        return Ok(spec);
    }
    Err(independent_scope_rejected(spec.permission()))
}

/// 判断指定动作是否使用自己的人员配置，供目录筛选。
pub(crate) fn is_configurable(resource: &str, action: &str) -> bool {
    find_action(resource, action).is_some_and(ScopeSpec::configurable)
}

/// 判断资源是否已登记至少一个范围或固定动作。
pub(crate) fn resource_is_known(resource: &str) -> bool {
    raw_specs().iter().any(|spec| spec.permission.resource() == resource)
        || catalog::FIXED_ACTIONS.iter().any(|entry| entry.permission.resource() == resource)
}

/// 枚举指定资源的范围动作和固定动作，包括内部入口。
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

/// 为人员范围页面仅枚举公共权限目录中的动作，不展示固定内部入口。
pub(crate) fn display_actions(resource: &str) -> Vec<&'static Permission> {
    actions(resource)
        .into_iter()
        .filter(|permission| {
            MODULES.iter().flat_map(|module| module.entries).any(|(_, entry)| entry.permission == *permission)
        })
        .collect()
}

/// 验证目录后按首次登记顺序枚举资源，去除重复项。
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

/// 合并所选可配置动作支持的维度，保持首次出现顺序；忽略不可配置动作。
pub(crate) fn configurable_dimensions(resource: &str, actions: &[String]) -> Vec<ScopeDimension> {
    let mut dimensions = Vec::new();
    for action in actions {
        let Some(spec) = find_action(resource, action).filter(|spec| spec.configurable()) else {
            continue;
        };
        for dimension in spec.dimensions {
            if !dimensions.contains(dimension) {
                dimensions.push(*dimension);
            }
        }
    }
    dimensions
}

/// 按资源的登记规格校验范围类型，拒绝未开放资源和不支持的类型。
pub(crate) fn validate_scope_type(resource: &str, scope_type: DataScopeType) -> Result<()> {
    let spec = raw_specs().iter().find(|spec| spec.permission.resource() == resource).ok_or_else(|| {
        tracing::debug!(resource, "未登记授权策略");
        Error::ValidationError("该业务模块尚未开放数据范围设置，请联系权限管理员".into())
    })?;
    if !spec.scope_types().contains(&scope_type) {
        return Err(Error::ValidationError(format!(
            "此业务不支持「{}」范围，请选择其他范围类型",
            scope_type.label()
        )));
    }
    Ok(())
}

/// 逐动作校验范围项的目标维度，防止把其他动作的维度用于当前动作。
pub(crate) fn validate_target_dimension(
    resource: &str,
    actions: &[String],
    dimension: ScopeDimension,
) -> Result<()> {
    for action in actions {
        let spec = require_action(resource, action)?;
        if !spec.dimensions.contains(&dimension) {
            let dimension = match dimension {
                ScopeDimension::InternalOrg => "内部组织",
                ScopeDimension::SettlementParty => "结算主体",
                ScopeDimension::Warehouse => "仓库",
            };
            return Err(Error::ValidationError(format!(
                "所选操作不支持按{dimension}设置数据范围，请调整授权范围"
            )));
        }
    }
    Ok(())
}

/// 由范围规格决定组织取证规模：公司范围不加载；组织和工作项治理加载全员；其余加载操作人。
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

/// 按动作返回展示用的范围政策说明，不作为授权条件。
pub(crate) fn action_description(resource: &str, action: &str) -> Result<&'static str> {
    let kind = action_kind(resource, action).ok_or_else(|| scope_unavailable(resource, action))?;
    Ok(describe(resource, find_action(resource, action), kind))
}

/// 汇总所选动作的政策说明；相同说明只展示一次，不同说明按动作列出，空动作集报错。
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

/// 核对范围规格唯一、范围与固定分类互斥，并拒绝非法维度或成环的来源。
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

    Ok(())
}

// 核对单条规格的维度、本人基础、历史读取和复用来源是否符合静态目录约束。
fn validate_scope(spec: &ScopeSpec) -> Result<()> {
    if spec.dimensions.is_empty() || spec.scope_types.is_empty() {
        return Err(Error::ValidationError(format!("{} 缺少范围维度或类型", spec.permission)));
    }

    if spec.default_self && !matches!(&spec.source, ScopeSource::Current) {
        return Err(Error::ValidationError(format!("{} 不能声明本人基础", spec.permission)));
    }
    if spec.default_self && spec.dimensions != [ScopeDimension::InternalOrg] {
        return Err(Error::ValidationError(format!("{} 的本人基础只能使用内部组织", spec.permission)));
    }

    if spec.allows_history
        && !matches!(
            spec.permission.action(),
            "list"
                | "detail"
                | "financial_expense_source"
                | "profit_loss_read"
                | "extra_cost_read"
                | "customer_quality_history"
                | "delivery_options_read"
        )
    {
        return Err(Error::ValidationError(format!("{} 的历史读取只适用于读取", spec.permission)));
    }
    if spec.allows_history && spec.dimensions != [ScopeDimension::InternalOrg] {
        return Err(Error::ValidationError(format!("{} 的历史读取只能使用内部组织", spec.permission)));
    }

    let source = runtime_source(spec)?;
    if let ScopeSource::Permission(_) = &spec.source
        && source.dimensions.iter().any(|dimension| !spec.dimensions.contains(dimension))
    {
        return Err(Error::ValidationError(format!(
            "{} 不支持来源 {} 的范围维度",
            spec.permission, source.permission
        )));
    }

    validate_scope_types(spec)
}

// 校验同资源各动作的范围类型一致；维度仍按动作分别校验。
fn validate_scope_types(spec: &ScopeSpec) -> Result<()> {
    let siblings =
        raw_specs().iter().filter(|other| other.permission.resource() == spec.permission.resource());
    if siblings.clone().any(|other| other.scope_types != spec.scope_types) {
        return Err(Error::ValidationError(format!("{} 的范围类型不一致", spec.permission.resource())));
    }
    Ok(())
}

// 用权限的资源和动作值查找登记分类，供入口检查和错误区分。
fn kind_of(permission: &Permission) -> Option<ActionKind> {
    action_kind(permission.resource(), permission.action())
}

// 将未登记的权限转换为范围未开放的校验错误。
fn unwired(permission: &Permission) -> Error {
    scope_unavailable(permission.resource(), permission.action())
}

// 记录未开放动作的技术上下文，返回面向配置人员的校验提示。
fn scope_unavailable(resource: &str, action: &str) -> Error {
    tracing::debug!(resource, action, "操作未开放数据范围配置");
    Error::ValidationError("所选操作尚未开放数据范围配置，请核对授权设置".into())
}

// 用已登记的政策说明解释为何不允许给指定权限单独配置范围。
fn independent_scope_rejected(permission: &Permission) -> Error {
    let text = action_description(permission.resource(), permission.action())
        .unwrap_or("此动作没有独立数据范围配置");
    Error::ValidationError(format!("此操作不接受独立数据范围：{text}"))
}

// 按资源特例和入口分类选择中文说明；说明不参与授权判断。
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::access_control::ScopeDimension;

    // 验证真实目录的登记唯一且复用指向当前配置，并保持固定动作与未知动作的区别。
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

    // 验证缺失引用、成环和多级范围复用分别被拒绝。
    #[test]
    fn missing_and_cyclic_sources_are_rejected() {
        const LEFT: Permission = Permission::new("scope_cycle_left", "list");
        const MISSING: Permission = Permission::new("scope_cycle_missing", "list");
        let org: &[ScopeDimension] = &[ScopeDimension::InternalOrg];
        let types: &[DataScopeType] = &[DataScopeType::Company];

        let missing = scope(&LEFT, ScopeSource::Permission(MISSING), false, types, org, false);
        assert!(runtime_source(&missing).is_err());

        let list = erp_permissions::contract::LIST;
        let cycle = scope(&list, ScopeSource::Permission(list.clone()), false, types, org, false);
        assert!(runtime_source(&cycle).is_err());

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

    // 验证仅当前来源可编辑范围，并核对本人基础、允许类型和权限规范化查询。
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

    // 验证公司共享不加载组织、普通范围加载操作人、治理动作加载全员，并保留多维结算范围。
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
            find_action("supplier_deposit", "resolve").unwrap().dimensions(),
            &[ScopeDimension::InternalOrg, ScopeDimension::SettlementParty]
        );
    }
}
