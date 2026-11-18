//! 固定来源规则及输出边界；对象执行和后续查询转换消费同一分区规则。
use std::collections::BTreeSet;

use erp_permissions::{
    payable, policy_permission, purchase_order, receivable, sales_order, supplier_settlement,
};

use super::same_role_permissions;
use crate::entity::access_control::ResolvedScope;
use crate::entity::access_control::scope_spec::{self, ActionKind};
#[cfg(test)]
use crate::entity::authorization::{AccessContext, ObjectFacts};
use crate::entity::authorization::{AccessFact, AccessRule, FactKind, RelationKey, RelationMatch};
use crate::entity::resource_access::ResourceKind;
use crate::{Error, Permission, QualifiedPermissions, Result};

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

/// 实际范围依赖及其资格上下文；退款资金根使用自己的独立详情资格。
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

/// 根及独立来源资格一次取证，整账资格只作为固定可选分支。
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

/// 构造时检查所有可能执行的来源分支，不受当前账号和范围配置影响。
pub(super) fn binding_condition(permission: &Permission, output: Output) -> Result<AccessRule> {
    let partitions =
        conditions_with(permission, &|_| true, &|_, source| super::capability_scope(source), output)?;
    let mut rule = AccessRule::any(partitions.iter().map(Partition::condition).collect::<Result<_>>()?);
    if permission == &receivable::CUSTOMER_RECEIPT_SUBMIT {
        rule = AccessRule::all(vec![rule, AccessRule::Fact(AccessFact::Owned)])
            .requiring(&BTreeSet::from([FactKind::Owner]));
    }
    Ok(rule)
}

/// 查询转换必须同时应用分区条件和数据条件，不能只编译 rule。
pub(super) struct Partition {
    present: Vec<Source>,
    absent: Vec<Source>,
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

/// 本次入口具备读取完整金额所需的同角色功能资格；不证明来源记录可见。
pub(crate) fn whole_qualified(permission: &Permission, qualified: &QualifiedPermissions) -> bool {
    qualified.same_role(&same_role_permissions(permission))
        && (!needs_ledger(permission)
            || qualified.same_role(&[permission.clone(), policy_permission::FINANCE_LEDGER_READ]))
}

/// 生成固定分区；查询必须保持完整原始来源集合，不能按已获准来源选分区。
#[cfg(test)]
pub(super) fn conditions(
    permission: &Permission,
    qualified: &QualifiedPermissions,
    scopes: &[(Permission, ResolvedScope)],
    output: Output,
) -> Result<Vec<Partition>> {
    conditions_with(
        permission,
        &|required| qualified.same_role(required),
        &|_, permission| {
            scopes
                .iter()
                .find(|(key, _)| key == permission)
                .map(|(_, scope)| scope.clone())
                .ok_or_else(|| Error::ValidationError(format!("缺少来源范围：{permission}")))
        },
        output,
    )
}

fn conditions_with(
    permission: &Permission,
    qualifies: &dyn Fn(&[Permission]) -> bool,
    scopes: &dyn Fn(&Permission, &Permission) -> Result<ResolvedScope>,
    output: Output,
) -> Result<Vec<Partition>> {
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

        let (present, absent): (Vec<_>, Vec<_>) =
            sources.iter().enumerate().partition(|(index, _)| mask & (1 << index) != 0);
        let present = present.into_iter().map(|(_, source)| *source).collect::<Vec<_>>();
        let absent = absent.into_iter().map(|(_, source)| *source).collect::<Vec<_>>();

        let rules = present
            .iter()
            .filter(|source| output == Output::Object || share_source(permission, **source))
            .map(|source| source_rule(permission, *source, scopes, output))
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
        partitions.push(Partition { present, absent, rule, nested: None });
    }
    Ok(partitions)
}

/// 来源数据条件保留普通范围与显式读取授权的并集，不追加来源菜单权限。
fn source_rule(
    entry: &Permission,
    source: Source,
    scopes: &dyn Fn(&Permission, &Permission) -> Result<ResolvedScope>,
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

/// 每个来源范围保留授予它的入口角色；不同退款资金根不得共用范围结果。
pub(crate) fn scoped_condition(
    permission: &Permission,
    qualified: &QualifiedPermissions,
    scopes: &[(Permission, Permission, ResolvedScope)],
    output: Output,
) -> Result<AccessRule> {
    let lookup = |entry: &Permission, source: &Permission| {
        scopes
            .iter()
            .find(|(key, origin, _)| key == entry && origin == source)
            .map(|(_, _, scope)| scope.clone())
            .ok_or_else(|| Error::ValidationError(format!("缺少来源范围：{entry} -> {source}")))
    };
    let partitions = conditions_with(permission, &|required| qualified.same_role(required), &lookup, output)?;
    let mut rule = AccessRule::any(partitions.iter().map(Partition::condition).collect::<Result<_>>()?);
    if permission == &receivable::CUSTOMER_RECEIPT_SUBMIT {
        rule = AccessRule::all(vec![rule, AccessRule::Fact(AccessFact::Owned)])
            .requiring(&BTreeSet::from([FactKind::Owner]));
    }
    Ok(rule)
}

/// 审批管理已取得完整同角色资格；来源范围由该组资格解析，不追加来源菜单资格。
pub(crate) fn management_condition(
    permission: &Permission,
    scopes: &[(Permission, ResolvedScope)],
) -> Result<AccessRule> {
    let lookup = |_: &Permission, source: &Permission| {
        scopes
            .iter()
            .find(|(key, _)| key == source)
            .map(|(_, scope)| scope.clone())
            .ok_or_else(|| Error::ValidationError("缺少审批管理来源范围".into()))
    };
    let partitions = conditions_with(permission, &|_| true, &lookup, Output::Object)?;
    Ok(AccessRule::any(partitions.iter().map(Partition::condition).collect::<Result<_>>()?))
}

/// 来源分区和来源数据检查合并为同一表达式，供对象与查询共同执行。
#[cfg(test)]
pub(crate) fn data_condition(
    permission: &Permission,
    qualified: &QualifiedPermissions,
    scopes: &[(Permission, ResolvedScope)],
    output: Output,
) -> Result<AccessRule> {
    let partitions = conditions(permission, qualified, scopes, output)?;
    let mut rule = AccessRule::any(partitions.iter().map(Partition::condition).collect::<Result<_>>()?);
    if permission == &receivable::CUSTOMER_RECEIPT_SUBMIT {
        rule = AccessRule::all(vec![rule, AccessRule::Fact(AccessFact::Owned)])
            .requiring(&BTreeSet::from([FactKind::Owner]));
    }
    Ok(rule)
}

impl Partition {
    /// 每个分区保留完整关系的有无条件；不能提前按通过来源过滤根记录。
    fn condition(&self) -> Result<AccessRule> {
        let mut rules = vec![self.rule.clone()];
        for source in self.present.iter().chain(&self.absent) {
            rules.push(AccessRule::RelationPresence {
                relation: RelationKey::parse(source.relation())?,
                present: self.present.contains(source),
            });
        }

        if let Some((source, partitions)) = &self.nested {
            rules.push(AccessRule::Related {
                relation: RelationKey::parse(source.relation())?,
                resource: source.permission().resource().into(),
                mode: RelationMatch::ExactlyOne,
                rule: Box::new(AccessRule::Independent {
                    permission: source.permission(),
                    rule: Box::new(AccessRule::any(
                        partitions.iter().map(Self::condition).collect::<Result<_>>()?,
                    )),
                }),
            });
        }
        Ok(AccessRule::all(rules))
    }

    /// 已知空关系才进入空分区；缺失关系、损坏引用不能伪装为无来源。
    #[cfg(test)]
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
    qualifies: &dyn Fn(&[Permission]) -> bool,
    scopes: &dyn Fn(&Permission, &Permission) -> Result<ResolvedScope>,
) -> Result<Vec<Partition>> {
    let mut partitions = Vec::new();
    for source in sources {
        let permission = source.permission();
        let allowed = qualifies(&same_role_permissions(&permission));
        let nested =
            if allowed { conditions_with(&permission, qualifies, scopes, Output::Object)? } else { vec![] };
        partitions.push(Partition {
            present: vec![*source],
            absent: sources.iter().copied().filter(|item| item != source).collect(),
            rule: AccessRule::Fact(AccessFact::Owned),
            nested: Some((*source, nested)),
        });
    }
    Ok(partitions)
}

impl Partition {
    /// 使用相同分区规则递归核验退款资金根，完整性不能由可见来源子集替代。
    #[cfg(test)]
    fn allows(
        &self,
        context: &AccessContext,
        object: &ObjectFacts,
        grants: &BTreeSet<(ResourceKind, String)>,
    ) -> Result<bool> {
        self.condition()?.allows_object(context.actor.id(), object, grants)
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_permissions::{cost, purchase_payment_request, returns};

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
        data_condition(permission, qualified, scopes, Output::Object)?.allows_object(
            context(permission).actor.id(),
            object,
            grants,
        )
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
        let partitions = conditions(&permission, &qualified, &scopes(), Output::Share).unwrap();
        assert!(partitions.iter().any(|partition| partition.matches(&object).unwrap()
            && partition.allows(&context(&permission), &object, &BTreeSet::new()).unwrap()));
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
        let shares = conditions(&permission, &qualified, &scopes(), Output::Share).unwrap();
        assert!(shares.iter().any(|partition| partition.matches(&object).unwrap()
            && partition.allows(&context(&permission), &object, &BTreeSet::new()).unwrap()));

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
            conditions(&permission, &qualifications(&permission, false), &scopes(), Output::Share),
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

    /// 冲正只能由真实经办人提交，并独立取得原资金根详情及同角色整账资格。
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
