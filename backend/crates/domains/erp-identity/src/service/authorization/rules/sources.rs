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

/// 列出真实来源的范围和取证入口；退款或冲销追溯资金根时，按该根自己的详情入口分别请求范围。
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

/// 同次身份取证覆盖根入口、独立资金来源和可选整账资格，供实际分区选择。
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

/// 启动时按输出粒度展开固定来源分区及事实能力，不因某账号缺权或未配置人员范围而省略。
pub(super) fn binding_condition(permission: &Permission, output: Output) -> Result<AccessRule> {
    condition_with(
        permission,
        &|_| true,
        &|entry, source| required_scope(&ScopeLookup::Capability, entry, source),
        output,
    )
}

impl Source {
    // 返回来源类型的固定关联键，由业务事实和 Mapping 提供真实关系，调用方不能改选。
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

    // 返回该来源的固定范围动作；资金根用自身详情入口，订单和结算范围由当前入口解释。
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

// 按实际权限列出所有允许的业务来源类型，不交由调用方决定适用规则。
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

// 只让固定财务列表和详情输出受限份额；命令及付款申请不能按部分来源放行。
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

// 按财务读模型的登记方向选择可输出的来源份额；整账判断仍覆盖全部真实来源。
fn share_source(permission: &Permission, source: Source) -> bool {
    match permission.resource() {
        "customer_receipt" | "cost_entry" | "cost_allocation" => source == Source::Sales,
        "supplier_payment" => matches!(source, Source::Purchase | Source::Settlement),
        _ => true,
    }
}

// 判断完整金额读取是否需要入口与整账权限同角色，包括没有实际分配的记录。
fn needs_ledger(permission: &Permission) -> bool {
    matches!(
        permission.resource(),
        "cost_entry" | "cost_allocation" | "customer_receipt" | "supplier_payment" | "invoice"
    ) && matches!(permission.action(), "list" | "detail")
}

/// 检查整账所需的同角色功能资格；仍须另用范围和完整来源条件证明来源可读。
pub(crate) fn whole_qualified(permission: &Permission, qualified: &QualifiedPermissions) -> bool {
    qualified.same_role(&same_role_permissions(permission))
        && (!needs_ledger(permission)
            || qualified.same_role(&[permission.clone(), policy_permission::FINANCE_LEDGER_READ]))
}

// 生成最终来源条件；收款提交还须与经办人条件取交集，并要求负责人事实完整。
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

// 按原始真实引用是否存在划分分区，而非按已加载或已获准来源。整账须覆盖实际来源，份额只匹配允许输出的可见来源。
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

// 读取掩码中指定来源的存在位，供枚举精确的真实引用分区。
fn source_present(mask: usize, index: usize) -> bool {
    mask & (1 << index) != 0
}

// 把来源规则与各实际引用的有无条件合并，按先存在再缺席的顺序构造稳定分区。
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

// 构造订单或结算来源范围，订单保留有效记录读取补充，不另查来源菜单。完整对象要求全量非空，份额只需任一关系命中。
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

/// 按取证入口和范围来源双键生成运行时条件；退款涉及的独立资金根不复用其他入口范围。
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

// 按入口和来源取得必需范围，缺失时报出实际配对，不能降级为无过滤。
fn required_scope<'a>(
    scopes: &ScopeLookup<'a>,
    entry: &Permission,
    source: &Permission,
) -> Result<Cow<'a, ResolvedScope>> {
    scopes.get(entry, source, || Error::ValidationError(format!("缺少来源范围：{entry} -> {source}")))
}

/// 已证明完整同角色审批管理资格后，按这组合格角色解析出的来源范围生成完整对象条件，不再要求来源菜单。
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

/// 供局部测试用按来源索引的范围生成生产规则，仍执行同角色资格和最终来源组合。
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

// 退款或冲销须匹配唯一真实资金根、独立的原资金完整详情及当前经办责任，任一条件不能单独放行。
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

    // 为测试入口生成单角色资格样本，可选加上整账权限以区分输出。
    fn qualifications(permission: &Permission, ledger: bool) -> QualifiedPermissions {
        let mut required = same_role_permissions(permission);
        if ledger {
            required.push(policy_permission::FINANCE_LEDGER_READ);
        }
        QualifiedPermissions::test_grants(&[("role", &required)])
    }
    // 构造销售本人范围及采购、供应商结算公司范围，供分区测试共用。
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
    // 构造带指定 ID 和负责人的真实销售单来源事实。
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
    // 构造测试财务根及全部已知关系，默认无引用，并按样本填入销售来源及对应 ID 集合。
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
    // 以固定测试账号应用完整对象条件，传入实际规则需要的范围和补权集合。
    fn check(
        permission: &Permission,
        qualified: &QualifiedPermissions,
        object: &ObjectFacts,
        scopes: &[(Permission, ResolvedScope)],
        grants: &BTreeSet<(ResourceKind, String)>,
    ) -> Result<bool> {
        data_condition(permission, qualified, scopes, Output::Object)?.allows_object("actor", object, grants)
    }
    // 验证成本来源可由范围或记录授权放行，但不能省略必需事实和真实完整来源，功能资格仍须同角色。
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
    // 验证成本份额不需要整账资格但仍受来源范围限制，创建须满足固定的销售详情同角色资格。
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

    // 验证有实际采购或结算引用时按对应完整分区判断，不能把未加载的来源视为不存在。
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

    // 验证可见份额不放行整账；无分配记录也须满足同角色整账资格及完整引用事实。
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
    // 验证财务来源范围必须同时匹配取证入口和来源权限，匹配任一键不足以放行。
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

    // 验证收款提交的启动与运行时条件一致，并同时要求经办责任、完整实际来源及必需事实。
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

    // 遍历登记的来源动作验证规则可执行，并确认付款申请命令拒绝份额输出。
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
    // 验证客户退款须独立取得原应收详情资格，且真实资金来源完整、当前账号是经办人。
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

    // 验证冲正须真实经办人操作，且独立取得原资金详情及同角色整账资格，并覆盖嵌套来源。
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
