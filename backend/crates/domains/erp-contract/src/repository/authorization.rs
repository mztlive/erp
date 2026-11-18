//! 合同责任事实与数据库映射；只消费组合层注入的历史参与来源。
mod creation;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use async_trait::async_trait;
pub use creation::creation_facts;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::Instant;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, FactValue, MappingFacts, ObjectFacts, QueryModel, QueryModelKey,
    QueryOutput, RelationKey,
};
use erp_identity::ports::{AccessFactsPort, AccessMapping, QueryFragment, RelationMapping};
use erp_identity::repository::organization::primary_membership_query;
use erp_identity::{
    Error as IdentityError, OrganizationDirectoryService, ResourceTargets, Result as IdentityResult,
};
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, mongo_ops};

use crate::ports::ContractParticipantPort;
use crate::repository::ContractExt;

/// 构造本域合同责任事实和查询映射，不授予功能资格。
/// 本域实现的根事实和完整对象映射；同次启动的各合同查询复用这两个实例。
/// db 为合同数据库，participants 为真实历史参与端口；history_collection 为该端口的固定来源集合。
/// 查询模型或历史来源集合标识非法时拒绝；构造不执行数据库读取。
pub fn bindings(
    db: Database,
    participants: Arc<dyn ContractParticipantPort>,
    history_collection: &str,
) -> IdentityResult<(Arc<dyn AccessFactsPort>, Arc<dyn AccessMapping>)> {
    if history_collection.trim().is_empty() || history_collection.contains('\0') {
        return Err(IdentityError::ValidationError("合同历史参与来源集合无效".into()));
    }

    let facts = Arc::new(ContractFacts { db, participants });
    let model = QueryModel {
        key: QueryModelKey::parse("contract_responsibility")?,
        resource: "contract".into(),
        collection: <Database as ContractExt>::CONTRACTS.into(),
        id_field: "id".into(),
        output: QueryOutput::Object,
        facts: [
            FactKind::Owner,
            FactKind::Collaborators,
            FactKind::Organization,
            FactKind::HistoryParticipants,
        ]
        .into(),
    };
    model.validate()?;
    let mapping = Arc::new(ContractMapping { model, history_collection: history_collection.into() });
    Ok((facts, mapping))
}

/// 合同既有责任口径由固定模型登记，不从角色或请求解释。
#[derive(Clone, Copy, PartialEq, Eq)]
enum ContractResponsibility {
    Read,
    Write,
}

impl ContractResponsibility {
    // 对象事实只装载本次目标的参与身份，写入模型保留已知空集合。
    async fn history(
        self,
        participants: &dyn ContractParticipantPort,
        context: &AccessContext,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> IdentityResult<Vec<String>> {
        match self {
            Self::Read => participants
                .document_ids_in(context.actor.id(), ids, executor)
                .await
                .map_err(|error| IdentityError::Internal(error.to_string())),
            Self::Write => Ok(Vec::new()),
        }
    }
}

// 单条合同历史参与采用存在性关联，账号其他参与记录不进入查询条件。
fn history_predicate(collection: &str, user: &str) -> QueryFragment {
    QueryFragment::Exists {
        collection: collection.into(),
        bindings: doc! {"contract_id": "$id"},
        pipeline: vec![doc! {"$match": {
            "participant_user_id": user,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "$expr": {"$eq": ["$document_id", "$$contract_id"]},
        }}],
    }
}

#[derive(Clone)]
struct ContractMapping {
    model: QueryModel,
    history_collection: String,
}
#[async_trait]
impl AccessMapping for ContractMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn key(&self, _: &AccessContext, _: Option<&MappingFacts>) -> IdentityResult<Bson> {
        Ok("$id".into())
    }
    fn records(&self, _: &AccessContext, _: Option<&MappingFacts>) -> IdentityResult<QueryFragment> {
        Ok(QueryFragment::Match(doc! {"deleted_at":NOT_DELETED_TIMESTAMP_BSON}))
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> IdentityResult<QueryFragment> {
        let filter = match fact {
            AccessFact::Owned => {
                doc! {"$or":[{"sales_owner_user_id":context.actor.id()},{"shared_sales_user_ids":context.actor.id()}]}
            },
            AccessFact::Collaborating => doc! {"shared_sales_user_ids":context.actor.id()},
            AccessFact::OrganizationIn(orgs) => {
                return Ok(primary_membership_query(
                    "$sales_owner_user_id".into(),
                    Some(orgs),
                    context.as_of,
                ));
            },
            AccessFact::HistoryParticipant => {
                return Ok(history_predicate(&self.history_collection, context.actor.id()));
            },
            _ => return Err(IdentityError::ValidationError("合同不支持该授权事实".into())),
        };
        Ok(QueryFragment::Match(filter))
    }
    fn relation(
        &self,
        _: &RelationKey,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> IdentityResult<RelationMapping> {
        Err(IdentityError::ValidationError("合同本体没有来源授权关系".into()))
    }
}
struct ContractFacts {
    db: Database,
    participants: Arc<dyn ContractParticipantPort>,
}
#[async_trait]
impl AccessFactsPort for ContractFacts {
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> IdentityResult<Vec<ObjectFacts>> {
        let rows=mongo_ops::find_many(&self.db.collection::<Document>(<Database as ContractExt>::CONTRACTS),doc!{"id":{"$in":ids.ids().iter().cloned().collect::<Vec<_>>()} ,"deleted_at":NOT_DELETED_TIMESTAMP_BSON},FindOptions::default(),executor).await?;

        let targets = ids.ids().iter().cloned().collect::<Vec<_>>();
        let responsibility = if matches!(context.permission.action(), "list" | "detail") {
            ContractResponsibility::Read
        } else {
            ContractResponsibility::Write
        };
        let history = responsibility.history(self.participants.as_ref(), context, &targets, executor).await?;

        let directory = OrganizationDirectoryService::new(self.db.clone());
        let mut organizations = BTreeMap::new();
        let mut objects = Vec::with_capacity(rows.len());
        for row in rows {
            let owner = row.get_str("sales_owner_user_id").ok().filter(|owner| !owner.is_empty());
            let organization = if let Some(owner) = owner {
                if !organizations.contains_key(owner) {
                    let fact = directory.own_org_with_validity(owner, context.as_of, executor).await?;
                    organizations.insert(owner.to_owned(), fact);
                }
                organizations.get(owner).cloned().expect("负责人组织事实已写入")
            } else {
                (None, None)
            };
            objects.push(contract_object(row, context, organization, &history, responsibility)?);
        }
        Ok(objects)
    }
}

// 将已读取合同及当前组织事实转换为identity事实。
fn contract_object(
    row: Document,
    context: &AccessContext,
    organization: (Option<String>, Option<Instant>),
    history: &[String],
    responsibility: ContractResponsibility,
) -> IdentityResult<ObjectFacts> {
    let id = row.get_str("id").map_err(|_| IdentityError::ValidationError("合同主键缺失".into()))?.to_owned();
    let owner = row.get_str("sales_owner_user_id").ok().filter(|id| !id.is_empty()).map(str::to_owned);
    let shared: BTreeSet<String> = row
        .get_array("shared_sales_user_ids")
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();

    let read = responsibility == ContractResponsibility::Read;
    let owned = if read && shared.contains(context.actor.id()) {
        Some(context.actor.id().to_owned())
    } else {
        owner.clone()
    };

    let (org, valid_until) = organization;
    Ok(ObjectFacts {
        resource: "contract".into(),
        id: id.clone(),
        version: row
            .get("version")
            .ok_or_else(|| IdentityError::ValidationError("合同缺少业务版本".into()))?
            .to_string(),
        owner_id: FactValue::Known(owned),
        org_unit_id: FactValue::Known(org),
        valid_until,
        collaborators: FactValue::Known(if read { shared } else { BTreeSet::new() }),
        history_participants: FactValue::Known(if read && history.contains(&id) {
            [context.actor.id().into()].into()
        } else {
            BTreeSet::new()
        }),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_permissions::contract;
    use mongodb::Client;
    use mongodb::options::{ClientOptions, ServerAddress};
    use persistence_core::NoTransaction;

    use super::*;
    use crate::{EmptyContractParticipants, FailClosedContractParticipantPort, Result};

    fn context() -> AccessContext {
        AccessContext {
            actor: AuditActor::new("sales-a".into(), "sales-a".into(), AccountKind::Admin),
            permission: contract::DETAIL,
            resource: "contract".into(),
            model: None,
            as_of: Instant::from_unix_secs(1),
        }
    }

    #[test]
    fn fixed_responsibility_models_preserve_shared_reads_and_independent_writes() {
        let row = doc! {"id":"contract-a","version":1_i64,"sales_owner_user_id":"sales-b","shared_sales_user_ids":["sales-a"]};
        let reading =
            contract_object(row.clone(), &context(), (None, None), &[], ContractResponsibility::Read)
                .unwrap();
        let writing =
            contract_object(row, &context(), (None, None), &[], ContractResponsibility::Write).unwrap();
        assert_eq!(reading.owner_id.get().unwrap().as_deref(), Some("sales-a"));
        assert!(reading.collaborators.get().unwrap().contains("sales-a"));
        assert_eq!(writing.owner_id.get().unwrap().as_deref(), Some("sales-b"));
        assert!(writing.collaborators.get().unwrap().is_empty());
        assert_eq!(reading.org_unit_id.get().unwrap(), writing.org_unit_id.get().unwrap());
    }

    #[test]
    fn incomplete_contract_facts_fail_before_authorization() {
        let row = doc! {"id":"contract-a","sales_owner_user_id":"sales-a"};
        assert!(matches!(
            contract_object(row, &context(), (None, None), &[], ContractResponsibility::Read),
            Err(IdentityError::ValidationError(_))
        ));
    }

    #[test]
    fn object_facts_preserve_owner_organization_and_its_next_time_boundary() {
        let boundary = Instant::from_unix_secs(100);
        for responsibility in [ContractResponsibility::Read, ContractResponsibility::Write] {
            let object = contract_object(
                doc! {"id":"contract-a","version":1_i64,"sales_owner_user_id":"sales-b"},
                &context(),
                (Some("org-b".into()), Some(boundary)),
                &[],
                responsibility,
            )
            .unwrap();
            assert_eq!(object.org_unit_id.get().unwrap().as_deref(), Some("org-b"));
            assert_eq!(object.valid_until, Some(boundary));
        }
        let future = contract_object(
            doc! {"id":"contract-a","version":1_i64,"sales_owner_user_id":"sales-b"},
            &context(),
            (None, Some(boundary)),
            &[],
            ContractResponsibility::Read,
        )
        .unwrap();
        assert!(future.org_unit_id.get().unwrap().is_none());
        assert_eq!(future.valid_until, Some(boundary));
    }

    #[test]
    fn history_query_correlates_the_current_contract_before_matching_participation() {
        let QueryFragment::Exists { collection, bindings, pipeline } =
            history_predicate("contract_participation_fixture", "sales-a")
        else {
            panic!("历史参与必须通过存在性关联读取");
        };
        assert_eq!(collection, "contract_participation_fixture");
        assert_eq!(bindings, doc! {"contract_id": "$id"});
        assert_eq!(
            pipeline,
            vec![doc! {"$match": {
                "participant_user_id": "sales-a", "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                "$expr": {"$eq": ["$document_id", "$$contract_id"]},
            }}]
        );
    }

    struct Participants {
        targets: Mutex<Vec<Vec<String>>>,
    }

    #[async_trait]
    impl ContractParticipantPort for Participants {
        async fn document_ids_in(
            &self,
            user: &str,
            ids: &[String],
            _: &mut dyn Executor,
        ) -> Result<Vec<String>> {
            assert_eq!(user, "sales-a");
            self.targets.lock().unwrap().push(ids.to_vec());
            Ok(ids.to_vec())
        }
    }

    #[tokio::test]
    async fn reads_load_only_the_requested_contract_targets() {
        let port = Participants { targets: Mutex::new(Vec::new()) };
        let targets = ["contract-a".to_owned()];
        let facts = ContractResponsibility::Read;
        assert!(port.targets.lock().unwrap().is_empty());
        assert_eq!(facts.history(&port, &context(), &targets, &mut NoTransaction).await.unwrap(), targets);
        assert_eq!(*port.targets.lock().unwrap(), vec![targets.to_vec()]);
        let object = contract_object(
            doc! {"id":"contract-a","version":1_i64,"sales_owner_user_id":"sales-b"},
            &context(),
            (None, None),
            &targets,
            facts,
        )
        .unwrap();
        assert_eq!(object.history_participants.get().unwrap(), &BTreeSet::from(["sales-a".to_owned()]));
    }

    #[tokio::test]
    async fn bound_mapping_executes_contract_ownership_and_injected_history_source() {
        let options = ClientOptions::builder()
            .hosts(vec![ServerAddress::Tcp { host: "127.0.0.1".into(), port: Some(1) }])
            .build();
        let db = Client::with_options(options).unwrap().database("contract_authorization_unit");
        let (_, mapping) =
            bindings(db.clone(), Arc::new(EmptyContractParticipants), "participation").unwrap();
        assert_eq!(mapping.model().collection, <Database as ContractExt>::CONTRACTS);
        assert_eq!(mapping.model().output, QueryOutput::Object);
        assert!(mapping.preparation().is_none());

        let QueryFragment::Match(ownership) =
            mapping.predicate(&AccessFact::Owned, &context(), None).unwrap()
        else {
            panic!("合同责任必须生成真实字段条件");
        };
        assert_eq!(
            ownership,
            doc! {"$or": [{"sales_owner_user_id": "sales-a"}, {"shared_sales_user_ids": "sales-a"}]}
        );
        let orgs = BTreeSet::from(["org-a".into()]);
        let QueryFragment::Exists { bindings: member, pipeline: memberships, .. } =
            mapping.predicate(&AccessFact::OrganizationIn(orgs), &context(), None).unwrap()
        else {
            panic!("合同组织范围必须关联真实主属关系");
        };
        assert_eq!(member, doc! {"member": "$sales_owner_user_id"});
        assert_eq!(
            memberships[0].get_document("$match").unwrap().get_document("valid_from").unwrap(),
            &doc! {"$lte": context().as_of.unix_secs()}
        );
        assert_eq!(memberships.last().unwrap(), &doc! {"$match": {"count": 1, "org": {"$in": ["org-a"]}}});

        let QueryFragment::Exists { collection, bindings: bound, pipeline } =
            mapping.predicate(&AccessFact::HistoryParticipant, &context(), None).unwrap()
        else {
            panic!("历史参与必须检查真实来源");
        };
        assert_eq!(collection, "participation");
        assert_eq!(bound, doc! {"contract_id": "$id"});
        assert_eq!(
            pipeline[0].get_document("$match").unwrap().get_str("participant_user_id").unwrap(),
            "sales-a"
        );

        assert!(matches!(
            bindings(db, Arc::new(EmptyContractParticipants), ""),
            Err(IdentityError::ValidationError(_))
        ));
    }

    #[tokio::test]
    async fn writes_do_not_load_history_and_reads_fail_when_participants_are_unwired() {
        let port = FailClosedContractParticipantPort;
        let targets = ["contract-a".into()];
        assert_eq!(
            ContractResponsibility::Write
                .history(&port, &context(), &targets, &mut NoTransaction)
                .await
                .unwrap(),
            Vec::<String>::new()
        );
        assert!(matches!(
            ContractResponsibility::Read
                .history(&port, &context(), &targets, &mut NoTransaction)
                .await,
            Err(IdentityError::Internal(message)) if message.contains("合同参与端口未接线")
        ));
    }
}
