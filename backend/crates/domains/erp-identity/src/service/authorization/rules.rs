//! 按权限固定的授权规则。对象判断和查询转换共用这里产生的数据条件。
use std::slice;

use erp_permissions::{
    MODULES, catalog, fulfillment, internal, receivable, sales_inquiry, supplier, supplier_fulfillment,
    supplier_offering,
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
    AccessFact, AccessRule, FactKind, QueryOutput, RelationKey, RelationMatch,
};
use crate::entity::resource_access::ResourceKind;
use crate::{Error, Permission, Result};

/// 调用方可以提交的目标形态。权限自己决定哪些形态有效。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TargetForm {
    Operation,
    Record,
    Candidate,
    OwnershipChange,
    Task,
}

// 取固定入口要求的完整同角色功能权限组合，不接受业务调用方改动所需集合。
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

// 把固定数据规则转成对象判断与查询编译共用条件；任务办理与创建的专用规则另行处理。
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
            if record_grant(permission) {
                let resource = if permission.resource() == "sales_order" {
                    ResourceKind::SalesOrder
                } else {
                    ResourceKind::PurchaseOrder
                };
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

fn specific_target(permission: &Permission, target: TargetForm) -> Option<bool> {
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
        return Some(matches!(target, TargetForm::Candidate | TargetForm::Record));
    }
    if [internal::WORK_ITEM_EXECUTE, internal::WORK_ITEM_RECIPIENT].contains(permission) {
        return Some(target == TargetForm::Task);
    }
    if permission == &internal::WORK_ITEM_EXECUTION_QUALIFICATION {
        return Some(target == TargetForm::Candidate);
    }
    if let Some(allowed) = definitions::target(permission, target) {
        return Some(allowed);
    }
    if approvals::supports(permission) {
        return Some(
            target == TargetForm::Record
                || (permission == &approval_instance::UPGRADE_BINDING && target == TargetForm::Candidate),
        );
    }
    if permission == &supplier_offering::COMPLETE_SUPPLY_EXCEPTION_TASK {
        return Some(target == TargetForm::Task);
    }
    if permission == &purchase_payment_request::CREDIT_LIST || permission == &receivable::INVOICE_POST {
        return Some(target == TargetForm::Task);
    }
    if target == TargetForm::Task && task_supported(permission) {
        return Some(true);
    }
    if permission == &sales_order::SUBMIT && target == TargetForm::Candidate {
        return Some(true);
    }
    if [
        internal::PURCHASE_CREATE_AND_SUBMIT,
        internal::SALES_INQUIRY_IMPORT_CREATE,
        internal::SALES_ORDER_HANDOVER_RECIPIENT,
    ]
    .contains(permission)
    {
        return Some(target == TargetForm::Candidate);
    }
    None
}

pub(super) fn ensure_target(permission: &Permission, kind: ActionKind, target: TargetForm) -> Result<()> {
    if let Some(allowed) = specific_target(permission, target) {
        return if allowed {
            Ok(())
        } else {
            Err(Error::ValidationError("此权限不支持该授权目标".into()))
        };
    }

    let allowed = match kind {
        ActionKind::Function => matches!(target, TargetForm::Operation),
        ActionKind::Task => {
            if records::condition(permission).is_some() {
                matches!(target, TargetForm::Record)
            } else {
                matches!(target, TargetForm::Task)
            }
        },
        ActionKind::Source => {
            if permission.action() == "create" {
                matches!(target, TargetForm::Candidate)
            } else if permission == &purchase_payment_request::UPDATE
                || permission == &receivable::CUSTOMER_RECEIPT_SUBMIT
            {
                matches!(target, TargetForm::Record | TargetForm::OwnershipChange)
            } else {
                matches!(target, TargetForm::Record)
            }
        },
        ActionKind::Scope => match permission.action() {
            "create" => matches!(target, TargetForm::Candidate),
            "update" | "handover" => matches!(target, TargetForm::Record | TargetForm::OwnershipChange),
            _ => matches!(target, TargetForm::Record),
        },
    };
    if allowed { Ok(()) } else { Err(Error::ValidationError("此权限不支持该授权目标".into())) }
}

// 区分允许已有记录读取的动作与必须先进入快照事务的其他对象操作。
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

// 按任务用途生成数据条件：普通读取可以按参与关系；内部材料及办理须当前指派，已完成采购任务还要本人参与。
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

// 从固定目标合同取得必需维度，不以本次范围条款引用了哪些字段为准。公司或记录授权命中也不能掩盖事实缺失。
fn required_facts(permission: &Permission) -> Result<BTreeSet<FactKind>> {
    let spec = scope_spec::require_scope(permission)?;
    let mut facts = BTreeSet::new();
    for dimension in spec.required_dimensions() {
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

fn record_grant(permission: &Permission) -> bool {
    matches!(
        permission.action(),
        "list"
            | "detail"
            | "financial_expense_source"
            | "profit_loss_read"
            | "purchase_expense_source"
            | "extra_cost_read"
            | "customer_quality_history"
            | "delivery_options_read"
    ) && matches!(permission.resource(), "sales_order" | "purchase_order")
}

// 按固定权限登记决定是否能编译查询，不用动作名称推断；可查询不意味相同权限的写入可绕过事务要求。
pub(super) fn query_supported(permission: &Permission) -> Result<bool> {
    scope_spec::kind(permission)?;
    Ok(queries::permissions().any(|entry| entry == permission))
}

// 供 Builder 枚举已接入的全部固定权限规则，不用公共菜单目录代替实际可执行的规则集。
pub(super) fn permissions() -> Vec<&'static Permission> {
    scope_spec::resources().into_iter().flat_map(scope_spec::actions).collect()
}

// 启动校验 Mapping 时展开所有可配置维度，不因某次启动人员范围未配置而漏掉能力检查。
pub(super) fn capability_scope(permission: &Permission) -> Result<ResolvedScope> {
    let spec = scope_spec::require_scope(permission)?;
    if matches!(scope_spec::runtime_source(spec)?.source(), scope_spec::ScopeSource::Company) {
        return Ok(ResolvedScope {
            role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
            user_limit: None,
        });
    }
    let dimensions = spec.supported_dimensions();
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

// 启动时为固定规则列全 Mapping 可能需要的能力；请求期间仍按实际身份与范围编译条件。
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
            let rule = quality::binding_condition(
                permission,
                selection::binding_condition(
                    permission,
                    data_condition(permission, Some(&capability_scope(permission)?))?,
                )?,
            )?;
            if management_permission(permission).is_some() {
                return Ok(AccessRule::all(vec![
                    rule,
                    data_condition(&work_item::MANAGE, Some(&capability_scope(&work_item::MANAGE)?))?,
                ]));
            }
            Ok(rule)
        },
        ActionKind::Task => data_condition(permission, None),
        ActionKind::Function => Err(Error::ValidationError("仅功能操作不绑定查询".into())),
    }
}

// 商品目录查询按真实供应商对象执行数据范围，拥有连接访问资格不等于能读取所有供应商。
pub(super) fn supplier_catalog_condition(rule: AccessRule) -> Result<AccessRule> {
    Ok(AccessRule::Related {
        relation: RelationKey::parse("supplier")?,
        resource: "supplier".into(),
        mode: RelationMatch::AllNonEmpty,
        rule: Box::new(rule),
    })
}

// 为工作项查询指定固定的管理动作过滤，业务调用不能另选查询入口作为管理范围。
pub(crate) fn management_permission(permission: &Permission) -> Option<Permission> {
    (permission == &work_item::LIST || permission == &work_item::DETAIL).then_some(work_item::MANAGE)
}

// 从固定入口提取附加功能资格，为规则内部各分支准备独立取证；客户端不能添加或省略资格。
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

pub(super) fn qualified(permission: &Permission, qualified: &QualifiedPermissions) -> bool {
    if transfers::supports(permission) {
        return transfers::qualified(permission, qualified);
    }
    let required = same_role_permissions(permission);
    (required.is_empty() || qualified.same_role(&required))
        && records::independent(permission).iter().all(|p| qualified.same_role(slice::from_ref(p)))
        && records::qualified(permission, qualified)
}

// 生成可公开的缺权说明时，仅在所有启用角色均未授予且能唯一识别公共权限时报告具体权限码。
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

// 查出精确任务办理目标独有的功能资格，防止把普通读取权限当成可以办理。
pub(super) fn task_permissions(permission: &Permission) -> Vec<Permission> {
    let mut permissions = same_role_permissions(permission);
    if permission == &supplier_settlement::STATEMENT_REVIEW_DECIDE {
        permissions.extend([supplier_settlement::STATEMENT_DETAIL, work_item::DETAIL]);
    }
    permissions.extend(task_execution_permissions(permission));
    permissions.sort();
    permissions.dedup();
    permissions
}

fn task_execution_permissions(permission: &Permission) -> Vec<Permission> {
    match permission.resource() {
        "delivery" => vec![
            fulfillment::DELIVERY_DELIVERY_LIST,
            fulfillment::DELIVERY_DELIVERY_DETAIL,
            fulfillment::DELIVERY_DELIVERY_UPDATE,
            fulfillment::DELIVERY_DELIVERY_POST,
        ],
        "purchase_receipt" => vec![
            fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_LIST,
            fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_DETAIL,
            fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_UPDATE,
            fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_POST,
        ],
        "electronic_delivery" => vec![
            fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_LIST,
            fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CONFIRM,
        ],
        "service_fulfillment" => vec![
            fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_LIST,
            fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CONFIRM,
        ],
        "customer_acceptance" => vec![
            fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_LIST,
            fulfillment::ACCEPTANCE_EVIDENCE_CUSTOMER_ACCEPTANCE_EVIDENCE,
            fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_CREATE,
            fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_COMMIT,
            sales_order::EVIDENCE_SALES_ORDER_EVIDENCE_DOWNLOAD,
        ],
        "supplier_fulfillment_order" if permission == &supplier_fulfillment::ORDER_INVESTIGATION => vec![
            supplier_fulfillment::EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_SETUP,
            supplier_fulfillment::ORDER_INVESTIGATION,
            supplier_fulfillment::ORDER_TASK_COMPLETION,
        ],
        _ => vec![],
    }
}

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

pub(super) fn task_query(permission: &Permission) -> bool {
    permission == &purchase_order::CREATE || permission == &purchase_payment_request::CREDIT_LIST
}

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

    #[test]
    fn reports_a_single_public_permission_missing_from_all_roles() {
        let roles = QualifiedPermissions::test_grants(&[("sales", &[sales_order::SUBMIT])]);

        assert!(!qualified(&sales_order::SUBMIT, &roles));
        assert_eq!(missing_public_permission(&sales_order::SUBMIT, &[], &roles), Some(sales_order::CREATE));
    }

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

    fn scope() -> ResolvedScope {
        ResolvedScope {
            role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
            user_limit: None,
        }
    }
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
