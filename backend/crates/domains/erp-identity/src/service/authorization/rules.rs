//! 按权限固定的授权规则。对象判断和查询转换共用这里产生的数据条件。
//! 统一服务在第四阶段调用这些函数；在此之前由单元测试执行。
#![allow(dead_code)]
pub(crate) mod sources;
use std::collections::BTreeSet;

use erp_permissions::{cost, sales_order};

use crate::entity::access_control::scope_spec::{self, ActionKind};
use crate::entity::access_control::{ResolvedScope, ScopeDimension};
use crate::entity::authorization::{AccessFact, AccessRule, FactKind, ObjectFacts};
use crate::entity::resource_access::ResourceKind;
use crate::{Error, Permission, QualifiedPermissions, Result};

/// 调用方可以提交的目标形态。权限自己决定哪些形态有效。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TargetForm {
    Operation,
    Record,
    Candidate,
    OwnershipChange,
    Task,
}

/// 一次规则判断的输入。身份资格由调用方在同一次解析中取好后复用。
pub(crate) struct RuleInput<'a> {
    pub target: TargetForm,
    pub qualified: &'a QualifiedPermissions,
    pub scope: Option<&'a ResolvedScope>,
    pub object: Option<&'a ObjectFacts>,
    pub candidate: Option<&'a ObjectFacts>,
    pub record_granted: bool,
    pub sources: Option<&'a sources::Input<'a>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decision {
    Allow,
    Deny,
}

/// 完整功能资格由入口固定，业务输入不能追加或删减。
pub(crate) fn same_role_permissions(permission: &Permission) -> Vec<Permission> {
    let mut required = vec![permission.clone()];
    if permission == &erp_permissions::approval_instance::SUBMIT_DECISION {
        required.push(erp_permissions::approval_instance::INSTANCE_LIST.clone());
    } else if permission == &sales_order::SUBMIT {
        required.push(sales_order::CREATE);
    } else if permission == &cost::ENTRY_LIST {
        required.push(sales_order::LIST.clone());
    } else if permission == &cost::ENTRY_CREATE {
        required.push(sales_order::DETAIL.clone());
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

pub(crate) fn evaluate(permission: &Permission, input: &RuleInput<'_>) -> Result<Decision> {
    let kind = scope_spec::kind(permission)?;
    ensure_target(permission, kind, input.target)?;
    let required = same_role_permissions(permission);
    if !input.qualified.same_role(&required) {
        return Err(Error::Forbidden("没有启用角色提供本操作的完整权限".into()));
    }
    match kind {
        ActionKind::Function => Ok(Decision::Allow),
        ActionKind::Task => evaluate_task(permission, input),
        ActionKind::Source => evaluate_source(permission, input),
        ActionKind::Scope => evaluate_scope(permission, input),
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

/// 列表读取允许参与关系；办理条件必须另行验证当前指派。
fn task_condition(permission: &Permission) -> AccessRule {
    if matches!(permission.action(), "list" | "detail" | "read") {
        AccessRule::any(vec![
            AccessRule::Fact(AccessFact::AssignedTo),
            AccessRule::Fact(AccessFact::Participant),
        ])
    } else {
        AccessRule::Fact(AccessFact::AssignedTo)
    }
}

/// 采购任务完成后必须同时证明原指派与参与，未读取的必要事实不能被并集掩盖。
fn evaluate_task(permission: &Permission, input: &RuleInput<'_>) -> Result<Decision> {
    let object = input.object.ok_or_else(|| Error::ValidationError("精确任务事实未读取".into()))?;
    let allowed = task_allowed(permission, &input.qualified.account().id, object)?;
    Ok(if allowed { Decision::Allow } else { Decision::Deny })
}

/// 办理与查询共用真实任务事实，但办理不能用参与关系代替指派。
pub(super) fn task_allowed(permission: &Permission, actor: &str, object: &ObjectFacts) -> Result<bool> {
    let assigned = object.assignee_id.get()?.as_deref() == Some(actor);
    let readable = matches!(permission.action(), "list" | "detail" | "read");
    let completed =
        if permission.resource() == "procurement_inquiry" { *object.completed.get()? } else { false };
    let allowed = if readable || completed {
        let participant = object.participants.get()?.contains(actor);
        if readable { assigned || participant } else { assigned && participant }
    } else {
        assigned
    };
    Ok(allowed)
}

/// 来源输入由实际对象、范围及授权行构成，不能传入预先计算的允许布尔值。
fn evaluate_source(permission: &Permission, input: &RuleInput<'_>) -> Result<Decision> {
    let sources = input.sources.ok_or_else(|| Error::ValidationError("来源事实未读取".into()))?;
    sources::evaluate(permission, input.qualified, sources)
}

fn evaluate_scope(permission: &Permission, input: &RuleInput<'_>) -> Result<Decision> {
    if matches!(input.target, TargetForm::OwnershipChange) {
        let current = input.object.ok_or_else(|| Error::ValidationError("对象事实未读取".into()))?;
        let candidate = input.candidate.ok_or_else(|| Error::ValidationError("目标归属事实未读取".into()))?;
        let current =
            decide_evidence(permission, input.scope, current, false, &input.qualified.account().id)?;
        let candidate =
            decide_evidence(permission, input.scope, candidate, false, &input.qualified.account().id)?;
        return Ok(if current == Decision::Allow && candidate == Decision::Allow {
            Decision::Allow
        } else {
            Decision::Deny
        });
    }
    let evidence = match input.target {
        TargetForm::Candidate => input.candidate,
        _ => input.object,
    }
    .ok_or_else(|| Error::ValidationError("对象事实未读取".into()))?;
    let grant = input.record_granted && record_grant(permission);
    decide_evidence(permission, input.scope, evidence, grant, &input.qualified.account().id)
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

/// 对象事实直接执行与查询转换相同的数据条件。
fn decide_evidence(
    permission: &Permission,
    scope: Option<&ResolvedScope>,
    evidence: &ObjectFacts,
    record_granted: bool,
    actor: &str,
) -> Result<Decision> {
    let rule = data_condition(permission, scope)?;
    let mut grants = BTreeSet::new();
    if record_granted {
        let resource = if evidence.resource == "sales_order" {
            ResourceKind::SalesOrder
        } else {
            ResourceKind::PurchaseOrder
        };
        grants.insert((resource, evidence.id.clone()));
    }
    let allowed = rule.allows_object(actor, evidence, &grants)?;
    Ok(if allowed { Decision::Allow } else { Decision::Deny })
}

fn record_grant(permission: &Permission) -> bool {
    matches!(permission.action(), "list" | "detail")
        && matches!(permission.resource(), "sales_order" | "purchase_order")
}

#[cfg(test)]
mod tests {
    use erp_permissions::sales_inquiry as inquiry;

    use super::*;
    use crate::QualifiedPermissions;
    use crate::entity::access_control::ScopeClause;
    use crate::entity::authorization::FactValue;

    fn grants(pairs: &[(&str, &[Permission])]) -> QualifiedPermissions {
        QualifiedPermissions::test_grants(pairs)
    }

    fn self_scope() -> ResolvedScope {
        ResolvedScope {
            role_clauses: vec![ScopeClause { self_owned: true, ..ScopeClause::default() }],
            user_limit: None,
        }
    }

    fn owned(owner: Option<bool>) -> ObjectFacts {
        ObjectFacts {
            resource: "sales_order".into(),
            id: "record".into(),
            version: "v1".into(),
            owner_id: owner
                .map(|owned| FactValue::Known(Some(if owned { "user" } else { "other" }.into())))
                .unwrap_or(FactValue::Missing),
            org_unit_id: FactValue::Known(None),
            ..ObjectFacts::default()
        }
    }

    fn input<'a>(
        qualified: &'a QualifiedPermissions,
        scope: Option<&'a ResolvedScope>,
        object: Option<&'a ObjectFacts>,
    ) -> RuleInput<'a> {
        RuleInput {
            target: TargetForm::Record,
            qualified,
            scope,
            object,
            candidate: None,
            record_granted: false,
            sources: None,
        }
    }

    #[test]
    fn inquiry_update_uses_owner_scope_and_reports_missing_facts() {
        let permission = &inquiry::COMMANDS_SALES_SAVE;
        let qualified = grants(&[("sales", std::slice::from_ref(permission))]);
        let scope = self_scope();
        let present = owned(Some(true));
        let mut request = input(&qualified, Some(&scope), Some(&present));
        assert_eq!(evaluate(permission, &request).unwrap(), Decision::Allow);
        let other = owned(Some(false));
        request.object = Some(&other);
        assert_eq!(evaluate(permission, &request).unwrap(), Decision::Deny);
        let missing = owned(None);
        request.object = Some(&missing);
        assert!(
            matches!(evaluate(permission, &request), Err(Error::ValidationError(message)) if message.starts_with("缺少必要授权事实"))
        );
        let empty = ResolvedScope { role_clauses: vec![], user_limit: None };
        request.scope = Some(&empty);
        request.object = Some(&present);
        assert_eq!(evaluate(permission, &request).unwrap(), Decision::Deny);
        let limited = ResolvedScope {
            role_clauses: vec![ScopeClause { self_owned: true, ..ScopeClause::default() }],
            user_limit: Some(ScopeClause::default()),
        };
        request.scope = Some(&limited);
        assert_eq!(evaluate(permission, &request).unwrap(), Decision::Deny);
        request.target = TargetForm::Operation;
        assert!(evaluate(permission, &request).is_err());
        assert!(matches!(data_condition(permission, Some(&scope)).unwrap(), AccessRule::Required { .. }));
    }

    #[test]
    fn sales_detail_record_grant_supplements_read_after_personal_limit() {
        let permission = &sales_order::DETAIL;
        let update = &sales_order::HANDOVER;
        let qualified = grants(&[("sales", &[permission.clone(), update.clone()])]);
        let scope = ResolvedScope { role_clauses: vec![], user_limit: None };
        let evidence = ObjectFacts {
            history_participants: FactValue::Known(["user".into()].into()),
            ..owned(Some(false))
        };
        let mut request = input(&qualified, Some(&scope), Some(&evidence));
        assert_eq!(evaluate(permission, &request).unwrap(), Decision::Allow);
        let limited = ResolvedScope { role_clauses: vec![], user_limit: Some(ScopeClause::default()) };
        request.scope = Some(&limited);
        assert_eq!(evaluate(permission, &request).unwrap(), Decision::Deny);
        request.record_granted = true;
        assert_eq!(evaluate(permission, &request).unwrap(), Decision::Allow);
        request.record_granted = true;
        assert_eq!(evaluate(update, &request).unwrap(), Decision::Deny);
    }

    #[test]
    fn fixed_qualification_cannot_omit_approval_read_or_restrict_normal_sales() {
        let decide = erp_permissions::approval_instance::SUBMIT_DECISION;
        let read = erp_permissions::approval_instance::INSTANCE_LIST;
        let split =
            grants(&[("decide", std::slice::from_ref(&decide)), ("read", std::slice::from_ref(&read))]);
        let mut request = input(&split, None, None);
        request.target = TargetForm::Task;
        let task = ObjectFacts { assignee_id: FactValue::Known(Some("user".into())), ..owned(Some(true)) };
        request.object = Some(&task);
        assert!(matches!(evaluate(&decide, &request), Err(Error::Forbidden(_))));
        let missing = grants(&[("decide", std::slice::from_ref(&decide))]);
        request.qualified = &missing;
        assert!(matches!(evaluate(&decide, &request), Err(Error::Forbidden(_))));
        let together = grants(&[("both", &[decide.clone(), read])]);
        request.qualified = &together;
        assert_eq!(evaluate(&decide, &request).unwrap(), Decision::Allow);
        assert_eq!(same_role_permissions(&sales_order::DETAIL), vec![sales_order::DETAIL]);
        assert_eq!(same_role_permissions(&sales_order::LIST), vec![sales_order::LIST]);
        assert_eq!(same_role_permissions(&cost::ENTRY_LIST), vec![cost::ENTRY_LIST, sales_order::LIST]);
        assert_eq!(same_role_permissions(&cost::ENTRY_CREATE), vec![cost::ENTRY_CREATE, sales_order::DETAIL]);
    }

    #[test]
    fn task_handling_requires_assignment_and_rejects_a_plain_record() {
        let permission = &inquiry::COMMANDS_PROCUREMENT_RENEW;
        let qualified = grants(&[("buyer", std::slice::from_ref(permission))]);
        let decide = |task: &ObjectFacts| {
            let mut request = input(&qualified, None, Some(task));
            request.target = TargetForm::Task;
            evaluate(permission, &request)
        };
        let mut task = ObjectFacts {
            assignee_id: FactValue::Known(Some("user".into())),
            completed: FactValue::Known(false),
            ..owned(Some(true))
        };
        assert_eq!(decide(&task).unwrap(), Decision::Allow);
        task.assignee_id = FactValue::Known(None);
        task.participants = FactValue::Known(BTreeSet::new());
        assert_eq!(decide(&task).unwrap(), Decision::Deny);
        task.participants = FactValue::Known(["user".into()].into());
        assert_eq!(decide(&task).unwrap(), Decision::Deny);
        task.assignee_id = FactValue::Missing;
        assert!(matches!(decide(&task), Err(Error::ValidationError(_))));
        task.assignee_id = FactValue::Known(Some("user".into()));
        task.completed = FactValue::Known(true);
        task.participants = FactValue::Known(BTreeSet::new());
        assert_eq!(decide(&task).unwrap(), Decision::Deny);
        task.participants = FactValue::Missing;
        assert!(matches!(decide(&task), Err(Error::ValidationError(_))));
        task.participants = FactValue::Known(["user".into()].into());
        assert_eq!(decide(&task).unwrap(), Decision::Allow);
        let request = input(&qualified, None, Some(&task));
        assert!(matches!(evaluate(permission, &request), Err(Error::ValidationError(_))));
        assert!(scope_spec::find(permission).is_none());
    }

    #[test]
    fn candidate_and_ownership_change_share_one_scope_rule() {
        let create = &inquiry::COMMANDS_SALES_CREATE;
        let update = &inquiry::COMMANDS_SALES_SAVE;
        let qualified = grants(&[("sales", &[create.clone(), update.clone()])]);
        let scope = self_scope();
        let owner = owned(Some(true));
        let stranger = owned(Some(false));
        let mut request = input(&qualified, Some(&scope), None);
        request.target = TargetForm::Candidate;
        request.candidate = Some(&owner);
        assert_eq!(evaluate(create, &request).unwrap(), Decision::Allow);
        request.target = TargetForm::OwnershipChange;
        request.object = Some(&owner);
        request.candidate = Some(&stranger);
        assert_eq!(evaluate(update, &request).unwrap(), Decision::Deny);
        request.candidate = Some(&owner);
        assert_eq!(evaluate(update, &request).unwrap(), Decision::Allow);
    }

    #[test]
    fn customer_visibility_does_not_grant_contract() {
        let contract = &erp_permissions::contract::DETAIL;
        let contract_user = grants(&[("sales", std::slice::from_ref(contract))]);
        let scope = self_scope();
        let mut owner = owned(Some(true));
        owner.history_participants = FactValue::Known(BTreeSet::new());
        let mut contract_request = input(&contract_user, Some(&scope), Some(&owner));
        assert_eq!(runtime_customer_source(), "customer:list");
        assert_eq!(evaluate(contract, &contract_request).unwrap(), Decision::Allow);
        let mut stranger = owned(Some(false));
        stranger.history_participants = FactValue::Known(BTreeSet::new());
        contract_request.object = Some(&stranger);
        assert_eq!(evaluate(contract, &contract_request).unwrap(), Decision::Deny);
    }

    fn runtime_customer_source() -> String {
        let spec = scope_spec::find(&erp_permissions::contract::DETAIL).unwrap();
        scope_spec::runtime_source(spec).unwrap().permission().to_string()
    }
}
