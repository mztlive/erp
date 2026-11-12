//! 固定来源规则及输出边界；对象执行和后续查询转换消费同一分区规则。
use std::collections::BTreeSet;

use erp_permissions::{
    payable, policy_permission, purchase_order, receivable, sales_order, supplier_settlement,
};

use super::{Decision, same_role_permissions};
use crate::entity::access_control::ResolvedScope;
use crate::entity::access_control::scope_spec::{self, ActionKind};
use crate::entity::authorization::{
    AccessContext, AccessFact, AccessRule, FactKind, ObjectFacts, RelationKey, RelationMatch,
};
use crate::entity::resource_access::ResourceKind;
use crate::{Error, Permission, QualifiedPermissions, Result};

/// 完整对象和匹配份额的判断不可互换；存在性也只能消费匹配份额结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Output {
    Object,
    Share,
}

/// 一次来源判断使用已经取得的同拍事实和范围，不接受业务提供的允许结果。
pub(crate) struct Input<'a> {
    pub context: &'a AccessContext,
    pub object: &'a ObjectFacts,
    pub scopes: &'a [(Permission, ResolvedScope)],
    pub grants: &'a BTreeSet<(ResourceKind, String)>,
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

/// 查询转换必须同时应用分区条件和数据条件，不能只编译 rule。
pub(super) struct Partition {
    present: Vec<Source>,
    absent: Vec<Source>,
    pub output: Output,
    pub rule: AccessRule,
    nested: Option<(Source, Vec<Partition>)>,
}

impl Source {
    /// 来源键由固定规则选取，业务只能提供该键对应的真实关系。
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

    /// 独立资金源用详情资格，订单和结算条款沿当前入口资格解析。
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

/// 每个已登记来源入口必须落到明确的来源集合，不以分类代替实际政策。
fn sources(permission: &Permission) -> Result<Vec<Source>> {
    let sources = match permission.resource() {
        "cost_entry" if permission.action() == "create" => vec![Source::Sales],
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
        _ => return Err(Error::ValidationError("此权限没有固定来源规则".into())),
    };
    Ok(sources)
}

/// 仅资金份额入口允许部分来源输出；命令和付款申请始终要求完整来源。
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

/// 份额只来自该财务入口原有的来源方向，整账分区仍覆盖全部真实来源。
fn share_source(permission: &Permission, source: Source) -> bool {
    match permission.resource() {
        "customer_receipt" | "cost_entry" | "cost_allocation" => source == Source::Sales,
        "supplier_payment" => matches!(source, Source::Purchase | Source::Settlement),
        _ => true,
    }
}

/// 完整资金金额要求入口与整账资格由同一角色提供，包括真实零分配记录。
fn needs_ledger(permission: &Permission) -> bool {
    matches!(
        permission.resource(),
        "cost_entry" | "cost_allocation" | "customer_receipt" | "supplier_payment" | "invoice"
    ) && matches!(permission.action(), "list" | "detail")
}

/// 生成固定分区；查询必须保持完整原始来源集合，不能按已获准来源选分区。
pub(super) fn conditions(
    permission: &Permission,
    qualified: &QualifiedPermissions,
    scopes: &[(Permission, ResolvedScope)],
    output: Output,
) -> Result<Vec<Partition>> {
    if scope_spec::kind(permission)? != ActionKind::Source {
        return Err(Error::ValidationError("此入口不使用来源分区规则".into()));
    }
    if !qualified.same_role(&same_role_permissions(permission)) {
        return Err(Error::Forbidden("没有启用角色提供来源入口的完整权限".into()));
    }
    if output == Output::Share && !allows_shares(permission) {
        return Err(Error::ValidationError("此权限不允许份额输出".into()));
    }
    let sources = sources(permission)?;
    if matches!(permission.resource(), "customer_refund" | "supplier_refund") {
        return refund_conditions(&sources, qualified, scopes);
    }
    let ledger = output == Output::Object && needs_ledger(permission);
    let ledger_allowed = qualified.same_role(&[permission.clone(), policy_permission::FINANCE_LEDGER_READ]);
    let mut partitions = Vec::new();
    for mask in 0..(1usize << sources.len()) {
        if mask == 0 && !ledger {
            continue;
        }
        if permission.resource() == "payable_account" && mask.count_ones() != 1 {
            continue;
        }
        let (present, absent): (Vec<_>, Vec<_>) =
            sources.iter().enumerate().partition(|(index, _)| mask & (1 << index) != 0);
        let present = present.into_iter().map(|(_, source)| *source).collect::<Vec<_>>();
        let absent = absent.into_iter().map(|(_, source)| *source).collect::<Vec<_>>();
        let rules = present
            .iter()
            .filter(|source| output == Output::Object || share_source(permission, **source))
            .map(|source| source_rule(*source, scopes, output))
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
        partitions.push(Partition { present, absent, output, rule, nested: None });
    }
    Ok(partitions)
}

/// 来源数据条件保留普通范围与显式读取授权的并集，不追加来源菜单权限。
fn source_rule(source: Source, scopes: &[(Permission, ResolvedScope)], output: Output) -> Result<AccessRule> {
    let permission = source.permission();
    let scope = scopes
        .iter()
        .find(|(key, _)| key == &permission)
        .map(|(_, scope)| scope)
        .ok_or_else(|| Error::ValidationError(format!("缺少来源范围：{permission}")))?;
    let mut rule = AccessRule::scope(scope, false);
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

/// 完整来源对象必须用完整对象输出规则，份额判断不能作为 require 的成功结果。
pub(super) fn evaluate(
    permission: &Permission,
    qualified: &QualifiedPermissions,
    input: &Input<'_>,
) -> Result<Decision> {
    if &input.context.permission != permission || input.object.resource != permission.resource() {
        return Err(Error::ValidationError("来源入口与对象错配".into()));
    }
    let partitions = conditions(permission, qualified, input.scopes, Output::Object)?;
    let mut matched = false;
    for partition in partitions {
        if partition.matches(input.object)? {
            matched = partition.allows(input.context, input.object, input.grants)?;
        }
    }
    Ok(if matched { Decision::Allow } else { Decision::Deny })
}

impl Partition {
    /// 已知空关系才进入空分区；缺失关系、损坏引用不能伪装为无来源。
    pub(super) fn matches(&self, object: &ObjectFacts) -> Result<bool> {
        let mut matches = true;
        for source in self.present.iter().chain(&self.absent) {
            let relation = object
                .relations
                .get(&RelationKey::parse(source.relation())?)
                .ok_or_else(|| Error::ValidationError("来源分区事实未读取".into()))?;
            if relation.expected_ids.is_empty() && !relation.objects.is_empty() {
                return Err(Error::ValidationError("来源分区身份错配".into()));
            }
            matches &= self.present.contains(source) != relation.expected_ids.is_empty();
        }
        Ok(matches)
    }
}

/// 退款恰有一个真实资金根；详情资格分别取证，原经办职责仍为必要条件。
fn refund_conditions(
    sources: &[Source],
    qualified: &QualifiedPermissions,
    scopes: &[(Permission, ResolvedScope)],
) -> Result<Vec<Partition>> {
    let mut partitions = Vec::new();
    for source in sources {
        let permission = source.permission();
        let allowed = qualified.same_role(&same_role_permissions(&permission));
        let nested =
            if allowed { conditions(&permission, qualified, scopes, Output::Object)? } else { vec![] };
        partitions.push(Partition {
            present: vec![*source],
            absent: sources.iter().copied().filter(|item| item != source).collect(),
            output: Output::Object,
            rule: AccessRule::Fact(AccessFact::Owned),
            nested: Some((*source, nested)),
        });
    }
    Ok(partitions)
}

impl Partition {
    /// 使用相同分区规则递归核验退款资金根，完整性不能由可见来源子集替代。
    fn allows(
        &self,
        context: &AccessContext,
        object: &ObjectFacts,
        grants: &BTreeSet<(ResourceKind, String)>,
    ) -> Result<bool> {
        let mut allowed = self.rule.allows_object(context, object, grants)?;
        if let Some((source, partitions)) = &self.nested {
            let relation = object
                .relations
                .get(&RelationKey::parse(source.relation())?)
                .ok_or_else(|| Error::ValidationError("资金来源未读取".into()))?;
            let mut seen = BTreeSet::new();
            for child in &relation.objects {
                if child.resource != source.permission().resource()
                    || !relation.expected_ids.contains(&child.id)
                    || !seen.insert(child.id.clone())
                {
                    return Err(Error::ValidationError("资金来源身份错配或重复".into()));
                }
                let child_context = AccessContext {
                    permission: source.permission(),
                    resource: child.resource.clone(),
                    ..context.clone()
                };
                let mut child_allowed = false;
                for partition in partitions {
                    if partition.matches(child)? {
                        child_allowed |= partition.allows(&child_context, child, grants)?;
                    }
                }
                allowed &= child_allowed;
            }
            allowed &= !seen.is_empty() && seen == relation.expected_ids;
        }
        Ok(allowed)
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_permissions::{cost, purchase_payment_request};

    use super::*;
    use crate::entity::access_control::ScopeClause;
    use crate::entity::access_control::scope_spec::{self, ActionKind};
    use crate::entity::authorization::{FactValue, RelatedObjects};

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
    fn context(permission: &Permission) -> AccessContext {
        AccessContext {
            actor: AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin),
            permission: permission.clone(),
            resource: permission.resource().into(),
            model: None,
            as_of: Instant::now(),
        }
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
    ) -> Result<Decision> {
        evaluate(permission, qualified, &Input { context: &context(permission), object, scopes, grants })
    }
    #[test]
    fn cost_source_uses_scope_or_grant_and_requires_complete_facts_and_sources() {
        let permission = cost::ENTRY_LIST;
        let qualified = qualifications(&permission, true);
        let scopes = scopes();
        let grants = BTreeSet::new();
        let mut object = root(&permission, vec![child("s1", "actor")]);
        assert_eq!(check(&permission, &qualified, &object, &scopes, &grants).unwrap(), Decision::Allow);
        object = root(&permission, vec![child("s1", "other")]);
        assert_eq!(check(&permission, &qualified, &object, &scopes, &grants).unwrap(), Decision::Deny);
        let grants = BTreeSet::from([(ResourceKind::SalesOrder, "s1".into())]);
        assert_eq!(check(&permission, &qualified, &object, &scopes, &grants).unwrap(), Decision::Allow);
        object.relations.get_mut(&RelationKey::parse("sales_source").unwrap()).unwrap().objects[0].owner_id =
            FactValue::Missing;
        assert!(matches!(
            check(&permission, &qualified, &object, &scopes, &grants),
            Err(Error::ValidationError(_))
        ));
        object = root(&permission, vec![child("s1", "actor"), child("s2", "other")]);
        assert_eq!(check(&permission, &qualified, &object, &scopes, &grants).unwrap(), Decision::Deny);
        object.relations.get_mut(&RelationKey::parse("sales_source").unwrap()).unwrap().objects.pop();
        assert_eq!(check(&permission, &qualified, &object, &scopes, &grants).unwrap(), Decision::Deny);
        assert_eq!(
            check(&permission, &qualified, &root(&permission, vec![]), &scopes, &grants).unwrap(),
            Decision::Allow
        );
        let split = QualifiedPermissions::test_grants(&[
            ("cost", std::slice::from_ref(&permission)),
            ("sales", &[sales_order::LIST]),
        ]);
        assert!(matches!(check(&permission, &split, &object, &scopes, &grants), Err(Error::Forbidden(_))));
    }
    #[test]
    fn cost_shares_use_normal_scope_without_ledger_and_creation_uses_fixed_detail_qualification() {
        let permission = erp_permissions::cost::ENTRY_LIST;
        let object = root(&permission, vec![child("s1", "actor")]);
        let qualified = qualifications(&permission, false);
        let partitions = conditions(&permission, &qualified, &scopes(), Output::Share).unwrap();
        assert!(partitions.iter().any(|partition| partition.matches(&object).unwrap()
            && partition.allows(&context(&permission), &object, &BTreeSet::new()).unwrap()));
        assert_eq!(
            check(&permission, &qualified, &object, &scopes(), &BTreeSet::new()).unwrap(),
            Decision::Deny
        );
        let create = erp_permissions::cost::ENTRY_CREATE;
        let object = root(&create, vec![child("s1", "actor")]);
        assert_eq!(
            check(&create, &qualifications(&create, false), &object, &scopes(), &BTreeSet::new()).unwrap(),
            Decision::Allow
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
            assert_eq!(
                check(&permission, &qualified, &object, &scopes(), &BTreeSet::new()).unwrap(),
                Decision::Allow
            );
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
        assert_eq!(
            check(&permission, &qualified, &object, &scopes(), &BTreeSet::new()).unwrap(),
            Decision::Deny
        );
        let shares = conditions(&permission, &qualified, &scopes(), Output::Share).unwrap();
        assert!(shares.iter().any(|partition| partition.matches(&object).unwrap()
            && partition.allows(&context(&permission), &object, &BTreeSet::new()).unwrap()));
        let ledger = qualifications(&permission, true);
        assert_eq!(
            check(&permission, &ledger, &object, &scopes(), &BTreeSet::new()).unwrap(),
            Decision::Deny
        );
        let zero = root(&permission, vec![]);
        assert_eq!(check(&permission, &ledger, &zero, &scopes(), &BTreeSet::new()).unwrap(), Decision::Allow);
        let split = QualifiedPermissions::test_grants(&[
            ("entry", std::slice::from_ref(&permission)),
            ("ledger", &[policy_permission::FINANCE_LEDGER_READ]),
        ]);
        assert_eq!(check(&permission, &split, &zero, &scopes(), &BTreeSet::new()).unwrap(), Decision::Deny);
        let mut missing = zero;
        missing.relations.clear();
        assert!(matches!(
            check(&permission, &ledger, &missing, &scopes(), &BTreeSet::new()),
            Err(Error::ValidationError(_))
        ));
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
                    if needs_ledger(permission) { Decision::Allow } else { Decision::Deny },
                    "{permission}"
                );
            }
        }
        let permission = purchase_payment_request::CREATE;
        assert!(matches!(
            conditions(&permission, &qualifications(&permission, false), &scopes(), Output::Share),
            Err(Error::ValidationError(_))
        ));
    }
    #[test]
    fn refund_requires_independent_origin_qualification_complete_source_and_handler() {
        let permission = erp_permissions::returns::CUSTOMER_REFUND_CUSTOMER_REFUND_SUBMIT;
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
        assert_eq!(
            check(&permission, &qualified, &object, &scopes(), &BTreeSet::new()).unwrap(),
            Decision::Allow
        );
        assert_eq!(
            check(&permission, &qualifications(&permission, false), &object, &scopes(), &BTreeSet::new())
                .unwrap(),
            Decision::Deny
        );
        object.owner_id = FactValue::Known(Some("other".into()));
        assert_eq!(
            check(&permission, &qualified, &object, &scopes(), &BTreeSet::new()).unwrap(),
            Decision::Deny
        );
        object.owner_id = FactValue::Known(Some("actor".into()));
        object.relations.get_mut(&RelationKey::parse("receivable_source").unwrap()).unwrap().objects.clear();
        assert_eq!(
            check(&permission, &qualified, &object, &scopes(), &BTreeSet::new()).unwrap(),
            Decision::Deny
        );
    }
}
