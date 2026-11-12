//! 公共授权 API 的无数据库查询夹具；不执行真实 MongoDB 请求。
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_permissions::sales_order;
use mongodb::bson::{Bson, doc};
use persistence_core::Executor;
use serde_json::{Value, to_value};

use super::fixtures::execute;
use crate::entity::authorization::{
    AccessContext, AccessFact, AccessRule, FactKind, MappingFacts, QueryModel, QueryModelKey, QueryOutput,
    RelationKey,
};
use crate::ports::{AccessMapping, MappingPreparation, QueryFragment, RelationMapping};
use crate::service::authorization::rules;
use crate::{CompiledAccess, Error, Permission, Result};

pub(crate) struct Mapping {
    pub model: QueryModel,
    pub relations: BTreeMap<RelationKey, Arc<dyn AccessMapping>>,
    pub dynamic: bool,
    pub reliable: bool,
    pub revision: AtomicU64,
    pub prepares: AtomicU64,
    pub expires: Mutex<Option<Instant>>,
    pub change_stage: AtomicBool,
    pub replace_source: AtomicBool,
}
impl AccessMapping for Mapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn preparation(&self) -> Option<&dyn MappingPreparation> {
        if self.dynamic { Some(self) } else { None }
    }
    fn relations(&self) -> BTreeMap<RelationKey, Arc<dyn AccessMapping>> {
        self.relations.clone()
    }
    fn key(&self, _: &AccessContext, _: Option<&MappingFacts>) -> Result<Bson> {
        Ok("$id".into())
    }
    fn records(&self, _: &AccessContext, _: Option<&MappingFacts>) -> Result<QueryFragment> {
        Ok(QueryFragment::Match(doc! {"deleted_at":0}))
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> Result<QueryFragment> {
        Ok(QueryFragment::Match(match fact {
            AccessFact::Owned => doc! {"owner":context.actor.id()},
            AccessFact::HistoryParticipant => doc! {"history":context.actor.id()},
            AccessFact::AssignedTo => doc! {"assignee":context.actor.id()},
            AccessFact::Participant => doc! {"participant":context.actor.id()},
            AccessFact::Completed(completed) => doc! {"completed":completed},
            _ => return Err(Error::ValidationError("fixture unsupported predicate".into())),
        }))
    }
    fn relation(
        &self,
        key: &RelationKey,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> Result<RelationMapping> {
        if self.replace_source.load(Ordering::SeqCst) {
            return Ok(RelationMapping {
                expected_ids: format!("${}", key.as_str()).into(),
                mapping: mapping(&sales_order::DETAIL),
            });
        }
        Ok(RelationMapping {
            expected_ids: format!("${}", key.as_str()).into(),
            mapping: self
                .relations
                .get(key)
                .ok_or_else(|| Error::ValidationError("fixture missing relation".into()))?
                .clone(),
        })
    }
}

#[async_trait]
impl MappingPreparation for Mapping {
    async fn prepare(&self, _: &AccessContext, executor: &mut dyn Executor) -> Result<MappingFacts> {
        self.prepares.fetch_add(1, Ordering::SeqCst);
        if self.change_stage.load(Ordering::SeqCst) {
            executor.next_read();
        }
        let revision = self.revision.load(Ordering::SeqCst);
        Ok(MappingFacts {
            data: serde_json::json!({"revision":revision}),
            version: self.reliable.then(|| revision.to_string()),
            valid_until: *self.expires.lock().unwrap(),
        })
    }
}

pub(crate) fn mapping(permission: &Permission) -> Arc<Mapping> {
    let rule = rules::binding_condition(permission, &QueryOutput::Object).unwrap();
    model(permission.resource(), &rule)
}
fn model(resource: &str, rule: &AccessRule) -> Arc<Mapping> {
    let mut relations = BTreeMap::new();
    collect(rule, &mut relations);
    Arc::new(Mapping {
        model: QueryModel {
            key: QueryModelKey::parse(resource).unwrap(),
            resource: resource.into(),
            collection: format!("{resource}_rows"),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: [
                FactKind::Owner,
                FactKind::Organization,
                FactKind::Collaborators,
                FactKind::HistoryParticipants,
                FactKind::SettlementParty,
                FactKind::Warehouse,
                FactKind::Assignee,
                FactKind::Participants,
                FactKind::Completed,
            ]
            .into(),
        },
        relations,
        dynamic: false,
        reliable: true,
        revision: AtomicU64::new(1),
        prepares: AtomicU64::new(0),
        expires: Mutex::new(None),
        change_stage: AtomicBool::new(false),
        replace_source: AtomicBool::new(false),
    })
}
fn collect(rule: &AccessRule, relations: &mut BTreeMap<RelationKey, Arc<dyn AccessMapping>>) {
    match rule {
        AccessRule::Related { relation, resource, rule, .. } => {
            relations.insert(relation.clone(), model(resource, rule));
        },
        AccessRule::Required { rule, .. } | AccessRule::Independent { rule, .. } => collect(rule, relations),
        AccessRule::All(items) | AccessRule::Any(items) => {
            for rule in items {
                collect(rule, relations);
            }
        },
        _ => {},
    }
}

pub(crate) fn inquiry_members(access: &CompiledAccess) -> Vec<String> {
    let rows = execute(
        vec![
            doc! {"id":"one", "owner":"user", "deleted_at":0},
            doc! {"id":"other", "owner":"other", "deleted_at":0},
            doc! {"id":"deleted", "owner":"user", "deleted_at":1},
        ],
        access.stages(),
        &BTreeMap::new(),
        &doc! {},
    );
    rows.into_iter().map(|row| row.get_str("id").unwrap().to_owned()).collect()
}

pub(crate) fn receipt_projection(access: &CompiledAccess) -> Value {
    let collections =
        [("sales_order_rows".into(), vec![doc! {"id": "sale", "owner": "user", "deleted_at": 0}])].into();
    let rows = execute(
        vec![doc! {
            "id": "receipt", "deleted_at": 0,
            "sales_source": ["sale"], "purchase_source": [], "settlement_source": [],
            "total_amount": 1000, "amount": 1000, "payload": {"customer_name": "private"}
        }],
        access.stages(),
        &collections,
        &doc! {},
    );
    to_value(rows).unwrap()
}

pub(crate) fn task_members(access: &CompiledAccess, tasks: &[(&str, &str, bool, &str)]) -> Vec<String> {
    let rows = tasks
        .iter()
        .map(|(id, assignee, completed, participant)| {
            doc! {
                "id": *id, "assignee": *assignee, "completed": *completed,
                "participant": *participant, "deleted_at": 0
            }
        })
        .collect();
    let rows = execute(rows, access.stages(), &BTreeMap::new(), &doc! {});
    rows.into_iter().map(|row| row.get_str("id").unwrap().to_owned()).collect()
}
