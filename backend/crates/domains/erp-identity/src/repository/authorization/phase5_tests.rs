//! 固定来源、整账及映射有效期合同的真实编译行为测试，不访问 MongoDB。
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_permissions::{policy_permission, purchase_order, receivable, sales_order, supplier_settlement};
use mongodb::bson::{Bson, Document, doc};
use persistence_core::{Executor, ReadExecutor};

use super::binding::FrozenMapping;
use super::fixtures::execute;
use super::testing::{Mapping, mapping};
use super::{
    AccessProof, AccessValidity, CompiledAccess, Replay, ResolvedAccess, ResolvedRules, ScopeMetadata,
    WholeRule, compile,
};
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{
    AccessContext, AccessFact, AccessRule, QueryModel, QueryOutput, RelationKey, RelationMatch,
};
use crate::ports::{AccessMapping, QueryFragment, RelationMapping};
use crate::service::authorization::fixed;
use crate::service::authorization::rules::sources::{Output, scoped_condition};
use crate::{Error, Permission, QualifiedPermissions, Result};

// 扩展通用测试映射，用于覆盖固定阶段、时间边界和根记录约束。
struct DecoratedMapping {
    // 提供基础模型、条件及来源的测试映射实例。
    inner: Arc<Mapping>,
    // 可在测试中变更的固定阶段，用于验证绑定后不可篡改。
    fixed: Mutex<Vec<Document>>,
    // 测试设置的静态有效期截止时点。
    deadline: Option<Instant>,
    // 启用时让根记录条件依赖当前入口权限。
    entry_sensitive: bool,
    // 启用时要求根记录的来源完整性标记为真。
    require_integrity: bool,
}
impl DecoratedMapping {
    // 以基础映射建立包装，默认不添加固定阶段、期限或附加记录限制。
    fn new(inner: Arc<Mapping>) -> Self {
        Self {
            inner,
            fixed: Mutex::new(vec![]),
            deadline: None,
            entry_sensitive: false,
            require_integrity: false,
        }
    }
}
impl AccessMapping for DecoratedMapping {
    // 保留基础映射的原模型描述，让包装只改变被测额外行为。
    fn model(&self) -> &QueryModel {
        self.inner.model()
    }
    // 返回测试给定的固定准备阶段，允许测试模拟绑定后的描述变化。
    fn stages(&self) -> Vec<Document> {
        self.fixed.lock().unwrap().clone()
    }
    // 将配置的静态截止时点提供给查询编译器。
    fn valid_until(&self, _: &AccessContext) -> Option<Instant> {
        self.deadline
    }
    // 保留基础映射登记的来源实例及关系。
    fn relations(&self) -> BTreeMap<RelationKey, Arc<dyn AccessMapping>> {
        self.inner.relations()
    }
    // 按原上下文委托基础映射生成主键表达式。
    fn key(&self, context: &AccessContext) -> Result<Bson> {
        self.inner.key(context)
    }
    // 按测试开关追加入口匹配或来源完整性检查，未启用时沿用基础记录条件。
    fn records(&self, context: &AccessContext) -> Result<QueryFragment> {
        if self.entry_sensitive {
            return Ok(QueryFragment::Match(doc! {"deleted_at":0,"entry":context.permission.to_string()}));
        }
        if self.require_integrity {
            return Ok(QueryFragment::Match(doc! {"deleted_at":0,"sources_valid":true}));
        }
        self.inner.records(context)
    }
    // 用基础映射转换事实，避免包装引入额外授权谓词。
    fn predicate(&self, fact: &AccessFact, context: &AccessContext) -> Result<QueryFragment> {
        self.inner.predicate(fact, context)
    }
    // 保留基础映射解析出的原始来源引用及实例。
    fn relation(&self, relation: &RelationKey, context: &AccessContext) -> Result<RelationMapping> {
        self.inner.relation(relation, context)
    }
}

// 为测试账号组装绑定原执行边界的证明，可附独立整账规则。
fn proof(
    permission: Permission,
    rule: AccessRule,
    whole_rule: Option<AccessRule>,
    model: &QueryModel,
    executor: &dyn Executor,
) -> AccessProof {
    AccessProof {
        context: AccessContext {
            actor: AuditActor::new("user".into(), "login".into(), AccountKind::Admin),
            permission,
            resource: model.resource.clone(),
            model: Some(model.key.clone()),
            as_of: Instant::now(),
        },
        resolved: ResolvedAccess {
            rules: ResolvedRules { root: rule, whole: whole_rule.map(WholeRule::Separate), management: None },
            replay: Replay::Target,
            version: "phase5".into(),
        },
        scope: ScopeMetadata { has_role_scope: false, organization_version: 1 },
        validity: AccessValidity {
            valid_until: None,
            boundary: executor.boundary().unwrap(),
            service: Arc::new(()),
        },
    }
}
// 冻结测试映射后调用真实编译器，返回预期成功的授权计划。
fn plan(
    permission: Permission,
    rule: AccessRule,
    whole: Option<AccessRule>,
    mapping: Arc<dyn AccessMapping>,
    executor: &mut dyn Executor,
) -> CompiledAccess {
    let model = mapping.model().clone();
    let proof = proof(permission, rule, whole, &model, executor);
    let frozen = FrozenMapping::freeze(mapping, &mut vec![], 0).unwrap();
    compile(proof, QualifiedPermissions::test_grants(&[]), &frozen, executor).unwrap()
}
// 提取样本投影中的记录 ID，便于断言查询成员。
fn ids(rows: Vec<Document>) -> Vec<String> {
    rows.into_iter().map(|row| row.get_str("id").unwrap().to_string()).collect()
}
// 构造指定负责人及 ID 的未删除业务来源记录。
fn row(id: &str, owner: &str) -> Document {
    doc! {"id":id,"owner":owner,"deleted_at":0}
}
// 构造销售来源任一命中的测试规则，用于受限根视图。
fn relation(rule: AccessRule) -> AccessRule {
    AccessRule::Related {
        relation: RelationKey::parse("sales_source").unwrap(),
        resource: "sales_order".into(),
        mode: RelationMatch::Any,
        rule: Box::new(rule),
    }
}
// 以销售、采购和结算来源的本人范围生成收款份额与整账规则，按开关模拟完整金额资格。
fn receipt_rules(ledger: bool) -> (AccessRule, AccessRule) {
    let permissions = if ledger {
        vec![receivable::CUSTOMER_RECEIPT_LIST, policy_permission::FINANCE_LEDGER_READ]
    } else {
        vec![receivable::CUSTOMER_RECEIPT_LIST]
    };
    let qualified = QualifiedPermissions::test_grants(&[("finance", &permissions)]);
    let scopes =
        [sales_order::DETAIL, purchase_order::DETAIL, supplier_settlement::STATEMENT_LIST].map(|source| {
            (
                receivable::CUSTOMER_RECEIPT_LIST,
                source,
                ResolvedScope {
                    role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
                    user_limit: None,
                },
            )
        });
    (
        scoped_condition(&receivable::CUSTOMER_RECEIPT_LIST, &qualified, &scopes, Output::Share).unwrap(),
        scoped_condition(&receivable::CUSTOMER_RECEIPT_LIST, &qualified, &scopes, Output::Object).unwrap(),
    )
}

// 验证收款份额只保留本人且未删除的来源，且不授予整账读取或跨阶段复用。
#[test]
fn source_portions_keep_only_owned_live_sources_and_never_imply_whole_ledger() {
    let mut executor = ReadExecutor::default();
    let mut root = mapping(&receivable::CUSTOMER_RECEIPT_LIST);
    Arc::get_mut(&mut root).unwrap().model.output = QueryOutput::Existence { fields: ["id".into()].into() };
    let (share, whole) = receipt_rules(false);
    let plan = plan(receivable::CUSTOMER_RECEIPT_LIST, share, Some(whole), root, &mut executor);

    let sales = vec![
        row("own", "user"),
        row("foreign", "other"),
        doc! {"id":"deleted","owner":"user","deleted_at":1},
    ];
    let sources = [("sales_order_rows".into(), sales.clone())].into();
    let roots = vec![
        doc! {"id":"receipt","deleted_at":0,"sales_source":["own","foreign"],"purchase_source":[],"settlement_source":[],"amount":900},
    ];

    assert_eq!(execute(roots.clone(), plan.stages(), &sources, &doc! {}), vec![doc! {"id":"receipt"}]);
    assert!(execute(roots, plan.whole_stages().unwrap(), &sources, &doc! {}).is_empty());

    let relation = RelationKey::parse("sales_source").unwrap();
    assert_eq!(ids(execute(sales, plan.source_stages(&relation).unwrap(), &sources, &doc! {})), ["own"]);
    assert_eq!(plan.source_model(&relation).unwrap().resource, "sales_order");

    executor.next_read();
    assert!(matches!(plan.ensure_current(&mut executor), Err(Error::ValidationError(_))));
}

// 验证整账要求所有真实引用均可读，同时保留确实没有分摊来源的记录。
#[test]
fn whole_ledger_requires_every_reference_and_keeps_real_zero_allocation_roots() {
    let mut executor = ReadExecutor::default();
    let mut root = mapping(&receivable::CUSTOMER_RECEIPT_LIST);
    Arc::get_mut(&mut root).unwrap().model.output = QueryOutput::Existence { fields: ["id".into()].into() };
    let (share, whole) = receipt_rules(true);
    let plan = plan(receivable::CUSTOMER_RECEIPT_LIST, share, Some(whole), root, &mut executor);

    let roots = [
        ("complete",vec!["own"]), ("foreign",vec!["own","foreign"]),
        ("missing",vec!["own","missing"]), ("zero",vec![]),
    ].into_iter().map(|(id,sales)|doc! {"id":id,"deleted_at":0,"sales_source":sales,"purchase_source":[],"settlement_source":[]}).collect::<Vec<_>>();
    let sources = [("sales_order_rows".into(), vec![row("own", "user"), row("foreign", "other")])].into();

    assert_eq!(
        ids(execute(roots.clone(), plan.whole_stages().unwrap(), &sources, &doc! {})),
        ["complete", "zero"]
    );
    assert_eq!(ids(execute(roots, plan.stages(), &sources, &doc! {})), ["complete", "foreign", "missing"]);
}

// 验证来源满足规则也不能绕过根记录的存在性及完整性要求。
#[test]
fn valid_source_conditions_do_not_bypass_root_record_integrity() {
    let mut executor = ReadExecutor::default();
    let mut inner = mapping(&receivable::CUSTOMER_RECEIPT_LIST);
    Arc::get_mut(&mut inner).unwrap().model.output = QueryOutput::Existence { fields: ["id".into()].into() };
    let mut root = DecoratedMapping::new(inner);
    root.require_integrity = true;
    let plan = plan(
        receivable::CUSTOMER_RECEIPT_LIST,
        relation(AccessRule::Fact(AccessFact::Owned)),
        Some(AccessRule::Deny),
        Arc::new(root),
        &mut executor,
    );

    let sales = vec![row("own", "user")];
    let sources = [("sales_order_rows".into(), sales.clone())].into();
    let roots = [false,true].map(|valid|doc! {"id":if valid {"valid"} else {"broken"},"deleted_at":0,"sources_valid":valid,"sales_source":["own"]});

    assert_eq!(ids(execute(roots.into(), plan.stages(), &sources, &doc! {})), ["valid"]);
    assert_eq!(
        ids(execute(
            sales,
            plan.source_stages(&RelationKey::parse("sales_source").unwrap()).unwrap(),
            &sources,
            &doc! {}
        )),
        ["own"]
    );
}

// 验证来源子计划沿用独立入口上下文。
#[test]
fn independent_source_plan_uses_its_entry_context() {
    let mut executor = ReadExecutor::default();
    let child = mapping(&sales_order::DETAIL);
    let decorated = Arc::new(DecoratedMapping { entry_sensitive: true, ..DecoratedMapping::new(child) });

    let mut root = mapping(&receivable::CUSTOMER_RECEIPT_LIST);
    let mutable = Arc::get_mut(&mut root).unwrap();
    mutable.model.output = QueryOutput::Existence { fields: ["id".into()].into() };
    mutable.relations.insert(RelationKey::parse("sales_source").unwrap(), decorated);
    let rule = AccessRule::Independent {
        permission: sales_order::DETAIL,
        rule: Box::new(relation(AccessRule::Fact(AccessFact::Owned))),
    };
    let plan = plan(receivable::CUSTOMER_RECEIPT_LIST, rule.clone(), Some(rule), root, &mut executor);

    let rows = vec![
        doc! {"id":"own","owner":"user","deleted_at":0,"entry":"sales_order:detail"},
        doc! {"id":"wrong","owner":"user","deleted_at":0,"entry":"customer_receipt:list"},
    ];
    let sources = [("sales_order_rows".into(), rows.clone())].into();
    let roots = vec![doc! {"id":"root","deleted_at":0,"sales_source":["own"]}];
    assert_eq!(ids(execute(roots.clone(), plan.stages(), &sources, &doc! {})), ["root"]);
    assert_eq!(ids(execute(roots, plan.whole_stages().unwrap(), &sources, &doc! {})), ["root"]);
    assert_eq!(
        ids(execute(
            rows,
            plan.source_stages(&RelationKey::parse("sales_source").unwrap()).unwrap(),
            &sources,
            &doc! {}
        )),
        ["own"]
    );
}

// 验证整账复用根过滤后仍保留相同摘要和最早截止时间。
#[tokio::test]
async fn whole_root_reuse_preserves_version_and_deadline() {
    let permission = receivable::CUSTOMER_RECEIPT_LIST;
    let root = mapping(&permission);
    let deadline = Instant::from_unix_secs(4_102_444_800);
    *root.expires.lock().unwrap() = Some(deadline);
    let frozen = FrozenMapping::freeze(root.clone(), &mut vec![], 0).unwrap();
    let mut executor = ReadExecutor::default();
    let rule = AccessRule::Fact(AccessFact::Owned);
    let original = proof(permission, rule.clone(), Some(rule), frozen.model(), &executor);
    let mut reused = original.clone();
    reused.resolved.rules.whole = Some(WholeRule::Root);
    let original = compile(original, QualifiedPermissions::test_grants(&[]), &frozen, &mut executor).unwrap();
    let reused = compile(reused, QualifiedPermissions::test_grants(&[]), &frozen, &mut executor).unwrap();

    assert_eq!(original.version(), reused.version());
    assert_eq!(original.whole_stages().unwrap(), reused.whole_stages().unwrap());
    assert!(std::ptr::eq(reused.stages(), reused.whole_stages().unwrap()));
    assert_eq!(reused.validity.valid_until, original.validity.valid_until);
    assert_eq!(reused.validity.valid_until, Some(deadline));
    AccessValidity::with_test_time(deadline, async {
        assert!(matches!(reused.ensure_current(&mut executor), Err(Error::ConflictError(_))));
    })
    .await;
}

// 验证映射声明的时限会令查询凭证到期。
#[tokio::test]
async fn static_deadline_expires_the_compiled_plan() {
    let deadline = Instant::from_unix_secs(4_102_444_800);
    let before = Instant::from_unix_secs(deadline.unix_secs() - 1);
    AccessValidity::with_test_time(before, async {
        let mut executor = ReadExecutor::default();
        let root = Arc::new(DecoratedMapping {
            deadline: Some(deadline),
            ..DecoratedMapping::new(mapping(&sales_order::DETAIL))
        });
        let actor = AuditActor::new("user".into(), "login".into(), AccountKind::Admin);
        let service = fixed::authorization(
            &[sales_order::DETAIL],
            ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                user_limit: None,
            },
            vec![],
            vec![(sales_order::DETAIL, root)],
        )
        .unwrap();

        let plan = service.query(&actor, &sales_order::DETAIL, &mut executor).await.unwrap();
        assert_eq!(plan.validity.valid_until, Some(deadline));
        assert!(plan.revalidation.is_some());
        service.validate(&plan, &mut executor).await.unwrap();

        let plan = service.query(&actor, &sales_order::DETAIL, &mut executor).await.unwrap();
        AccessValidity::set_test_time(deadline);
        assert!(matches!(
            service.validate(&plan, &mut executor).await,
            Err(Error::ConflictError(message)) if message == "DATA_SCOPE_CHANGED：授权关系已到期"
        ));
    })
    .await;
}

// 验证非事务凭证保留映射有效期且不能跨读取阶段复用。
#[tokio::test]
async fn non_snapshot_mapping_deadline_and_phase_changes_reject_delivery() {
    let mut executor = ReadExecutor::default();
    let root = mapping(&sales_order::DETAIL);
    let deadline = Instant::from_unix_secs(Instant::now().unix_secs() + 60);
    *root.expires.lock().unwrap() = Some(deadline);
    let plan = plan(sales_order::DETAIL, AccessRule::Fact(AccessFact::Owned), None, root, &mut executor);
    assert_eq!(plan.validity.valid_until, Some(deadline));
    AccessValidity::with_test_time(deadline, async {
        assert!(matches!(plan.ensure_current(&mut executor), Err(Error::ConflictError(_))));
    })
    .await;

    executor.next_read();
    assert!(matches!(plan.ensure_current(&mut executor), Err(Error::ValidationError(_))));
}

// 验证固定准备不允许过滤、展开或截断根记录，也不允许嵌套写阶段。
#[test]
fn fixed_mapping_stages_reject_root_filter_expansion_and_nested_writes() {
    for stage in [
        doc! {"$match":{"owner":"user"}},
        doc! {"$unwind":"$sources"},
        doc! {"$limit":1},
        doc! {"$lookup":{"from":"sources","let":{},"pipeline":[{"$out":"leak"}],"as":"sources"}},
        doc! {"$lookup":{"from":"sources","let":{},"pipeline":[{"$merge":"leak"}],"as":"sources"}},
    ] {
        let root = DecoratedMapping::new(mapping(&sales_order::DETAIL));
        root.fixed.lock().unwrap().push(stage);
        assert!(matches!(
            FrozenMapping::freeze(Arc::new(root), &mut vec![], 0),
            Err(Error::ValidationError(_))
        ));
    }
}

// 验证固定关联保留没有匹配来源的根记录，且绑定后不能更换阶段。
#[test]
fn fixed_lookup_preserves_missing_relations_and_is_immutable_after_binding() {
    let mut executor = ReadExecutor::default();
    let root = Arc::new(DecoratedMapping::new(mapping(&sales_order::DETAIL)));
    root.fixed.lock().unwrap().push(doc! {"$lookup":{"from":"optional","let":{"root":"$id"},"pipeline":[{"$match":{"$expr":{"$eq":["$root_id","$$root"]}}}],"as":"optional"}});
    let frozen = FrozenMapping::freeze(root.clone(), &mut vec![], 0).unwrap();
    let proof =
        proof(sales_order::DETAIL, AccessRule::Fact(AccessFact::Owned), None, root.model(), &executor);
    let plan =
        compile(proof.clone(), QualifiedPermissions::test_grants(&[]), &frozen, &mut executor).unwrap();
    assert_eq!(
        ids(execute(
            vec![row("own", "user"), row("other", "other")],
            plan.stages(),
            &BTreeMap::new(),
            &doc! {}
        )),
        ["own"]
    );

    root.fixed.lock().unwrap().clear();
    assert!(matches!(
        compile(proof, QualifiedPermissions::test_grants(&[]), &frozen, &mut executor),
        Err(Error::ValidationError(_))
    ));
}

// 验证即使 BSON 文档值相等，改变固定排序键顺序仍会被拒绝。
#[test]
fn frozen_lookup_rejects_sort_key_reordering() {
    let root = Arc::new(DecoratedMapping::new(mapping(&sales_order::DETAIL)));
    let original =
        doc! {"$lookup":{"from":"sources","pipeline":[{"$sort":{"priority":1,"id":1}}],"as":"sources"}};
    let reordered =
        doc! {"$lookup":{"from":"sources","pipeline":[{"$sort":{"id":1,"priority":1}}],"as":"sources"}};
    root.fixed.lock().unwrap().push(original.clone());
    let frozen = FrozenMapping::freeze(root.clone(), &mut vec![], 0).unwrap();
    let context = proof(
        sales_order::DETAIL,
        AccessRule::Fact(AccessFact::Owned),
        None,
        root.model(),
        &ReadExecutor::default(),
    )
    .context;

    assert_eq!(original, reordered);
    *root.fixed.lock().unwrap() = vec![reordered];
    assert!(matches!(frozen.records(&context), Err(Error::ValidationError(_))));
}

// 验证冻结包装按原实例复用，而非按模型描述复用。
#[test]
fn frozen_child_index_reuses_wrappers_only_for_the_same_instance() {
    let shared = mapping(&sales_order::DETAIL);
    let distinct = mapping(&sales_order::DETAIL);
    assert_eq!(shared.model(), distinct.model());
    let first = RelationKey::parse("first").unwrap();
    let second = RelationKey::parse("second").unwrap();
    let third = RelationKey::parse("third").unwrap();
    let mut root = mapping(&receivable::CUSTOMER_RECEIPT_LIST);
    Arc::get_mut(&mut root).unwrap().relations = [
        (first.clone(), shared.clone() as Arc<dyn AccessMapping>),
        (second.clone(), shared.clone() as Arc<dyn AccessMapping>),
        (third.clone(), distinct as Arc<dyn AccessMapping>),
    ]
    .into();
    let context = proof(
        receivable::CUSTOMER_RECEIPT_LIST,
        AccessRule::Allow,
        None,
        root.model(),
        &ReadExecutor::default(),
    )
    .context;
    let mut cache = Vec::new();
    let frozen = FrozenMapping::freeze(root, &mut cache, 0).unwrap();
    let standalone = FrozenMapping::freeze(shared, &mut cache, 0).unwrap();
    let (expected_ids, related) = frozen.relation(&first, &context).unwrap();
    assert_eq!(expected_ids, Bson::String("$first".into()));
    assert!(std::ptr::eq(related, standalone.as_ref()));
    assert!(std::ptr::eq(related, frozen.child(&second).unwrap()));
    assert!(!std::ptr::eq(related, frozen.child(&third).unwrap()));
}
