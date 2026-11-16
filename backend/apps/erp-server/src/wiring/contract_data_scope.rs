//! 合同责任事实和来源接线；统一授权服务决定所有范围组合。
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_contract::authorization::{ContractAccess, ContractModels, scope};
use erp_contract::ports::ContractCreationPort;
use erp_contract::repository::ContractExt;
use erp_contract::{ContractParticipantPort, Error as ContractError};
use erp_core::common::time::Instant;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, FactValue, MappingFacts, ObjectFacts, QueryModel, QueryModelKey,
    QueryOutput, ReadAccess, RelationKey, RelationMatch,
};
use erp_identity::ports::{AccessFactsPort, AccessMapping, QueryFragment, RelationMapping};
use erp_identity::repository::OrganizationRepository;
use erp_identity::repository::organization::primary_membership_query;
use erp_identity::{
    AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AuthorizationRegistry, AuthorizationService,
    DataAccess, Error as IdentityError, OrganizationDirectoryService, Permission, ResourceTargets,
    Result as IdentityResult, SharedRbacService,
};
use erp_workflow::repository::document_registry::participant_access;
use erp_workflow::{DocumentRegistryExt, Error as WorkflowError};
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, mongo_ops};

use super::contract::MongoContractParticipants;
use super::identity_error::map_identity_error;
map_identity_error!(erp_contract);

#[derive(Clone)]
struct ContractModelsBuilder {
    db: Database,
    rbac: SharedRbacService,
}
impl ContractModelsBuilder {
    /// 绑定身份设施。
    /// # 参数
    /// db和rbac为共享设施。
    /// # 返回
    /// 未读取数据的adapter。
    /// # 错误
    /// 无。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }
    fn mapping(&self) -> erp_identity::Result<ContractMapping> {
        Ok(ContractMapping {
            responsibility: ContractResponsibility::Read,
            db: self.db.clone(),
            model: QueryModel {
                key: QueryModelKey::parse("contract_read_responsibility")?,
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
            },
        })
    }
    fn service(&self, action: &str) -> IdentityResult<(AuthorizationService, Arc<dyn AccessMapping>)> {
        let read = self.mapping()?;
        let write = ContractMapping {
            responsibility: ContractResponsibility::Write,
            model: QueryModel {
                key: QueryModelKey::parse("contract_write_responsibility")?,
                ..read.model.clone()
            },
            ..read.clone()
        };
        let read: Arc<dyn AccessMapping> = Arc::new(read);
        let write: Arc<dyn AccessMapping> = Arc::new(write);
        let mapping = match action {
            "list" | "detail" => read.clone(),
            "update" => write.clone(),
            _ => return Err(erp_identity::Error::ValidationError("未登记合同动作".into())),
        };
        let entries = [
            ("list", ContractResponsibility::Read, read.clone()),
            ("detail", ContractResponsibility::Read, read),
            ("update", ContractResponsibility::Write, write),
        ]
        .into_iter()
        .map(|(action, responsibility, mapping)| {
            contract_registration(self.db.clone(), action, responsibility, mapping)
        })
        .collect();
        Ok((
            AuthorizationService::new(
                self.db.clone(),
                self.rbac.clone(),
                AuthorizationRegistry::new(entries)?,
            ),
            mapping,
        ))
    }
}
impl ContractModelsBuilder {
    /// 装配实际客户来源的固定创建模型。
    fn creation(&self) -> erp_contract::Result<(AuthorizationService, Arc<dyn AccessMapping>)> {
        let mapping = creation_mapping(self.db.clone()).map_err(map_identity_error)?;
        let permission = Permission::parse("contract:create")?;
        let registry = AuthorizationRegistry::new(vec![AccessRegistration {
            permission: permission.clone(),
            required_permissions: vec![],
            access: AccessMode::Query {
                data: DataAccess {
                    subject: AccessSubject::Direct,
                    required_facts: Default::default(),
                    policy: AccessPolicy::Related {
                        relation: RelationKey::parse("customer").map_err(map_identity_error)?,
                        resource: "customer".into(),
                        mode: RelationMatch::AllNonEmpty,
                        required_facts: [FactKind::Owner, FactKind::Organization].into(),
                        policy: Box::new(AccessPolicy::Scope {
                            permission: Permission::parse("customer:list")?,
                            read: ReadAccess::default(),
                        }),
                    },
                },
                mapping: mapping.clone(),
            },
        }])
        .map_err(map_identity_error)?;
        let service = AuthorizationService::new(self.db.clone(), self.rbac.clone(), registry);
        Ok((service, mapping))
    }
}

/// 装配合同固定模型，保留跨域责任事实与历史参与事实。
/// # 参数
/// db、rbac 为应用共享身份设施。
/// # 返回
/// 领域可直接消费的统一授权能力。
/// # 错误
/// 固定目录非法时启动失败。
pub fn contract_authorization(db: Database, rbac: SharedRbacService) -> ContractAccess {
    let builder = ContractModelsBuilder::new(db.clone(), rbac.clone());
    let models = ContractModels {
        read: builder.service("detail").expect("固定合同读取目录有效"),
        write: builder.service("update").expect("固定合同写入目录有效"),
        creation: Arc::new(CreationAccess {
            db: db.clone(),
            model: builder.creation().expect("固定合同客户来源目录有效"),
        }),
    };
    ContractAccess::new(db, models)
}

struct CreationAccess {
    db: Database,
    model: (AuthorizationService, Arc<dyn AccessMapping>),
}

#[async_trait]
impl ContractCreationPort for CreationAccess {
    async fn require_create(
        &self,
        actor: &AuditActor,
        customer_id: &str,
        executor: &mut dyn Executor,
    ) -> erp_contract::Result<erp_contract::ContractResolvedScope> {
        let (service, mapping) = &self.model;
        let permission = Permission::parse("contract:create")?;
        let query = service
            .query(actor, &permission, AccessSubject::Direct, executor)
            .await
            .map_err(map_identity_error)?;
        let compiled = query.compile(mapping.as_ref(), executor).await.map_err(map_identity_error)?;
        service.validate(&compiled, executor).await.map_err(map_identity_error)?;
        let mut pipeline = compiled.stages().to_vec();
        pipeline.push(doc! {"$match":{"id":customer_id}});
        pipeline.push(doc! {"$limit":1});
        let rows = persistence_core::aggregate::rows(
            &self.db.collection::<Document>(&mapping.model().collection),
            pipeline,
            executor,
        )
        .await?;
        service.validate(&compiled, executor).await.map_err(map_identity_error)?;
        if rows.is_empty() {
            return Err(erp_contract::Error::Forbidden("当前账号无权为该客户归档合同".into()));
        }
        Ok(scope(compiled, service.clone()))
    }
}

/// 合同既有责任口径由固定模型登记，不从角色或请求解释。
#[derive(Clone, Copy, PartialEq, Eq)]
enum ContractResponsibility {
    Read,
    Write,
}

impl ContractResponsibility {
    /// 只有读取模型依赖历史参与修订，写入不读取无关参与事实。
    async fn revision(
        self,
        db: &Database,
        context: &AccessContext,
        executor: &mut dyn Executor,
    ) -> IdentityResult<Option<i64>> {
        match self {
            Self::Read => participant_access::revision(db, context.actor.id(), executor)
                .await
                .map(Some)
                .map_err(|error| match error {
                    WorkflowError::RepositoryError(error) => ContractError::from(error),
                    WorkflowError::Internal(message) => ContractError::Internal(message),
                    error => ContractError::Internal(error.to_string()),
                })
                .map_err(|error| IdentityError::Internal(error.to_string())),
            Self::Write => Ok(None),
        }
    }

    /// 对象事实只装载本次目标的参与身份，写入模型保留已知空集合。
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

/// 单条合同历史参与采用存在性关联，账号其他参与记录不进入查询条件。
fn history_predicate(user: &str) -> QueryFragment {
    QueryFragment::Exists {
        collection: Database::DOCUMENT_PARTICIPANTS.into(),
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
    responsibility: ContractResponsibility,
    db: Database,
    model: QueryModel,
}
#[async_trait]
impl AccessMapping for ContractMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn key(&self, _: &AccessContext, _: &MappingFacts) -> erp_identity::Result<Bson> {
        Ok("$id".into())
    }
    async fn prepare(
        &self,
        context: &AccessContext,
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<MappingFacts> {
        let state = OrganizationRepository::new(&self.db).state(executor).await?;
        let participation = self.responsibility.revision(&self.db, context, executor).await?;
        let valid_until = state
            .memberships
            .iter()
            .flat_map(|m| [Some(m.validity.valid_from), m.validity.valid_to])
            .flatten()
            .filter(|at| *at > context.as_of)
            .min();
        Ok(MappingFacts { version: Some(format!("{}:{participation:?}", state.version)), valid_until })
    }
    fn records(&self, _: &AccessContext, _: &MappingFacts) -> erp_identity::Result<QueryFragment> {
        Ok(QueryFragment::Match(doc! {"deleted_at":NOT_DELETED_TIMESTAMP_BSON}))
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: &MappingFacts,
    ) -> erp_identity::Result<QueryFragment> {
        let reading = self.responsibility == ContractResponsibility::Read;
        let filter = match fact {
            AccessFact::Owned if reading => {
                doc! {"$or":[{"sales_owner_user_id":context.actor.id()},{"shared_sales_user_ids":context.actor.id()}]}
            },
            AccessFact::Owned => doc! {"sales_owner_user_id":context.actor.id()},
            AccessFact::Collaborating if reading => doc! {"shared_sales_user_ids":context.actor.id()},
            AccessFact::Collaborating => doc! {"$expr":false},
            AccessFact::OrganizationIn(orgs) => {
                return Ok(primary_membership_query(
                    "$sales_owner_user_id".into(),
                    Some(orgs),
                    context.as_of,
                ));
            },
            AccessFact::HistoryParticipant if reading => return Ok(history_predicate(context.actor.id())),
            AccessFact::HistoryParticipant => doc! {"$expr":false},
            _ => return Err(erp_identity::Error::ValidationError("合同不支持该授权事实".into())),
        };
        Ok(QueryFragment::Match(filter))
    }
    fn relation(
        &self,
        _: &RelationKey,
        _: &AccessContext,
        _: &MappingFacts,
    ) -> erp_identity::Result<RelationMapping> {
        Err(erp_identity::Error::ValidationError("合同本体没有来源授权关系".into()))
    }
}
struct ContractFacts {
    responsibility: ContractResponsibility,
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
    ) -> erp_identity::Result<Vec<ObjectFacts>> {
        let rows=mongo_ops::find_many(&self.db.collection::<Document>(<Database as ContractExt>::CONTRACTS),doc!{"id":{"$in":ids.ids().iter().cloned().collect::<Vec<_>>()} ,"deleted_at":NOT_DELETED_TIMESTAMP_BSON},FindOptions::default(),executor).await?;
        let targets = ids.ids().iter().cloned().collect::<Vec<_>>();
        let history =
            self.responsibility.history(self.participants.as_ref(), context, &targets, executor).await?;
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
            objects.push(contract_object(row, context, organization, &history, self.responsibility)?);
        }
        Ok(objects)
    }
}
struct CreationMapping {
    source: Arc<dyn AccessMapping>,
    model: QueryModel,
}
#[async_trait]
impl AccessMapping for CreationMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn key(&self, c: &AccessContext, p: &MappingFacts) -> erp_identity::Result<Bson> {
        self.source.key(c, p)
    }
    async fn prepare(&self, c: &AccessContext, e: &mut dyn Executor) -> erp_identity::Result<MappingFacts> {
        self.source.prepare(c, e).await
    }
    fn records(&self, c: &AccessContext, p: &MappingFacts) -> erp_identity::Result<QueryFragment> {
        self.source.records(c, p)
    }
    fn predicate(
        &self,
        _: &AccessFact,
        _: &AccessContext,
        _: &MappingFacts,
    ) -> erp_identity::Result<QueryFragment> {
        Err(erp_identity::Error::ValidationError("创建根模型不支持范围事实".into()))
    }
    fn relation(
        &self,
        key: &RelationKey,
        _: &AccessContext,
        _: &MappingFacts,
    ) -> erp_identity::Result<RelationMapping> {
        if key.as_str() != "customer" {
            return Err(erp_identity::Error::ValidationError("未知合同来源关系".into()));
        }
        Ok(RelationMapping { expected_ids: Bson::Array(vec!["$id".into()]), mapping: self.source.clone() })
    }
}

/// 创建入口使用客户记录作为根模型，保留完整来源关系。
fn creation_mapping(db: Database) -> erp_identity::Result<Arc<dyn AccessMapping>> {
    let source = erp_customer::authorization::CustomerMapping::shared(db);
    let model = QueryModel {
        key: QueryModelKey::parse("contract_creation")?,
        resource: "contract".into(),
        ..source.model().clone()
    };
    Ok(Arc::new(CreationMapping { source, model }))
}

/// 将已读取合同及当前组织事实转换为identity事实。
fn contract_object(
    row: Document,
    context: &AccessContext,
    organization: (Option<String>, Option<Instant>),
    history: &[String],
    responsibility: ContractResponsibility,
) -> erp_identity::Result<ObjectFacts> {
    let id = row
        .get_str("id")
        .map_err(|_| erp_identity::Error::ValidationError("合同主键缺失".into()))?
        .to_owned();
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
            .ok_or_else(|| erp_identity::Error::ValidationError("合同缺少业务版本".into()))?
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

/// 固定合同动作及读写模型登记。
fn contract_registration(
    db: Database,
    action: &str,
    responsibility: ContractResponsibility,
    mapping: Arc<dyn AccessMapping>,
) -> AccessRegistration {
    let permission = Permission::parse(format!("contract:{action}")).expect("固定入口");
    AccessRegistration {
        permission: permission.clone(),
        required_permissions: vec![],
        access: AccessMode::QueryAndObject {
            data: DataAccess {
                subject: AccessSubject::Direct,
                required_facts: [FactKind::Owner, FactKind::Organization].into(),
                policy: AccessPolicy::Scope {
                    permission,
                    read: ReadAccess {
                        history: responsibility == ContractResponsibility::Read,
                        records: false,
                    },
                },
            },
            mapping,
            facts: Arc::new(ContractFacts {
                db: db.clone(),
                participants: MongoContractParticipants::shared(db.clone()),
                responsibility,
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_contract::FailClosedContractParticipantPort;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use persistence_core::NoTransaction;

    use super::*;

    fn context() -> AccessContext {
        AccessContext {
            actor: AuditActor::new("sales-a".into(), "sales-a".into(), AccountKind::Admin),
            permission: Permission::parse("contract:detail").unwrap(),
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
            Err(erp_identity::Error::ValidationError(_))
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
        let QueryFragment::Exists { collection, bindings, pipeline } = history_predicate("sales-a") else {
            panic!("历史参与必须通过存在性关联读取");
        };
        assert_eq!(collection, Database::DOCUMENT_PARTICIPANTS);
        assert_eq!(bindings, doc! {"contract_id": "$id"});
        assert_eq!(
            pipeline,
            vec![doc! {"$match": {
                "participant_user_id": "sales-a", "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                "$expr": {"$eq": ["$document_id", "$$contract_id"]},
            }}]
        );
    }

    #[tokio::test]
    async fn writes_do_not_depend_on_the_read_participation_port() {
        let facts = ContractResponsibility::Write;
        let port = FailClosedContractParticipantPort;
        let targets = ["contract-a".into()];
        assert_eq!(
            facts.history(&port, &context(), &targets, &mut NoTransaction).await.unwrap(),
            Vec::<String>::new()
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
        ) -> erp_contract::Result<Vec<String>> {
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
}
