//! 按权限固定的授权规则。对象判断和查询转换共用这里产生的数据条件。
use std::borrow::Cow;
use std::slice;

use erp_permissions::{
    MODULES, catalog, internal, receivable, sales_inquiry, supplier, supplier_fulfillment, supplier_offering,
};

use crate::QualifiedPermissions;
mod inventory_warehouses;
pub(crate) mod quality;
pub(super) mod queries;
pub(super) mod records;
pub(crate) mod selection;
pub(crate) mod sources;
use std::collections::BTreeSet;

use erp_permissions::{
    approval_instance, cost, inventory, purchase_order, purchase_payment_request, returns, sales_order,
    supplier_settlement, work_item,
};

use super::{
    approval_query, approvals, definitions, materials, operations, transfers, work_item_query, work_items,
};
use crate::entity::access_control::scope_spec::{self, ActionKind};
use crate::entity::access_control::{DataScopeType, ResolvedScope, ScopeClause, ScopeDimension};
use crate::entity::authorization::{
    AccessFact, AccessRule, AccessTarget, FactKind, QueryOutput, RelationKey, RelationMatch,
};
use crate::entity::resource_access::ResourceKind;
use crate::{Error, Permission, Result};

// 同一来源可按不同入口的合格角色解析，不能只按来源归并。
pub(super) enum ScopeLookup<'a> {
    Capability,
    Resolved(&'a [(Permission, Permission, ResolvedScope)]),
}

impl<'a> ScopeLookup<'a> {
    /// 按入口和来源同时查找运行时范围；启动校验生成来源能力范围，缺失时按调用场景报错。
    pub(super) fn get(
        &self,
        entry: &Permission,
        source: &Permission,
        missing: impl FnOnce() -> Error,
    ) -> Result<Cow<'a, ResolvedScope>> {
        match self {
            Self::Capability => capability_scope(source).map(Cow::Owned),
            Self::Resolved(scopes) => scopes
                .iter()
                .find(|(key, origin, _)| key == entry && origin == source)
                .map(|(_, _, scope)| Cow::Borrowed(scope))
                .ok_or_else(missing),
        }
    }
}

/// 取得固定入口须由同一启用角色授予的完整权限，范围复用入口沿用原动作组合，调用方不能拼装。
pub(crate) fn same_role_permissions(permission: &Permission) -> Vec<Permission> {
    if let Some(source) = inventory_warehouses::source(permission) {
        return same_role_permissions(source);
    }
    if transfers::supports(permission) {
        return vec![];
    }
    if let Some(required) = operations::requirements(permission) {
        return required;
    }
    if let Some(entry) = definitions::entry(permission) {
        return vec![entry];
    }
    if let Some(required) = records::requirements(permission) {
        return required;
    }

    let mut required = vec![permission.clone()];
    if [
        sales_inquiry::COMMANDS_SALES_CREATE,
        sales_inquiry::COMMANDS_SALES_SAVE,
        sales_inquiry::COMMANDS_SALES_SUBMIT,
        sales_inquiry::IMPORT_SALES_IMPORT_PREVIEW,
    ]
    .contains(permission)
    {
        required.push(catalog::PRODUCT_SELLABLE_SKU_LIST);
    }
    if permission == &approval_instance::SUBMIT_DECISION || approvals::supports(permission) {
        required.push(approval_instance::INSTANCE_LIST.clone());
    } else if permission == &sales_order::SUBMIT {
        required.push(sales_order::CREATE);
    } else if permission == &cost::ENTRY_LIST {
        required.push(sales_order::LIST.clone());
    } else if permission == &cost::ENTRY_CREATE {
        required.push(sales_order::DETAIL.clone());
    } else if matches!(permission, p if p == &inventory::STOCK_ADJUSTMENT_LIST || p == &inventory::STOCK_ADJUSTMENT_CREATE || p == &inventory::STOCK_ADJUSTMENT_UPDATE || p == &inventory::STOCK_ADJUSTMENT_SUBMIT)
    {
        required.push(inventory::STOCK_ADJUSTMENT_DETAIL);
    } else if permission == &purchase_payment_request::CREDIT_LIST {
        required.push(purchase_payment_request::DETAIL);
    }
    required
}

/// 生成对象和查询共用的普通条件，纳入固定事实维度、范围、责任和允许的读取补充；来源及精确任务另有专用规则。
pub(crate) fn data_condition(permission: &Permission, scope: Option<&ResolvedScope>) -> Result<AccessRule> {
    if permission == &internal::WORK_ITEM_RECIPIENT {
        return Ok(AccessRule::Allow);
    }
    match scope_spec::kind(permission)? {
        ActionKind::Function => Err(Error::ValidationError("仅功能资格的操作没有数据条件".into())),
        ActionKind::Task => {
            Ok(records::condition(permission).unwrap_or_else(|| task_data_condition(permission)))
        },
        ActionKind::Source => Err(Error::ValidationError("来源入口须使用固定来源分区条件".into())),
        ActionKind::Scope => {
            let spec = scope_spec::require_scope(permission)?;
            let resolved = scope.ok_or_else(|| Error::ValidationError("缺少范围解析".into()))?;
            let mut rule = AccessRule::scope(resolved, spec.allows_history());
            if let Some(resource) = record_grant(permission) {
                rule = AccessRule::any(vec![rule, AccessRule::RecordReadGrant(resource)]);
            }
            if permission == &inventory::STOCK_ADJUSTMENT_SUBMIT {
                rule = AccessRule::all(vec![rule, AccessRule::Fact(AccessFact::AssignedTo)])
                    .requiring(&BTreeSet::from([FactKind::Assignee]));
            }
            if matches!(
                permission.resource(),
                "stock_adjustment" | "stock_balance" | "stock_movement" | "stock_reservation"
            ) {
                rule = AccessRule::all(vec![
                    rule,
                    AccessRule::Related {
                        relation: RelationKey::parse("warehouse")?,
                        resource: "warehouse".into(),
                        mode: RelationMatch::ExactlyOne,
                        rule: Box::new(AccessRule::Allow),
                    },
                ]);
            }
            Ok(records::scoped(permission, rule).requiring(&required_facts(permission)?))
        },
    }
}

// 识别覆盖默认目标形态的固定入口；未特判时交给动作类别，不把有任务或候选要求的操作降级为普通记录。
fn specific_target(permission: &Permission, target: &AccessTarget) -> Option<bool> {
    if [
        returns::CUSTOMER_REFUND_CUSTOMER_REFUND_SUBMIT,
        returns::SUPPLIER_REFUND_SUPPLIER_REFUND_SUBMIT,
        returns::RECEIPT_REVERSAL_RECEIPT_REVERSAL_SUBMIT,
        returns::PAYMENT_REVERSAL_PAYMENT_REVERSAL_SUBMIT,
        supplier_fulfillment::ORDER_SUBMIT,
        internal::SALES_CREATE_AND_SUBMIT,
    ]
    .contains(permission)
    {
        return Some(matches!(target, AccessTarget::Candidate(_) | AccessTarget::Records(_)));
    }
    if [internal::WORK_ITEM_EXECUTE, internal::WORK_ITEM_RECIPIENT].contains(permission) {
        return Some(matches!(target, AccessTarget::Task { .. }));
    }
    if permission == &internal::WORK_ITEM_EXECUTION_QUALIFICATION {
        return Some(matches!(target, AccessTarget::Candidate(_)));
    }
    if let Some(allowed) = definitions::target(permission, target) {
        return Some(allowed);
    }
    if approvals::supports(permission) {
        return Some(
            matches!(target, AccessTarget::Records(_))
                || (permission == &approval_instance::UPGRADE_BINDING
                    && matches!(target, AccessTarget::Candidate(_))),
        );
    }
    if permission == &supplier_offering::COMPLETE_SUPPLY_EXCEPTION_TASK {
        return Some(matches!(target, AccessTarget::Task { .. }));
    }
    if permission == &purchase_payment_request::CREDIT_LIST || permission == &receivable::INVOICE_POST {
        return Some(matches!(target, AccessTarget::Task { .. }));
    }
    if matches!(target, AccessTarget::Task { .. }) && task_supported(permission) {
        return Some(true);
    }
    if permission == &sales_order::SUBMIT && matches!(target, AccessTarget::Candidate(_)) {
        return Some(true);
    }
    if [
        internal::PURCHASE_CREATE_AND_SUBMIT,
        internal::SALES_INQUIRY_IMPORT_CREATE,
        internal::SALES_ORDER_HANDOVER_RECIPIENT,
    ]
    .contains(permission)
    {
        return Some(matches!(target, AccessTarget::Candidate(_)));
    }
    None
}

/// 在统一入口取证前检查目标形态；先应用固定例外，再按动作类别限制操作、记录、候选和精确任务。
pub(super) fn ensure_target(permission: &Permission, target: &AccessTarget) -> Result<()> {
    let kind = scope_spec::kind(permission)?;
    if let Some(allowed) = specific_target(permission, target) {
        return if allowed {
            Ok(())
        } else {
            Err(Error::ValidationError("此权限不支持该授权目标".into()))
        };
    }

    let allowed = match kind {
        ActionKind::Function => matches!(target, AccessTarget::Operation),
        ActionKind::Task => {
            if records::condition(permission).is_some() {
                matches!(target, AccessTarget::Records(_))
            } else {
                matches!(target, AccessTarget::Task { .. })
            }
        },
        ActionKind::Source => {
            if permission.action() == "create" {
                matches!(target, AccessTarget::Candidate(_))
            } else if permission == &purchase_payment_request::UPDATE
                || permission == &receivable::CUSTOMER_RECEIPT_SUBMIT
            {
                matches!(target, AccessTarget::Records(_) | AccessTarget::OwnershipChange { .. })
            } else {
                matches!(target, AccessTarget::Records(_))
            }
        },
        ActionKind::Scope => match permission.action() {
            "create" => matches!(target, AccessTarget::Candidate(_)),
            "update" | "handover" => {
                matches!(target, AccessTarget::Records(_) | AccessTarget::OwnershipChange { .. })
            },
            _ => matches!(target, AccessTarget::Records(_)),
        },
    };
    if allowed { Ok(()) } else { Err(Error::ValidationError("此权限不支持该授权目标".into())) }
}

/// 识别可走读取路径的固定动作；非读取对象操作必须先进入快照，精确任务仍另受快照限制。
pub(super) fn is_read(permission: &Permission) -> bool {
    if let Some(source) = inventory_warehouses::source(permission) {
        return is_read(source);
    }
    if transfers::supports(permission) {
        return true;
    }
    if permission == &internal::APPROVAL_DEFINITION_ASSIGNEES {
        return true;
    }
    if approvals::supports(permission) {
        return permission == &internal::APPROVAL_MANAGEMENT_READ
            || permission == &internal::APPROVAL_INSTANCE_READ;
    }
    matches!(
        permission.action(),
        "change_draft_read"
            | "change_submitted_result_read"
            | "change_voided_result_read"
            | "applied_result_read"
            | "created_result_read"
            | "sourcing_created_result_read"
            | "posted_result_read"
            | "portal_impact_read"
            | "profit_loss_read"
            | "extra_cost_read"
            | "completed_result_read"
            | "started_list"
            | "mine_list"
            | "quality_current"
            | "customer_quality_current"
            | "customer_quality_history"
            | "profit_loss_period_basis_read"
            | "delivery_options_read"
            | "template_products_read"
            | "list"
            | "detail"
            | "get"
            | "read"
            | "read_offering_update"
            | "financial_expense_source"
            | "copy_link"
            | "select_current"
            | "recover"
            | "product_import_read"
            | "supplier_import_read"
            | "brand_import_read"
            | "data_transfer_read"
            | "payment_request_candidate"
            | "purchase_expense_source"
            | "order_basis_read"
    )
}

// 按任务用途生成数据条件：普通任务读取允许指派或参与，内部材料及办理要求当前指派，已完成采购任务还须本人参与；审批入口按固定事实和来源另查。
fn task_data_condition(permission: &Permission) -> AccessRule {
    if permission == &internal::APPROVAL_INSTANCE_READ {
        return AccessRule::Allow.requiring(&BTreeSet::from([
            FactKind::Owner,
            FactKind::Assignee,
            FactKind::Participants,
        ]));
    }
    if approvals::supports(permission) {
        return AccessRule::Allow;
    }
    if matches!(permission.action(), "list" | "detail" | "read") {
        AccessRule::any(vec![
            AccessRule::Fact(AccessFact::AssignedTo),
            AccessRule::Fact(AccessFact::Participant),
        ])
    } else if permission.resource() == "procurement_inquiry" {
        AccessRule::all(vec![
            AccessRule::Fact(AccessFact::AssignedTo),
            AccessRule::any(vec![
                AccessRule::Fact(AccessFact::Completed(false)),
                AccessRule::Fact(AccessFact::Participant),
            ]),
        ])
    } else {
        AccessRule::Fact(AccessFact::AssignedTo)
    }
}

// 按动作的固定数据合同声明必需事实，不取决于本次范围条款；公司范围或记录补权不能掩盖缺失。
fn required_facts(permission: &Permission) -> Result<BTreeSet<FactKind>> {
    let spec = scope_spec::require_scope(permission)?;
    let mut facts = BTreeSet::new();
    for dimension in spec.dimensions() {
        match dimension {
            ScopeDimension::InternalOrg => {
                facts.extend([FactKind::Owner, FactKind::Organization]);
            },
            ScopeDimension::SettlementParty => {
                facts.insert(FactKind::SettlementParty);
            },
            ScopeDimension::Warehouse => {
                facts.insert(FactKind::Warehouse);
            },
        }
    }
    Ok(facts)
}

// 允许以销售单记录读取授权补充数据范围的固定入口：仅用于读取销售单或作为下游来源读取，
// 不授予销售单自身的写入范围；其中 `PURCHASE_EXPENSE_SOURCE` 刻意关闭历史参与，但保留显式记录读取授权。
const SALES_RECORD_GRANTS: &[Permission] = &[
    sales_order::LIST,
    sales_order::DETAIL,
    internal::SALES_ORDER_FINANCIAL_EXPENSE_SOURCE,
    internal::SALES_ORDER_PROFIT_LOSS_READ,
    internal::PURCHASE_EXPENSE_SOURCE,
    internal::SALES_ORDER_EXTRA_COST_READ,
    internal::CUSTOMER_QUALITY_HISTORY,
];

// 允许以采购单记录读取授权补充数据范围的固定入口：仅用于读取采购单，不授予采购单自身的写入范围。
const PURCHASE_RECORD_GRANTS: &[Permission] =
    &[purchase_order::LIST, purchase_order::DETAIL, internal::PURCHASE_DELIVERY_OPTIONS_READ];

// 按精确权限常量判断入口能否使用销售单或采购单的记录读取补充，不按资源与动作名做笛卡尔积匹配。
fn record_grant(permission: &Permission) -> Option<ResourceKind> {
    if SALES_RECORD_GRANTS.contains(permission) {
        Some(ResourceKind::SalesOrder)
    } else if PURCHASE_RECORD_GRANTS.contains(permission) {
        Some(ResourceKind::PurchaseOrder)
    } else {
        None
    }
}

/// 按固定登记校验权限是否能编译查询，不从动作名推断，也不放宽写入所需快照。
pub(super) fn query_supported(permission: &Permission) -> Result<bool> {
    scope_spec::kind(permission)?;
    Ok(queries::permissions().any(|entry| entry == permission))
}

/// 枚举范围目录中的固定入口供构建器检查绑定，不拿公共菜单替代实际规则集。
pub(super) fn permissions() -> Vec<&'static Permission> {
    scope_spec::resources().into_iter().flat_map(scope_spec::actions).collect()
}

/// 为启动校验构造覆盖该动作全部可配置维度的范围，不按当前账号配置省略能力。
pub(super) fn capability_scope(permission: &Permission) -> Result<ResolvedScope> {
    let spec = scope_spec::require_scope(permission)?;
    if matches!(scope_spec::runtime_source(spec)?.source(), scope_spec::ScopeSource::Company) {
        return Ok(ResolvedScope {
            role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
            user_limit: None,
        });
    }
    let dimensions = spec.dimensions();
    let clause = ScopeClause {
        self_owned: dimensions.contains(&ScopeDimension::InternalOrg),
        collaborative: spec.scope_types().contains(&DataScopeType::Collaborative),
        org_unit_ids: if dimensions.contains(&ScopeDimension::InternalOrg) {
            ["binding".into()].into()
        } else {
            BTreeSet::new()
        },
        settlement_party_ids: if dimensions.contains(&ScopeDimension::SettlementParty) {
            ["binding".into()].into()
        } else {
            BTreeSet::new()
        },
        warehouse_ids: if dimensions.contains(&ScopeDimension::Warehouse) {
            ["binding".into()].into()
        } else {
            BTreeSet::new()
        },
        ..Default::default()
    };
    Ok(ResolvedScope { role_clauses: vec![clause], user_limit: None })
}

// 保持选型、质量附加条件的构造顺序，启动校验传入能力范围，运行时传入同次取证结果。
pub(super) fn scoped_extensions(
    permission: &Permission,
    qualified: Option<&QualifiedPermissions>,
    scopes: &ScopeLookup<'_>,
    base: AccessRule,
) -> Result<AccessRule> {
    let base = selection::condition(permission, qualified, scopes, base)?;
    quality::condition(permission, qualified, scopes, base)
}

/// 启动时按输出粒度展开固定规则全部可能使用的 Mapping 能力；运行时仍以实际身份及范围构造过滤。
pub(crate) fn binding_condition(permission: &Permission, output: &QueryOutput) -> Result<AccessRule> {
    let kind = scope_spec::kind(permission)?;
    if materials::supports(permission) {
        return materials::binding_condition(permission);
    }
    if permission == &internal::CONNECTION_CATALOG_READ {
        return supplier_catalog_condition(data_condition(
            &supplier::DETAIL,
            Some(&capability_scope(&supplier::DETAIL)?),
        )?);
    }
    if permission == &internal::APPROVAL_MANAGEMENT_READ {
        return approval_query::query_binding_condition();
    }
    if permission == &work_item::DETAIL {
        return work_item_query::binding_condition(data_condition(
            permission,
            Some(&capability_scope(permission)?),
        )?);
    }

    match kind {
        ActionKind::Source => {
            let output =
                if *output == QueryOutput::Object { sources::Output::Object } else { sources::Output::Share };
            let rule = sources::binding_condition(permission, output)?;
            if task_query(permission) { task_condition(permission, Some(rule)) } else { Ok(rule) }
        },
        _ if *output != QueryOutput::Object => Err(Error::ValidationError("此权限不支持受限输出".into())),
        ActionKind::Scope => {
            let base = data_condition(permission, Some(&capability_scope(permission)?))?;
            let rule = scoped_extensions(permission, None, &ScopeLookup::Capability, base)?;
            if task_query(permission) {
                // 创建候选仍使用范围条件，查询另须支持精确任务的指派条件。
                return Ok(AccessRule::all(vec![rule, task_condition(permission, None)?]));
            }
            if let Some(management) = management_binding_condition(permission)? {
                return Ok(AccessRule::all(vec![rule, management]));
            }
            Ok(rule)
        },
        ActionKind::Task => data_condition(permission, None),
        ActionKind::Function => Err(Error::ValidationError("仅功能操作不绑定查询".into())),
    }
}

/// 将商品目录规则限定到全部真实供应商来源；连接访问资格不等于能读所有供应商。
pub(super) fn supplier_catalog_condition(rule: AccessRule) -> Result<AccessRule> {
    Ok(AccessRule::Related {
        relation: RelationKey::parse("supplier")?,
        resource: "supplier".into(),
        mode: RelationMatch::AllNonEmpty,
        rule: Box::new(rule),
    })
}

/// 为工作项列表或详情选定固定管理过滤权限，不允许调用方另选范围入口。
pub(crate) fn management_permission(permission: &Permission) -> Option<Permission> {
    (permission == &work_item::LIST || permission == &work_item::DETAIL).then_some(work_item::MANAGE)
}

/// 启动时为有独立管理过滤的入口生成完整能力条件，供 Mapping 随根条件一并校验。
pub(super) fn management_binding_condition(permission: &Permission) -> Result<Option<AccessRule>> {
    management_permission(permission)
        .map(|management| data_condition(&management, Some(&capability_scope(&management)?)))
        .transpose()
}

/// 为一次身份取证汇总固定同角色、独立和可选资格，让各分支复用同次角色快照。
pub(super) fn qualifications(permission: &Permission) -> Vec<Permission> {
    if let Some(required) = transfers::requirements(permission) {
        return required.to_vec();
    }
    let mut required = same_role_permissions(permission);
    if scope_spec::kind(permission).is_ok_and(|kind| kind == ActionKind::Source)
        && let Ok(extra) = sources::qualifications(permission)
    {
        required.extend(extra);
    }
    required.extend(selection::qualifications(permission));
    required.extend(quality::qualifications(permission));
    required.extend(approvals::qualifications(permission));
    required.extend(work_items::qualifications(permission));
    required.extend(records::independent(permission));
    required.extend(records::optional(permission));

    required.sort();
    required.dedup();
    required
}

/// 判断一次身份快照是否满足固定同角色与独立资格，以及背景任务全部取消的附加管理员要求；文件任务按独立规则判断。
pub(super) fn qualified(permission: &Permission, qualified: &QualifiedPermissions) -> bool {
    if transfers::supports(permission) {
        return transfers::qualified(permission, qualified);
    }
    let required = same_role_permissions(permission);
    (required.is_empty() || qualified.same_role(&required))
        && records::independent(permission).iter().all(|p| qualified.same_role(slice::from_ref(p)))
        && records::qualified(permission, qualified)
}

/// 拒绝时仅在能唯一确定某项公共权限且所有启用角色均未授予时报告权限码；角色拆分或多项缺失不报告单项。
pub(super) fn missing_public_permission(
    permission: &Permission,
    task_required: &[Permission],
    qualified: &QualifiedPermissions,
) -> Option<Permission> {
    if transfers::supports(permission) {
        return None;
    }
    let mut required = same_role_permissions(permission);
    required.extend(records::independent(permission));
    required.extend_from_slice(task_required);
    required.sort();
    required.dedup();
    required.retain(|required| !qualified.same_role(slice::from_ref(required)));

    let [missing] = required.as_slice() else { return None };
    MODULES
        .iter()
        .flat_map(|module| module.entries)
        .any(|(_, entry)| entry.permission == missing)
        .then(|| missing.clone())
}

/// 列出精确任务所需的同角色资格，补入结算审核或履约动作要求，不能用普通读取资格替代。
pub(super) fn task_permissions(permission: &Permission) -> Vec<Permission> {
    let mut permissions = same_role_permissions(permission);
    if permission == &supplier_settlement::STATEMENT_REVIEW_DECIDE {
        permissions.extend([supplier_settlement::STATEMENT_DETAIL, work_item::DETAIL]);
    }
    permissions.extend_from_slice(work_items::fulfillment_task_permissions(permission));
    permissions.sort();
    permissions.dedup();
    permissions
}

/// 识别允许按精确任务目标检查的固定入口，供绑定校验及运行时目标分派。
pub(super) fn task_supported(permission: &Permission) -> bool {
    scope_spec::kind(permission)
        .is_ok_and(|kind| kind == ActionKind::Task && records::condition(permission).is_none())
        || permission == &work_item::DETAIL
        || permission == &internal::WORK_ITEM_EXECUTE
        || permission == &supplier_offering::COMPLETE_SUPPLY_EXCEPTION_TASK
        || permission == &supplier_fulfillment::ORDER_INVESTIGATION
        || permission == &purchase_order::CREATE
        || permission == &purchase_payment_request::SUBMIT
        || permission == &purchase_payment_request::CREDIT_LIST
        || permission == &receivable::INVOICE_POST
        || matches!(
            permission.resource(),
            "delivery"
                | "purchase_receipt"
                | "service_fulfillment"
                | "electronic_delivery"
                | "customer_acceptance"
        )
}

/// 仅让采购创建及付款授信列表使用精确任务条件编译查询，不改变其对象授权要求。
pub(super) fn task_query(permission: &Permission) -> bool {
    permission == &purchase_order::CREATE || permission == &purchase_payment_request::CREDIT_LIST
}

/// 按入口选定精确任务事实端口；审批、采购询价及对账差异各用本资源，其他用工作项端口。
pub(super) fn task_facts_resource(permission: &Permission) -> &str {
    if matches!(
        permission.resource(),
        "approval_instance" | "procurement_inquiry" | "reconciliation_difference"
    ) {
        permission.resource()
    } else {
        "work_item"
    }
}

/// 构造精确任务责任条件：采购创建只查任务指派，普通任务叠加指派和原规则；工作项执行及接收者沿专用规则判断。
pub(super) fn task_condition(permission: &Permission, base: Option<AccessRule>) -> Result<AccessRule> {
    if !task_supported(permission) {
        return Err(Error::ValidationError("此权限不支持精确任务".into()));
    }
    if [internal::WORK_ITEM_EXECUTE, internal::WORK_ITEM_RECIPIENT].contains(permission) {
        return Ok(base.unwrap_or(AccessRule::Allow));
    }
    let assigned = AccessRule::Fact(AccessFact::AssignedTo);
    // 工作项提供自身责任与业务来源关系，来源条件不能套用任务负责人归属。
    if permission == &purchase_order::CREATE {
        return Ok(assigned.requiring(&BTreeSet::from([FactKind::Assignee])));
    }
    Ok(AccessRule::all(vec![base.unwrap_or(AccessRule::Allow), assigned])
        .requiring(&BTreeSet::from([FactKind::Assignee])))
}

#[cfg(test)]
mod tests {
    use erp_permissions::sales_inquiry as inquiry;

    use super::*;
    use crate::entity::authorization::{FactValue, ObjectFacts};

    // 验证销售提交缺少唯一公开创建权限时，拒绝说明能定位该权限。
    #[test]
    fn reports_a_single_public_permission_missing_from_all_roles() {
        let roles = QualifiedPermissions::test_grants(&[("sales", &[sales_order::SUBMIT])]);

        assert!(!qualified(&sales_order::SUBMIT, &roles));
        assert_eq!(missing_public_permission(&sales_order::SUBMIT, &[], &roles), Some(sales_order::CREATE));
    }

    // 验证同角色资格被拆分或多项权限全缺时，不将拒绝误报为缺少某一权限。
    #[test]
    fn reports_no_single_permission_for_a_split_role_or_multiple_missing_permissions() {
        let split = QualifiedPermissions::test_grants(&[
            ("sales", &[sales_order::SUBMIT]),
            ("creator", &[sales_order::CREATE]),
        ]);
        let empty = QualifiedPermissions::test_grants(&[]);

        assert!(!qualified(&sales_order::SUBMIT, &split));
        assert_eq!(missing_public_permission(&sales_order::SUBMIT, &[], &split), None);
        assert_eq!(missing_public_permission(&sales_order::SUBMIT, &[], &empty), None);
    }

    // 构造无个人上限的本人范围，供销售授权测试使用。
    fn scope() -> ResolvedScope {
        ResolvedScope {
            role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
            user_limit: None,
        }
    }
    // 构造负责人为他人、测试账号曾参与的销售单事实。
    fn object() -> ObjectFacts {
        ObjectFacts {
            resource: "sales_order".into(),
            id: "one".into(),
            version: "v1".into(),
            owner_id: FactValue::Known(Some("other".into())),
            org_unit_id: FactValue::Known(None),
            history_participants: FactValue::Known(["user".into()].into()),
            ..Default::default()
        }
    }
    // 验证历史参与先与角色范围合并，再受个人上限限制；记录补权可恢复读取但不能交接。
    #[test]
    fn history_applies_before_personal_limit_and_record_grants_only_supplement_reads() {
        let mut scope = scope();
        let object = object();
        let grants = [(ResourceKind::SalesOrder, "one".into())].into();
        let allows = |permission: &Permission,
                      scope: &ResolvedScope,
                      grants: &BTreeSet<(ResourceKind, String)>| {
            data_condition(permission, Some(scope)).unwrap().allows_object("user", &object, grants).unwrap()
        };

        assert!(allows(&sales_order::DETAIL, &scope, &BTreeSet::new()));
        scope.user_limit = Some(ScopeClause::default());
        assert!(!allows(&sales_order::DETAIL, &scope, &BTreeSet::new()));
        assert!(allows(&sales_order::DETAIL, &scope, &grants));
        assert!(!allows(&sales_order::HANDOVER, &scope, &grants));
    }

    // 验证记录读取补充按精确权限常量匹配，拒绝跨资源组合及原单写入或当前口径入口。
    #[test]
    fn record_grant_matches_exact_permissions_and_rejects_cross_resource_or_write_entries() {
        for permission in SALES_RECORD_GRANTS {
            assert_eq!(permission.resource(), "sales_order", "{permission}");
            assert_eq!(scope_spec::kind(permission).unwrap(), ActionKind::Scope, "{permission}");
            assert_eq!(record_grant(permission), Some(ResourceKind::SalesOrder), "{permission}");
        }
        for permission in PURCHASE_RECORD_GRANTS {
            assert_eq!(permission.resource(), "purchase_order", "{permission}");
            assert_eq!(scope_spec::kind(permission).unwrap(), ActionKind::Scope, "{permission}");
            assert_eq!(record_grant(permission), Some(ResourceKind::PurchaseOrder), "{permission}");
        }

        for cross_resource in [
            "sales_order:delivery_options_read",
            "purchase_order:financial_expense_source",
            "purchase_order:profit_loss_read",
            "purchase_order:purchase_expense_source",
            "purchase_order:extra_cost_read",
            "purchase_order:customer_quality_history",
        ] {
            let permission = Permission::parse(cross_resource).unwrap();
            assert_eq!(record_grant(&permission), None, "{cross_resource}");
        }

        for denied in [
            sales_order::CREATE,
            sales_order::HANDOVER,
            sales_order::SUBMIT,
            sales_order::VOID,
            sales_order::CANCEL_APPROVAL,
            internal::SALES_CHANGE_DRAFT_READ,
            internal::CUSTOMER_QUALITY_CURRENT,
            internal::SALES_ORDER_HANDOVER_RECIPIENT,
            internal::SALES_CREATED_RESULT_READ,
            purchase_order::CREATE,
            purchase_order::SAVE_DRAFT,
            purchase_order::SUBMIT,
            purchase_order::VOID,
            internal::PAYMENT_REQUEST_CANDIDATES,
            internal::PURCHASE_PORTAL_IMPACT_READ,
            erp_permissions::customer::LIST,
            erp_permissions::customer::DETAIL,
        ] {
            assert_eq!(record_grant(&denied), None, "{denied}");
        }
    }

    // 验证费用采购来源拒绝历史参与补充，但接受精确销售单记录读取授权，且不掩盖缺失事实。
    #[test]
    fn purchase_expense_source_rejects_history_but_accepts_sales_record_read_grant() {
        let scope = scope();
        let mut object = object();
        let rule = data_condition(&internal::PURCHASE_EXPENSE_SOURCE, Some(&scope)).unwrap();

        assert!(!rule.allows_object("user", &object, &BTreeSet::new()).unwrap());

        let wrong_resource = [(ResourceKind::PurchaseOrder, "one".into())].into();
        assert!(!rule.allows_object("user", &object, &wrong_resource).unwrap());

        let sales_grant = [(ResourceKind::SalesOrder, "one".into())].into();
        assert!(rule.allows_object("user", &object, &sales_grant).unwrap());

        object.owner_id = FactValue::Missing;
        assert!(matches!(rule.allows_object("user", &object, &sales_grant), Err(Error::ValidationError(_))));
    }
    // 验证空范围拒绝，且公司范围也不能掩盖必需负责人事实缺失。
    #[test]
    fn empty_scope_denies_and_missing_facts_fail_even_in_company_scope() {
        let mut object = object();
        let empty = ResolvedScope { role_clauses: vec![], user_limit: None };
        assert!(
            !data_condition(&inquiry::COMMANDS_SALES_SAVE, Some(&empty))
                .unwrap()
                .allows_object("user", &object, &BTreeSet::new())
                .unwrap()
        );

        let company = ResolvedScope {
            role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
            user_limit: None,
        };
        object.owner_id = FactValue::Missing;
        assert!(matches!(
            data_condition(&inquiry::COMMANDS_SALES_SAVE, Some(&company)).unwrap().allows_object(
                "user",
                &object,
                &BTreeSet::new()
            ),
            Err(Error::ValidationError(_))
        ));
    }
    // 验证已完成采购任务须当前指派且本人参与，参与事实缺失报接入错误。
    #[test]
    fn completed_procurement_task_needs_assignment_and_participation() {
        let rule = data_condition(&inquiry::COMMANDS_PROCUREMENT_RENEW, None).unwrap();
        let mut task = object();
        task.completed = FactValue::Known(true);
        task.assignee_id = FactValue::Known(Some("user".into()));
        task.participants = FactValue::Known(BTreeSet::new());
        assert!(!rule.allows_object("user", &task, &BTreeSet::new()).unwrap());

        task.participants = FactValue::Known(["user".into()].into());
        assert!(rule.allows_object("user", &task, &BTreeSet::new()).unwrap());

        task.participants = FactValue::Missing;
        assert!(matches!(
            rule.allows_object("user", &task, &BTreeSet::new()),
            Err(Error::ValidationError(_))
        ));
    }
}
