//! 按权限固定的授权规则。对象判断和查询转换共用这里产生的数据条件。
mod queries;
pub(crate) mod sources;
use std::collections::BTreeSet;

use erp_permissions::{approval_instance, cost, purchase_payment_request, sales_order};

use crate::entity::access_control::scope_spec::{self, ActionKind};
use crate::entity::access_control::{DataScopeType, ResolvedScope, ScopeClause, ScopeDimension};
use crate::entity::authorization::{AccessFact, AccessRule, FactKind, QueryOutput};
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

/// 完整功能资格由入口固定，业务输入不能追加或删减。
pub(crate) fn same_role_permissions(permission: &Permission) -> Vec<Permission> {
    let mut required = vec![permission.clone()];
    if permission == &approval_instance::SUBMIT_DECISION {
        required.push(approval_instance::INSTANCE_LIST.clone());
    } else if permission == &sales_order::SUBMIT {
        required.push(sales_order::CREATE);
    } else if permission == &cost::ENTRY_LIST {
        required.push(sales_order::LIST.clone());
    } else if permission == &cost::ENTRY_CREATE {
        required.push(sales_order::DETAIL.clone());
    } else if permission == &purchase_payment_request::CREDIT_LIST {
        required.push(purchase_payment_request::DETAIL);
    }
    required
}

/// 产生对象判断和查询转换共用的数据条件。任务办理和创建专有检查不走这里。
pub(crate) fn data_condition(permission: &Permission, scope: Option<&ResolvedScope>) -> Result<AccessRule> {
    match scope_spec::kind(permission)? {
        ActionKind::Function => Err(Error::ValidationError("仅功能资格的操作没有数据条件".into())),
        ActionKind::Task => Ok(task_condition(permission)),
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
            Ok(rule.requiring(&required_facts(permission)?))
        },
    }
}

pub(super) fn ensure_target(permission: &Permission, kind: ActionKind, target: TargetForm) -> Result<()> {
    let allowed = match kind {
        ActionKind::Function => matches!(target, TargetForm::Operation),
        ActionKind::Task => matches!(target, TargetForm::Task),
        ActionKind::Source => {
            if permission.action() == "create" {
                matches!(target, TargetForm::Candidate)
            } else {
                matches!(target, TargetForm::Record)
            }
        },
        ActionKind::Scope => match permission.action() {
            "create" => matches!(target, TargetForm::Candidate),
            "update" => matches!(target, TargetForm::Record | TargetForm::OwnershipChange),
            _ => matches!(target, TargetForm::Record),
        },
    };
    if allowed { Ok(()) } else { Err(Error::ValidationError("此权限不支持该授权目标".into())) }
}

/// 已有记录的读取动作；其余对象操作必须先进入快照事务。
pub(super) fn is_read(permission: &Permission) -> bool {
    matches!(permission.action(), "list" | "detail" | "get" | "read")
}

/// 普通任务读取允许参与关系；内部资料和办理要求当前指派，已完成采购任务还须本人参与。
fn task_condition(permission: &Permission) -> AccessRule {
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

/// 目标必需维度独立于本次条款是否恰好使用该字段，不能被公司或记录授权分支掩盖。
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
    matches!(permission.action(), "list" | "detail")
        && matches!(permission.resource(), "sales_order" | "purchase_order")
}

/// 查询能力按固定权限声明，不能用动作拼写推断；写入目标的事务要求保持独立。
pub(super) fn query_supported(permission: &Permission) -> Result<bool> {
    scope_spec::kind(permission)?;
    Ok(queries::PERMISSIONS.contains(permission))
}

/// 所有固定规则，不以公共目录条目替代已接入的动作。
pub(super) fn permissions() -> Vec<&'static Permission> {
    scope_spec::resources().into_iter().flat_map(scope_spec::actions).collect()
}

/// 用所有可配置维度检查 Mapping，不依赖启动时偶然的人员范围配置。
fn capability_scope(permission: &Permission) -> Result<ResolvedScope> {
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

/// 固定条件的全部可能能力；运行时仍按真实资格和范围生成条件。
pub(crate) fn binding_condition(permission: &Permission, output: &QueryOutput) -> Result<AccessRule> {
    let kind = scope_spec::kind(permission)?;
    match kind {
        ActionKind::Source => {
            let output =
                if *output == QueryOutput::Object { sources::Output::Object } else { sources::Output::Share };
            sources::binding_condition(permission, output)
        },
        _ if *output != QueryOutput::Object => Err(Error::ValidationError("此权限不支持受限输出".into())),
        ActionKind::Scope => data_condition(permission, Some(&capability_scope(permission)?)),
        ActionKind::Task => data_condition(permission, None),
        ActionKind::Function => Err(Error::ValidationError("仅功能操作不绑定查询".into())),
    }
}

#[cfg(test)]
mod tests {
    use erp_permissions::sales_inquiry as inquiry;

    use super::*;
    use crate::entity::authorization::{FactValue, ObjectFacts};

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
