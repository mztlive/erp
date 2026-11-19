//! 固定来源规则及输出边界；对象执行和后续查询转换消费同一分区规则。
use std::borrow::Cow;
use std::collections::BTreeSet;

use erp_permissions::{
    payable, policy_permission, purchase_order, receivable, sales_order, supplier_settlement,
};

use super::{ScopeLookup, same_role_permissions};
use crate::entity::access_control::ResolvedScope;
use crate::entity::access_control::scope_spec::{self, ActionKind};
use crate::entity::authorization::{AccessFact, AccessRule, FactKind, RelationKey, RelationMatch};
use crate::entity::resource_access::ResourceKind;
use crate::{Error, Permission, QualifiedPermissions, Result};

type SourceScopes<'a, 'f> = dyn Fn(&Permission, &Permission) -> Result<Cow<'a, ResolvedScope>> + 'f;

/// 完整对象和匹配份额的判断不可互换；存在性也只能消费匹配份额结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Output {
    Object,
    Share,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Sales,
    Purchase,
    Settlement,
    Receipt,
    Receivable,
    Payment,
    Payable,
}

// 登记实际来源范围及其取证入口；退款追溯到资金根时单独使用该根的详情资格。
pub(crate) fn scope_requests(permission: &Permission) -> Result<Vec<(Permission, Permission)>> {
    let mut requests = Vec::new();
    for source in sources(permission)? {
        let source_permission = source.permission();
        if matches!(source, Source::Receipt | Source::Receivable | Source::Payment | Source::Payable) {
            requests.extend(scope_requests(&source_permission)?);
        } else {
            requests.push((permission.clone(), source_permission));
        }
    }
    Ok(requests)
}

// 同次身份取证覆盖根入口和独立来源入口，整账权限只在固定可选分支取用。
pub(crate) fn qualifications(permission: &Permission) -> Result<Vec<Permission>> {
    let mut permissions = same_role_permissions(permission);
    permissions.push(policy_permission::FINANCE_LEDGER_READ);
    for source in sources(permission)? {
        if matches!(source, Source::Receipt | Source::Receivable | Source::Payment | Source::Payable) {
            permissions.extend(qualifications(&source.permission())?);
        }
    }
    permissions.sort();
    permissions.dedup();
    Ok(permissions)
}

// 启动验证来源 Mapping 时检查所有固定执行分支，不因当前账号或人员范围暂未启用某分支而跳过。
pub(super) fn binding_condition(permission: &Permission, output: Output) -> Result<AccessRule> {
    condition_with(
        permission,
        &|_| true,
        &|entry, source| required_scope(&ScopeLookup::Capability, entry, source),
        output,
    )
}

impl Source {
    // 由固定来源规则取得稳定关联键；业务提供该键的真实关系，不自行改选来源。
    fn relation(self) -> &'static str {
        match self {
            Self::Sales => "sales_source",
            Self::Purchase => "purchase_source",
            Self::Settlement => "settlement_source",
            Self::Receipt => "receipt_source",
            Self::Receivable => "receivable_source",
            Self::Payment => "payment_source",
            Self::Payable => "payable_source",
        }
    }

    // 区分来源资格上下文：独立资金来源用相应详情入口，订单和结算条款按当前入口解释。
    fn permission(self) -> Permission {
        match self {
            Self::Sales => sales_order::DETAIL,
            Self::Purchase => purchase_order::DETAIL,
            Self::Settlement => supplier_settlement::STATEMENT_LIST,
            Self::Receipt => receivable::CUSTOMER_RECEIPT_DETAIL,
            Self::Receivable => receivable::ACCOUNT_DETAIL,
            Self::Payment => payable::SUPPLIER_PAYMENT_DETAIL,
            Self::Payable => payable::ACCOUNT_DETAIL,
        }
    }
}

// 为已登记的来源入口列出实际业务来源，不能只返回一个笼统分类让调用方选择规则。
fn sources(permission: &Permission) -> Result<Vec<Source>> {
    let sources = match permission.resource() {
        "cost_entry" if permission.action() == "create" => vec![Source::Sales],
        "customer_receipt" if permission.action() == "submit" => vec![Source::Sales],
        "invoice" if matches!(permission.action(), "post" | "posted_result_read") => vec![Source::Sales],
        "receivable_account" | "sales_invoice_request" => {
            vec![Source::Sales]
        },
        "purchase_payment_request" => vec![Source::Purchase],
        "payable_account" => vec![Source::Purchase, Source::Settlement],
        "purchase_invoice_allocation" => vec![Source::Sales, Source::Purchase, Source::Settlement],
        "cost_entry" | "cost_allocation" | "customer_receipt" | "supplier_payment" | "invoice" => {
            vec![Source::Sales, Source::Purchase, Source::Settlement]
        },
        "customer_refund" => vec![Source::Receipt, Source::Receivable],
        "supplier_refund" => vec![Source::Payment, Source::Payable],
        "receipt_reversal" => vec![Source::Receipt],
        "payment_reversal" => vec![Source::Payment],
        _ => return Err(Error::ValidationError("此权限没有固定来源规则".into())),
    };
    Ok(sources)
}

// 只有登记为资金份额的查询才允许部分来源输出；命令和付款申请不能省略任何真实来源。
fn allows_shares(permission: &Permission) -> bool {
    matches!(
        permission.resource(),
        "customer_receipt"
            | "supplier_payment"
            | "invoice"
            | "cost_entry"
            | "cost_allocation"
            | "purchase_invoice_allocation"
    ) && matches!(permission.action(), "list" | "detail")
}

// 复合财务入口只输出原规则登记方向的来源份额；整账过滤仍以全部真实来源为准。
fn share_source(permission: &Permission, source: Source) -> bool {
    match permission.resource() {
        "customer_receipt" | "cost_entry" | "cost_allocation" => source == Source::Sales,
        "supplier_payment" => matches!(source, Source::Purchase | Source::Settlement),
        _ => true,
    }
}

// 读取完整资金金额时要求固定入口和整账资格同角色满足；没有分配的真实记录也不能跳过这一条件。
fn needs_ledger(permission: &Permission) -> bool {
    matches!(
        permission.resource(),
        "cost_entry" | "cost_allocation" | "customer_receipt" | "supplier_payment" | "invoice"
    ) && matches!(permission.action(), "list" | "detail")
}

// 为整账分支检查完整金额的同角色功能资格；来源可见性仍须由后续范围和完整来源条件证明。
pub(crate) fn whole_qualified(permission: &Permission, qualified: &QualifiedPermissions) -> bool {
    qualified.same_role(&same_role_permissions(permission))
        && (!needs_ledger(permission)
            || qualified.same_role(&[permission.clone(), policy_permission::FINANCE_LEDGER_READ]))
}

// 收款提交必须在完整来源分区之外同时满足收款记录的经办责任。
fn condition_with(
    permission: &Permission,
    qualifies: &dyn Fn(&[Permission]) -> bool,
    scopes: &SourceScopes<'_, '_>,
    output: Output,
) -> Result<AccessRule> {
    let rule = source_condition(permission, qualifies, scopes, output)?;
    if permission == &receivable::CUSTOMER_RECEIPT_SUBMIT {
        return Ok(AccessRule::all(vec![rule, AccessRule::Fact(AccessFact::Owned)])
            .requiring(&BTreeSet::from([FactKind::Owner])));
    }
    Ok(rule)
}

// 按真实完整来源确定分区，不能用已获准来源的子集判断是否具备整账资格。
fn source_condition(
    permission: &Permission,
    qualifies: &dyn Fn(&[Permission]) -> bool,
    scopes: &SourceScopes<'_, '_>,
    output: Output,
) -> Result<AccessRule> {
    if scope_spec::kind(permission)? != ActionKind::Source {
        return Err(Error::ValidationError("此入口不使用来源分区规则".into()));
    }
    if !qualifies(&same_role_permissions(permission)) {
        return Err(Error::Forbidden("没有启用角色提供来源入口的完整权限".into()));
    }
    if output == Output::Share && !allows_shares(permission) {
        return Err(Error::ValidationError("此权限不允许份额输出".into()));
    }

    let sources = sources(permission)?;
    if matches!(
        permission.resource(),
        "customer_refund" | "supplier_refund" | "receipt_reversal" | "payment_reversal"
    ) {
        return refund_conditions(&sources, qualifies, scopes);
    }

    let ledger = output == Output::Object && needs_ledger(permission);
    let ledger_allowed = qualifies(&[permission.clone(), policy_permission::FINANCE_LEDGER_READ]);
    let mut partitions = Vec::new();
    for mask in 0..(1usize << sources.len()) {
        if mask == 0 && !ledger && permission != &receivable::CUSTOMER_RECEIPT_SUBMIT {
            continue;
        }
        if permission.resource() == "payable_account" && mask.count_ones() != 1 {
            continue;
        }

        let rules = sources
            .iter()
            .copied()
            .enumerate()
            .filter(|(index, source)| {
                source_present(mask, *index)
                    && (output == Output::Object || share_source(permission, *source))
            })
            .map(|(_, source)| source_rule(permission, source, scopes, output))
            .collect::<Result<Vec<_>>>()?;
        let rule = if ledger && !ledger_allowed {
            AccessRule::Deny
        } else if mask == 0 {
            AccessRule::Allow
        } else if output == Output::Share {
            AccessRule::any(rules)
        } else {
            AccessRule::all(rules)
        };
        partitions.push(AccessRule::all(with_presence(rule, &sources, mask)?));
    }
    Ok(AccessRule::any(partitions))
}

fn source_present(mask: usize, index: usize) -> bool {
    mask & (1 << index) != 0
}

fn with_presence(rule: AccessRule, sources: &[Source], mask: usize) -> Result<Vec<AccessRule>> {
    let mut rules = vec![rule];
    // 先检查存在的引用，再检查缺席的引用，保持规则和查询阶段的顺序。
    for present in [true, false] {
        for (index, source) in sources.iter().enumerate() {
            if source_present(mask, index) == present {
                rules.push(AccessRule::RelationPresence {
                    relation: RelationKey::parse(source.relation())?,
                    present,
                });
            }
        }
    }
    Ok(rules)
}

// 为实际来源保留普通范围或有效记录授权的读取分支，不再附加来源菜单功能权限。
fn source_rule(
    entry: &Permission,
    source: Source,
    scopes: &SourceScopes<'_, '_>,
    output: Output,
) -> Result<AccessRule> {
    let permission = source.permission();
    let scope = scopes(entry, &permission)?;
    let mut rule = AccessRule::scope(&scope, false);
    let resource = match source {
        Source::Sales => Some(ResourceKind::SalesOrder),
        Source::Purchase => Some(ResourceKind::PurchaseOrder),
        _ => None,
    };
    if let Some(resource) = resource {
        rule = AccessRule::any(vec![rule, AccessRule::RecordReadGrant(resource)]);
    }
    if matches!(source, Source::Receipt | Source::Receivable | Source::Payment | Source::Payable) {
        // 财务来源必须由自己的固定完整对象规则取证，不能把根的归属范围当成资金授权。
        return Err(Error::ValidationError("财务根不能作为订单范围解析".into()));
    }
    rule = rule.requiring(&BTreeSet::from([FactKind::Owner, FactKind::Organization]));
    Ok(AccessRule::Related {
        relation: RelationKey::parse(source.relation())?,
        resource: permission.resource().into(),
        mode: if output == Output::Object { RelationMatch::AllNonEmpty } else { RelationMatch::Any },
        rule: Box::new(rule),
    })
}

// 按授予来源入口的角色解析各自数据范围，退款涉及的不同资金根不能复用同一份范围结果。
pub(crate) fn scoped_condition(
    permission: &Permission,
    qualified: &QualifiedPermissions,
    scopes: &[(Permission, Permission, ResolvedScope)],
    output: Output,
) -> Result<AccessRule> {
    let scopes = ScopeLookup::Resolved(scopes);
    condition_with(
        permission,
        &|required| qualified.same_role(required),
        &|entry, source| required_scope(&scopes, entry, source),
        output,
    )
}

fn required_scope<'a>(
    scopes: &ScopeLookup<'a>,
    entry: &Permission,
    source: &Permission,
) -> Result<Cow<'a, ResolvedScope>> {
    scopes.get(entry, source, || Error::ValidationError(format!("缺少来源范围：{entry} -> {source}")))
}

// 审批管理取证已证明完整同角色功能组合后，只用这组合格角色解析来源范围，不额外要求来源菜单。
pub(crate) fn management_condition(
    permission: &Permission,
    scopes: &[(Permission, ResolvedScope)],
) -> Result<AccessRule> {
    let lookup = |_: &Permission, source: &Permission| {
        scopes
            .iter()
            .find(|(key, _)| key == source)
            .map(|(_, scope)| Cow::Borrowed(scope))
            .ok_or_else(|| Error::ValidationError("缺少审批管理来源范围".into()))
    };
    source_condition(permission, &|_| true, &lookup, Output::Object)
}

// 局部测试提供按来源索引的范围，最终条件仍走实际规则。
#[cfg(test)]
pub(crate) fn data_condition(
    permission: &Permission,
    qualified: &QualifiedPermissions,
    scopes: &[(Permission, ResolvedScope)],
    output: Output,
) -> Result<AccessRule> {
    condition_with(
        permission,
        &|required| qualified.same_role(required),
        &|_, permission| {
            scopes
                .iter()
                .find(|(key, _)| key == permission)
                .map(|(_, scope)| Cow::Borrowed(scope))
                .ok_or_else(|| Error::ValidationError(format!("缺少来源范围：{permission}")))
        },
        output,
    )
}

// 退款条件要求唯一真实资金根、各入口自己的详情资格，以及原经办责任，不能只凭其中一项读取。
fn refund_conditions(
    sources: &[Source],
    qualifies: &dyn Fn(&[Permission]) -> bool,
    scopes: &SourceScopes<'_, '_>,
) -> Result<AccessRule> {
    let mut partitions = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let permission = source.permission();
        let nested = if qualifies(&same_role_permissions(&permission)) {
            source_condition(&permission, qualifies, scopes, Output::Object)?
        } else {
            AccessRule::Deny
        };
        let mut rules = with_presence(AccessRule::Fact(AccessFact::Owned), sources, 1 << index)?;
        rules.push(AccessRule::Related {
            relation: RelationKey::parse(source.relation())?,
            resource: permission.resource().into(),
            mode: RelationMatch::ExactlyOne,
            rule: Box::new(AccessRule::Independent { permission, rule: Box::new(nested) }),
        });
        partitions.push(AccessRule::all(rules));
    }
    Ok(AccessRule::any(partitions))
}

#[cfg(test)]
mod tests {
    use erp_permissions::{cost, purchase_payment_request, returns};

    use super::*;
    use crate::entity::access_control::ScopeClause;
    use crate::entity::access_control::scope_spec::{self, ActionKind};
    use crate::entity::authorization::{FactValue, ObjectFacts, RelatedObjects};

    fn qualifications(permission: &Permission, ledger: bool) -> QualifiedPermissions {
        let mut required = same_role_permissions(permission);
        if ledger {
            required.push(policy_permission::FINANCE_LEDGER_READ);
        }
        QualifiedPermissions::test_grants(&[("role", &required)])
    }
    fn scopes() -> Vec<(Permission, ResolvedScope)> {
        vec![
            (
                sales_order::DETAIL,
                ResolvedScope {
                    role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
                    user_limit: None,
                },
            ),
            (
                purchase_order::DETAIL,
                ResolvedScope {
                    role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                    user_limit: None,
                },
            ),
            (
                supplier_settlement::STATEMENT_LIST,
                ResolvedScope {
                    role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                    user_limit: None,
                },
            ),
        ]
    }
    fn child(id: &str, owner: &str) -> ObjectFacts {
        ObjectFacts {
            resource: "sales_order".into(),
            id: id.into(),
            version: "1".into(),
            owner_id: FactValue::Known(Some(owner.into())),
            org_unit_id: FactValue::Known(Some("org".into())),
            ..Default::default()
        }
    }
    fn root(permission: &Permission, children: Vec<ObjectFacts>) -> ObjectFacts {
        let mut object = ObjectFacts {
            resource: permission.resource().into(),
            id: "root".into(),
            version: "1".into(),
            owner_id: FactValue::Known(None),
            ..Default::default()
        };
        for source in sources(permission).unwrap() {
            object.relations.insert(
                RelationKey::parse(source.relation()).unwrap(),
                RelatedObjects { expected_ids: BTreeSet::new(), objects: vec![] },
            );
        }
        if let Some(relation) = object.relations.get_mut(&RelationKey::parse("sales_source").unwrap()) {
            relation.expected_ids = children.iter().map(|child| child.id.clone()).collect();
            relation.objects = children;
        }
        object
    }
    fn check(
        permission: &Permission,
        qualified: &QualifiedPermissions,
        object: &ObjectFacts,
        scopes: &[(Permission, ResolvedScope)],
        grants: &BTreeSet<(ResourceKind, String)>,
    ) -> Result<bool> {
        data_condition(permission, qualified, scopes, Output::Object)?.allows_object("actor", object, grants)
    }
    #[test]
    fn cost_source_uses_scope_or_grant_and_requires_complete_facts_and_sources() {
        let permission = cost::ENTRY_LIST;
        let qualified = qualifications(&permission, true);
        let scopes = scopes();
        let grants = BTreeSet::new();

        let mut object = root(&permission, vec![child("s1", "actor")]);
        assert!(check(&permission, &qualified, &object, &scopes, &grants).unwrap());
        object = root(&permission, vec![child("s1", "other")]);
        assert!(!check(&permission, &qualified, &object, &scopes, &grants).unwrap());
        let grants = BTreeSet::from([(ResourceKind::SalesOrder, "s1".into())]);
        assert!(check(&permission, &qualified, &object, &scopes, &grants).unwrap());

        object.relations.get_mut(&RelationKey::parse("sales_source").unwrap()).unwrap().objects[0].owner_id =
            FactValue::Missing;
        assert!(matches!(
            check(&permission, &qualified, &object, &scopes, &grants),
            Err(Error::ValidationError(_))
        ));

        object = root(&permission, vec![child("s1", "actor"), child("s2", "other")]);
        assert!(!check(&permission, &qualified, &object, &scopes, &grants).unwrap());
        object.relations.get_mut(&RelationKey::parse("sales_source").unwrap()).unwrap().objects.pop();
        assert!(!check(&permission, &qualified, &object, &scopes, &grants).unwrap());
        assert!(check(&permission, &qualified, &root(&permission, vec![]), &scopes, &grants).unwrap());

        let split = QualifiedPermissions::test_grants(&[
            ("cost", std::slice::from_ref(&permission)),
            ("sales", &[sales_order::LIST]),
        ]);
        assert!(matches!(check(&permission, &split, &object, &scopes, &grants), Err(Error::Forbidden(_))));
    }
    #[test]
    fn cost_shares_use_normal_scope_without_ledger_and_creation_uses_fixed_detail_qualification() {
        let permission = cost::ENTRY_LIST;
        let object = root(&permission, vec![child("s1", "actor")]);
        let qualified = qualifications(&permission, false);
        let share = data_condition(&permission, &qualified, &scopes(), Output::Share).unwrap();
        assert!(share.allows_object("actor", &object, &BTreeSet::new()).unwrap());
        assert!(!check(&permission, &qualified, &object, &scopes(), &BTreeSet::new()).unwrap());

        let create = cost::ENTRY_CREATE;
        let object = root(&create, vec![child("s1", "actor")]);
        assert!(
            check(&create, &qualifications(&create, false), &object, &scopes(), &BTreeSet::new()).unwrap()
        );
        let split = QualifiedPermissions::test_grants(&[
            ("cost", std::slice::from_ref(&create)),
            ("sales", &[sales_order::DETAIL]),
        ]);
        assert!(matches!(
            check(&create, &split, &object, &scopes(), &BTreeSet::new()),
            Err(Error::Forbidden(_))
        ));
    }

    #[test]
    fn purchase_and_settlement_require_the_actual_complete_partition() {
        for (permission, source) in [
            (purchase_payment_request::DETAIL, Source::Purchase),
            (payable::ACCOUNT_DETAIL, Source::Settlement),
        ] {
            let qualified = qualifications(&permission, false);
            let mut object = root(&permission, vec![]);
            let mut child = child("p1", "other");
            child.resource = source.permission().resource().into();
            object.relations.insert(
                RelationKey::parse(source.relation()).unwrap(),
                RelatedObjects { expected_ids: ["p1".into()].into(), objects: vec![child.clone()] },
            );
            assert!(check(&permission, &qualified, &object, &scopes(), &BTreeSet::new()).unwrap());

            object
                .relations
                .get_mut(&RelationKey::parse(source.relation()).unwrap())
                .unwrap()
                .objects
                .push(child);
            assert!(matches!(
                check(&permission, &qualified, &object, &scopes(), &BTreeSet::new()),
                Err(Error::ValidationError(_))
            ));
        }
    }

    #[test]
    fn financial_shares_never_grant_whole_amount_and_empty_sources_require_ledger() {
        let permission = receivable::CUSTOMER_RECEIPT_DETAIL;
        let qualified = qualifications(&permission, false);
        let object = root(&permission, vec![child("s1", "actor"), child("s2", "other")]);
        assert!(!check(&permission, &qualified, &object, &scopes(), &BTreeSet::new()).unwrap());
        let share = data_condition(&permission, &qualified, &scopes(), Output::Share).unwrap();
        assert!(share.allows_object("actor", &object, &BTreeSet::new()).unwrap());

        let ledger = qualifications(&permission, true);
        assert!(!check(&permission, &ledger, &object, &scopes(), &BTreeSet::new()).unwrap());
        let zero = root(&permission, vec![]);
        assert!(check(&permission, &ledger, &zero, &scopes(), &BTreeSet::new()).unwrap());
        let split = QualifiedPermissions::test_grants(&[
            ("entry", std::slice::from_ref(&permission)),
            ("ledger", &[policy_permission::FINANCE_LEDGER_READ]),
        ]);
        assert!(!check(&permission, &split, &zero, &scopes(), &BTreeSet::new()).unwrap());

        let mut missing = zero;
        missing.relations.clear();
        assert!(matches!(
            check(&permission, &ledger, &missing, &scopes(), &BTreeSet::new()),
            Err(Error::ValidationError(_))
        ));
    }
    #[test]
    fn financial_scope_lookup_requires_the_source_and_its_entry() {
        let permission = receivable::CUSTOMER_RECEIPT_SUBMIT;
        let qualified = qualifications(&permission, false);
        let source = sales_order::DETAIL;
        let scope = super::super::capability_scope(&source).unwrap();
        let wrong_entry = [(receivable::ACCOUNT_DETAIL, source.clone(), scope.clone())];
        let wrong_source = [(permission.clone(), purchase_order::DETAIL, scope)];
        for scopes in [&wrong_entry, &wrong_source] {
            assert!(matches!(
                scoped_condition(&permission, &qualified, scopes, Output::Object),
                Err(Error::ValidationError(message)) if message == format!("缺少来源范围：{permission} -> {source}")
            ));
        }
    }

    #[test]
    fn receipt_submit_uses_the_same_final_condition_and_requires_both_handler_and_sources() {
        let permission = receivable::CUSTOMER_RECEIPT_SUBMIT;
        let qualified = qualifications(&permission, false);
        let source = sales_order::DETAIL;
        let scope = super::super::capability_scope(&source).unwrap();
        let runtime_scopes = [(permission.clone(), source.clone(), scope.clone())];
        let rule = scoped_condition(&permission, &qualified, &runtime_scopes, Output::Object).unwrap();
        assert_eq!(binding_condition(&permission, Output::Object).unwrap(), rule);
        assert_eq!(
            data_condition(&permission, &qualified, &[(source, scope)], Output::Object).unwrap(),
            rule
        );
        assert!(matches!(
            scoped_condition(&permission, &qualified, &runtime_scopes, Output::Share),
            Err(Error::ValidationError(_))
        ));

        let mut receipt = root(&permission, vec![]);
        let grants = BTreeSet::new();
        receipt.owner_id = FactValue::Known(Some("actor".into()));
        assert!(rule.allows_object("actor", &receipt, &grants).unwrap());
        receipt.owner_id = FactValue::Known(Some("other".into()));
        assert!(!rule.allows_object("actor", &receipt, &grants).unwrap());
        receipt.owner_id = FactValue::Missing;
        assert!(matches!(rule.allows_object("actor", &receipt, &grants), Err(Error::ValidationError(_))));

        let mut sale = child("s1", "other");
        sale.collaborators = FactValue::Known(BTreeSet::new());
        let mut receipt = root(&permission, vec![sale]);
        receipt.owner_id = FactValue::Known(Some("actor".into()));
        assert!(!rule.allows_object("actor", &receipt, &grants).unwrap());
        let grants = [(ResourceKind::SalesOrder, "s1".into())].into();
        assert!(rule.allows_object("actor", &receipt, &grants).unwrap());
        receipt.relations.clear();
        assert!(matches!(rule.allows_object("actor", &receipt, &grants), Err(Error::ValidationError(_))));
    }

    #[test]
    fn all_registered_source_actions_execute_and_commands_reject_share_output() {
        for resource in scope_spec::resources() {
            for permission in scope_spec::actions(resource) {
                if scope_spec::kind(permission).unwrap() != ActionKind::Source {
                    continue;
                }
                let qualified = qualifications(permission, true);
                let object = root(permission, vec![]);
                assert_eq!(
                    check(permission, &qualified, &object, &scopes(), &BTreeSet::new()).unwrap(),
                    needs_ledger(permission),
                    "{permission}"
                );
            }
        }

        let permission = purchase_payment_request::CREATE;
        assert!(matches!(
            data_condition(&permission, &qualifications(&permission, false), &scopes(), Output::Share),
            Err(Error::ValidationError(_))
        ));
    }
    #[test]
    fn refund_requires_independent_origin_qualification_complete_source_and_handler() {
        let permission = returns::CUSTOMER_REFUND_CUSTOMER_REFUND_SUBMIT;
        let qualified = QualifiedPermissions::test_grants(&[
            ("submit", std::slice::from_ref(&permission)),
            ("source", &[receivable::ACCOUNT_DETAIL]),
        ]);
        let mut object = root(&permission, vec![]);
        object.owner_id = FactValue::Known(Some("actor".into()));
        let origin = root(&receivable::ACCOUNT_DETAIL, vec![child("s1", "actor")]);
        object.relations.insert(
            RelationKey::parse("receivable_source").unwrap(),
            RelatedObjects { expected_ids: [origin.id.clone()].into(), objects: vec![origin] },
        );
        assert!(check(&permission, &qualified, &object, &scopes(), &BTreeSet::new()).unwrap());

        assert!(
            !check(&permission, &qualifications(&permission, false), &object, &scopes(), &BTreeSet::new())
                .unwrap()
        );

        object.owner_id = FactValue::Known(Some("other".into()));
        assert!(!check(&permission, &qualified, &object, &scopes(), &BTreeSet::new()).unwrap());
        object.owner_id = FactValue::Known(Some("actor".into()));
        object.relations.get_mut(&RelationKey::parse("receivable_source").unwrap()).unwrap().objects.clear();
        assert!(!check(&permission, &qualified, &object, &scopes(), &BTreeSet::new()).unwrap());
    }

    // 验证：冲正只能由真实经办人提交，并独立取得原资金根详情及同角色整账资格。
    #[test]
    fn reversal_requires_handler_and_independent_full_original_funds() {
        for (permission, origin_permission, relation) in [
            (
                returns::RECEIPT_REVERSAL_RECEIPT_REVERSAL_SUBMIT,
                receivable::CUSTOMER_RECEIPT_DETAIL,
                "receipt_source",
            ),
            (
                returns::PAYMENT_REVERSAL_PAYMENT_REVERSAL_SUBMIT,
                payable::SUPPLIER_PAYMENT_DETAIL,
                "payment_source",
            ),
        ] {
            assert_eq!(scope_spec::kind(&permission).unwrap(), ActionKind::Source);
            let qualified = QualifiedPermissions::test_grants(&[
                ("submit", std::slice::from_ref(&permission)),
                ("source", &[origin_permission.clone(), policy_permission::FINANCE_LEDGER_READ]),
            ]);
            let mut object = root(&permission, vec![]);
            object.owner_id = FactValue::Known(Some("actor".into()));
            let mut origin = root(&origin_permission, vec![child("s1", "actor")]);
            origin.id = "original_funds".into();
            let key = RelationKey::parse(relation).unwrap();
            object.relations.insert(
                key.clone(),
                RelatedObjects { expected_ids: [origin.id.clone()].into(), objects: vec![origin] },
            );
            assert!(check(&permission, &qualified, &object, &scopes(), &BTreeSet::new()).unwrap());

            let missing_source = QualifiedPermissions::test_grants(&[(
                "submit",
                &[permission.clone(), policy_permission::FINANCE_LEDGER_READ],
            )]);
            assert!(!check(&permission, &missing_source, &object, &scopes(), &BTreeSet::new()).unwrap());
            let split_ledger = QualifiedPermissions::test_grants(&[
                ("submit", std::slice::from_ref(&permission)),
                ("source", std::slice::from_ref(&origin_permission)),
                ("ledger", &[policy_permission::FINANCE_LEDGER_READ]),
            ]);
            assert!(!check(&permission, &split_ledger, &object, &scopes(), &BTreeSet::new()).unwrap());

            object.owner_id = FactValue::Known(Some("other".into()));
            assert!(!check(&permission, &qualified, &object, &scopes(), &BTreeSet::new()).unwrap());

            object.owner_id = FactValue::Known(Some("actor".into()));
            let source = &mut object.relations.get_mut(&key).unwrap().objects[0];
            let nested = source.relations.get_mut(&RelationKey::parse("sales_source").unwrap()).unwrap();
            nested.expected_ids.insert("missing_source".into());
            assert!(!check(&permission, &qualified, &object, &scopes(), &BTreeSet::new()).unwrap());
            object.relations.get_mut(&key).unwrap().objects.clear();
            assert!(!check(&permission, &qualified, &object, &scopes(), &BTreeSet::new()).unwrap());
        }
    }
}
