//! 固定来源、整账及映射准备合同的真实编译行为测试，不访问 MongoDB。
use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
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
use super::{AccessProof, CompiledAccess, compile};
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{
    AccessContext, AccessFact, AccessRule, MappingFacts, QueryModel, QueryOutput, RelationKey, RelationMatch,
};
use crate::ports::{AccessMapping, MappingPreparation, QueryFragment, RelationMapping};
use crate::service::authorization::rules::sources::{Output, scoped_condition};
use crate::{Error, Permission, QualifiedPermissions, Result};

struct DecoratedMapping {
    inner: Arc<Mapping>,
    fixed: Mutex<Vec<Document>>,
    deadline: Option<Instant>,
    entry_sensitive: bool,
    require_integrity: bool,
}
impl DecoratedMapping {
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
    fn model(&self) -> &QueryModel {
        self.inner.model()
    }
    fn stages(&self) -> Vec<Document> {
        self.fixed.lock().unwrap().clone()
    }
    fn valid_until(&self, _: &AccessContext) -> Option<Instant> {
        self.deadline
    }
    fn relations(&self) -> BTreeMap<RelationKey, Arc<dyn AccessMapping>> {
        self.inner.relations()
    }
    fn preparation(&self) -> Option<&dyn MappingPreparation> {
        self.inner.preparation()
    }
    fn key(&self, context: &AccessContext, facts: Option<&MappingFacts>) -> Result<Bson> {
        self.inner.key(context, facts)
    }
    fn records(&self, context: &AccessContext, facts: Option<&MappingFacts>) -> Result<QueryFragment> {
        if self.entry_sensitive {
            return Ok(QueryFragment::Match(doc! {"deleted_at":0,"entry":context.permission.to_string()}));
        }
        if self.require_integrity {
            return Ok(QueryFragment::Match(doc! {"deleted_at":0,"sources_valid":true}));
        }
        self.inner.records(context, facts)
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        facts: Option<&MappingFacts>,
    ) -> Result<QueryFragment> {
        self.inner.predicate(fact, context, facts)
    }
    fn relation(
        &self,
        relation: &RelationKey,
        context: &AccessContext,
        facts: Option<&MappingFacts>,
    ) -> Result<RelationMapping> {
        self.inner.relation(relation, context, facts)
    }
}

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
        qualified: QualifiedPermissions::test_grants(&[]),
        approval_source: None,
        task: false,
        rule,
        whole_rule,
        management_rule: None,
        has_role_scope: false,
        version: "phase5".into(),
        valid_until: None,
        policy_version: 1,
        organization_version: 1,
        boundary: executor.boundary().unwrap(),
        service: Arc::new(()),
    }
}
async fn plan(
    permission: Permission,
    rule: AccessRule,
    whole: Option<AccessRule>,
    mapping: Arc<dyn AccessMapping>,
    executor: &mut dyn Executor,
) -> CompiledAccess {
    let model = mapping.model().clone();
    let proof = proof(permission, rule, whole, &model, executor);
    let frozen = FrozenMapping::freeze(mapping, &mut vec![], 0).unwrap();
    compile(&proof, &frozen, &model, executor).await.unwrap()
}
fn ids(rows: Vec<Document>) -> Vec<String> {
    rows.into_iter().map(|row| row.get_str("id").unwrap().to_string()).collect()
}
fn row(id: &str, owner: &str) -> Document {
    doc! {"id":id,"owner":owner,"deleted_at":0}
}
fn relation(rule: AccessRule) -> AccessRule {
    AccessRule::Related {
        relation: RelationKey::parse("sales_source").unwrap(),
        resource: "sales_order".into(),
        mode: RelationMatch::Any,
        rule: Box::new(rule),
    }
}
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

#[tokio::test]
async fn source_portions_keep_only_owned_live_sources_and_never_imply_whole_ledger() {
    let mut executor = ReadExecutor::default();
    let mut root = mapping(&receivable::CUSTOMER_RECEIPT_LIST);
    Arc::get_mut(&mut root).unwrap().model.output = QueryOutput::Existence { fields: ["id".into()].into() };
    let (share, whole) = receipt_rules(false);
    let plan = plan(receivable::CUSTOMER_RECEIPT_LIST, share, Some(whole), root, &mut executor).await;
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

#[tokio::test]
async fn whole_ledger_requires_every_reference_and_keeps_real_zero_allocation_roots() {
    let mut executor = ReadExecutor::default();
    let mut root = mapping(&receivable::CUSTOMER_RECEIPT_LIST);
    Arc::get_mut(&mut root).unwrap().model.output = QueryOutput::Existence { fields: ["id".into()].into() };
    let (share, whole) = receipt_rules(true);
    let plan = plan(receivable::CUSTOMER_RECEIPT_LIST, share, Some(whole), root, &mut executor).await;
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

#[tokio::test]
async fn valid_source_conditions_do_not_bypass_root_record_integrity() {
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
    )
    .await;
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

#[tokio::test]
async fn independent_source_plan_uses_its_entry_context_and_reuses_exact_preparation() {
    let mut executor = ReadExecutor::default();
    let mut child = mapping(&sales_order::DETAIL);
    Arc::get_mut(&mut child).unwrap().dynamic = true;
    let decorated =
        Arc::new(DecoratedMapping { entry_sensitive: true, ..DecoratedMapping::new(child.clone()) });
    let mut root = mapping(&receivable::CUSTOMER_RECEIPT_LIST);
    let mutable = Arc::get_mut(&mut root).unwrap();
    mutable.model.output = QueryOutput::Existence { fields: ["id".into()].into() };
    mutable.relations.insert(RelationKey::parse("sales_source").unwrap(), decorated);
    let rule = AccessRule::Independent {
        permission: sales_order::DETAIL,
        rule: Box::new(relation(AccessRule::Fact(AccessFact::Owned))),
    };
    let plan = plan(receivable::CUSTOMER_RECEIPT_LIST, rule.clone(), Some(rule), root, &mut executor).await;
    assert_eq!(child.prepares.load(Ordering::SeqCst), 1);
    assert_eq!(plan.dependencies.len(), 1);
    assert_eq!(plan.dependencies[0].context.permission, sales_order::DETAIL);
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
    child.revision.store(2, Ordering::SeqCst);
    assert!(matches!(plan.dependencies[0].validate(&mut executor).await, Err(Error::ConflictError(_))));
}

#[tokio::test]
async fn static_deadline_needs_no_preparation_and_expires_the_compiled_plan() {
    let mut executor = ReadExecutor::default();
    let deadline = Instant::from_unix_secs(Instant::now().unix_secs() + 2);
    let root = Arc::new(DecoratedMapping {
        deadline: Some(deadline),
        ..DecoratedMapping::new(mapping(&sales_order::DETAIL))
    });
    assert!(root.preparation().is_none());
    let plan = plan(sales_order::DETAIL, AccessRule::Allow, None, root, &mut executor).await;
    assert_eq!(plan.proof.valid_until, Some(deadline));
    assert!(plan.dependencies.is_empty());
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    assert!(matches!(plan.ensure_current(&mut executor), Err(Error::ConflictError(_))));
}

#[tokio::test]
async fn dynamic_deadline_and_phase_changes_reject_delivery() {
    let mut executor = ReadExecutor::default();
    let mut root = mapping(&sales_order::DETAIL);
    Arc::get_mut(&mut root).unwrap().dynamic = true;
    let deadline = Instant::from_unix_secs(Instant::now().unix_secs() + 60);
    *root.expires.lock().unwrap() = Some(deadline);
    let plan =
        plan(sales_order::DETAIL, AccessRule::Fact(AccessFact::Owned), None, root.clone(), &mut executor)
            .await;
    assert_eq!(plan.proof.valid_until, Some(deadline));
    assert_eq!(root.prepares.load(Ordering::SeqCst), 1);
    *root.expires.lock().unwrap() = Some(Instant::from_unix_secs(1));
    assert!(matches!(plan.dependencies[0].validate(&mut executor).await, Err(Error::ConflictError(_))));
    executor.next_read();
    assert!(matches!(plan.ensure_current(&mut executor), Err(Error::ValidationError(_))));
}

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

#[tokio::test]
async fn fixed_lookup_preserves_missing_relations_and_is_immutable_after_binding() {
    let mut executor = ReadExecutor::default();
    let root = Arc::new(DecoratedMapping::new(mapping(&sales_order::DETAIL)));
    root.fixed.lock().unwrap().push(doc! {"$lookup":{"from":"optional","let":{"root":"$id"},"pipeline":[{"$match":{"$expr":{"$eq":["$root_id","$$root"]}}}],"as":"optional"}});
    let frozen = FrozenMapping::freeze(root.clone(), &mut vec![], 0).unwrap();
    let proof =
        proof(sales_order::DETAIL, AccessRule::Fact(AccessFact::Owned), None, root.model(), &executor);
    let plan = compile(&proof, &frozen, root.model(), &mut executor).await.unwrap();
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
        compile(&proof, &frozen, root.model(), &mut executor).await,
        Err(Error::ValidationError(_))
    ));
}

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
    assert!(matches!(frozen.records(&context, None), Err(Error::ValidationError(_))));
}
