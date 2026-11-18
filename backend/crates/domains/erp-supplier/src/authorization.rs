//! 本域授权登记和持久化事实；所有判定与条件均由身份域生成。
use std::sync::Arc;

use async_trait::async_trait;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::Instant;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, FactValue, MappingFacts, ObjectFacts, QueryModel, QueryModelKey,
    QueryOutput, RelationKey,
};
use erp_identity::ports::{AccessFactsPort, AccessMapping, QueryFragment, RelationMapping};
use erp_identity::{
    AuthorizationService, CompiledAccess, Error as IdentityError, ResourceTargets, Result as IdentityResult,
};
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::{Executor, mongo_ops};

use crate::repository::SupplierExt;

/// 身份域编译的当前阶段凭证；不含可由业务组合的权限条款。
#[derive(Clone)]
pub struct CompiledScope {
    pub(crate) compiled: Arc<CompiledAccess>,
    pub(crate) authorization: Arc<AuthorizationService>,
    pub scope_version: String,
}
impl std::fmt::Debug for CompiledScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledScope").field("compiled", &self.compiled).finish()
    }
}
impl CompiledScope {
    /// 返回签发此查询凭证时使用的权限策略版本，供范围版本比较使用；不重新查询策略。
    pub fn policy_version(&self) -> u64 {
        self.compiled.policy_version()
    }

    /// 返回查询凭证绑定的组织版本，供调用方判断组织范围是否已经变化。
    pub fn organization_version(&self) -> u64 {
        self.compiled.organization_version()
    }

    /// 取身份域编译本次查询时冻结的授权时点；不能把此时点用作后续请求的授权依据。
    pub fn as_of(&self) -> Instant {
        self.compiled.as_of()
    }

    /// 交付身份域已经编译的完整 MongoDB 授权阶段。仓储须在投影、分页或计数之前应用，不能只取其中一部分。
    pub fn stages(&self) -> &[Document] {
        self.compiled.stages()
    }
    /// 供应商仓储取数前核对固定资源、集合、行身份、对象粒度和原读取边界；模型错配、过期或更换执行器均拒
    /// 绝，不额外查库。
    pub fn ensure_current(&self, executor: &mut dyn Executor) -> IdentityResult<()> {
        supplier_model(self.compiled.model())?;
        self.compiled.ensure_current(executor)
    }

    /// 在查询结果交付前用原执行器请身份域复验账号、权限、事实版本和有效期；凭证失效或授权失败时不得返回
    /// 旧结果。
    pub async fn validate(&self, executor: &mut dyn Executor) -> erp_identity::Result<()> {
        self.authorization.validate(&self.compiled, executor).await
    }
}

// 完整供应商读取只接受本域固定资源、集合、行身份和对象粒度。
fn supplier_model(model: &QueryModel) -> IdentityResult<()> {
    if model.resource != "supplier"
        || model.collection != <Database as SupplierExt>::SUPPLIER_ACCOUNTS
        || model.id_field != "id"
        || model.output != QueryOutput::Object
    {
        return Err(IdentityError::ValidationError("供应商查询授权模型不匹配".into()));
    }
    Ok(())
}

struct Mapping {
    model: QueryModel,
}
#[async_trait]
impl AccessMapping for Mapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn key(&self, _: &AccessContext, _: Option<&MappingFacts>) -> erp_identity::Result<Bson> {
        Ok(Bson::String("$id".into()))
    }
    fn records(&self, _: &AccessContext, _: Option<&MappingFacts>) -> erp_identity::Result<QueryFragment> {
        Ok(QueryFragment::Match(doc! {"deleted_at":NOT_DELETED_TIMESTAMP_BSON}))
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> erp_identity::Result<QueryFragment> {
        match fact {
            AccessFact::Owned => Ok(QueryFragment::Match(doc! {"maintainer_user_id":context.actor.id()})),
            AccessFact::OrganizationIn(orgs) => Ok(QueryFragment::Match(
                doc! {"business_org_unit_id":{"$in":orgs.iter().collect::<Vec<_>>()}},
            )),
            _ => Err(erp_identity::Error::ValidationError("共享资料未登记该授权事实".into())),
        }
    }
    fn relation(
        &self,
        _: &RelationKey,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> erp_identity::Result<RelationMapping> {
        Err(erp_identity::Error::ValidationError("共享资料未登记来源关系".into()))
    }
}
struct Facts {
    db: Database,
    resource: String,
    collection: String,
}
#[async_trait]
impl AccessFactsPort for Facts {
    async fn load(
        &self,
        _: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<Vec<ObjectFacts>> {
        let rows = mongo_ops::find_many(
            &self.db.collection::<Document>(&self.collection),
            doc! {"id":{"$in":ids.ids().iter().cloned().collect::<Vec<_>>()},"deleted_at":NOT_DELETED_TIMESTAMP_BSON},
            mongodb::options::FindOptions::default(),
            executor,
        )
        .await?;

        rows.into_iter().map(|row| object(&self.resource, row)).collect()
    }
}

// 直接翻译真实维护人与业务组织；历史无值保持已知空，不以创建人回填。
fn object(resource: &str, row: Document) -> erp_identity::Result<ObjectFacts> {
    Ok(ObjectFacts {
        resource: resource.into(),
        id: row
            .get_str("id")
            .map_err(|_| erp_identity::Error::ValidationError("授权对象缺少主键".into()))?
            .into(),
        version: row
            .get("version")
            .ok_or_else(|| erp_identity::Error::ValidationError("授权对象缺少业务版本".into()))?
            .to_string(),
        owner_id: FactValue::Known(responsibility_text(&row, "maintainer_user_id")?),
        org_unit_id: FactValue::Known(responsibility_text(&row, "business_org_unit_id")?),
        ..Default::default()
    })
}
// 缺值与无有效值均表示已知无归属，损坏类型必须拒绝。
fn responsibility_text(row: &Document, field: &str) -> erp_identity::Result<Option<String>> {
    match row.get(field) {
        None | Some(Bson::Null) => Ok(None),
        Some(Bson::String(value)) => Ok((!value.trim().is_empty()).then(|| value.clone())),
        _ => Err(erp_identity::Error::ValidationError(format!("授权责任字段{field}类型无效"))),
    }
}

/// 为身份域装配供应商对象事实读取器和固定查询映射，令单对象校验和列表谓词都使用实际维护人和业务组织。
pub fn bindings(db: Database) -> erp_identity::Result<(Arc<dyn AccessFactsPort>, Arc<dyn AccessMapping>)> {
    let facts = Arc::new(Facts { db, resource: "supplier".into(), collection: "supplier_accounts".into() });
    let mapping = Arc::new(Mapping {
        model: QueryModel {
            key: QueryModelKey::parse("supplier")?,
            resource: "supplier".into(),
            collection: "supplier_accounts".into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: [FactKind::Owner, FactKind::Organization].into(),
        },
    });
    Ok((facts, mapping))
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_identity::entity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::testing::fixed_authorization;
    use erp_permissions::supplier;
    use persistence_core::ReadExecutor;

    use super::*;

    fn mapping() -> Mapping {
        Mapping {
            model: QueryModel {
                key: QueryModelKey::parse("supplier").unwrap(),
                resource: "supplier".into(),
                collection: "supplier_accounts".into(),
                id_field: "id".into(),
                output: QueryOutput::Object,
                facts: [FactKind::Owner, FactKind::Organization].into(),
            },
        }
    }

    #[test]
    fn supplier_model_requires_fixed_resource_collection_identity_and_object_output() {
        let model = mapping().model;
        supplier_model(&model).unwrap();

        for invalid in [
            QueryModel { resource: "warehouse".into(), ..model.clone() },
            QueryModel { collection: "other_suppliers".into(), ..model.clone() },
            QueryModel { id_field: "supplier_id".into(), ..model.clone() },
            QueryModel { output: QueryOutput::Share { fields: ["id".into()].into() }, ..model.clone() },
            QueryModel { output: QueryOutput::Existence { fields: ["id".into()].into() }, ..model.clone() },
        ] {
            assert!(matches!(supplier_model(&invalid), Err(IdentityError::ValidationError(_))));
        }
    }

    #[tokio::test]
    async fn compiled_supplier_scope_rejects_wrong_collection_and_stale_boundaries() {
        let mut executor = ReadExecutor::default();
        for wrong_collection in [false, true] {
            let mut mapping = mapping();
            if wrong_collection {
                mapping.model.collection = "other_suppliers".into();
            }

            let authorization = fixed_authorization(
                &[supplier::LIST],
                ResolvedScope {
                    role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                    user_limit: None,
                },
                vec![],
                vec![(supplier::LIST, Arc::new(mapping))],
            )
            .unwrap();
            let actor = AuditActor::new("user".into(), "login".into(), AccountKind::Admin);
            let compiled = authorization.query(&actor, &supplier::LIST, &mut executor).await.unwrap();
            let scope = CompiledScope {
                scope_version: compiled.version(),
                compiled: Arc::new(compiled),
                authorization,
            };

            if wrong_collection {
                assert!(matches!(
                    scope.ensure_current(&mut executor),
                    Err(IdentityError::ValidationError(_))
                ));
            } else {
                scope.ensure_current(&mut executor).unwrap();
                assert!(matches!(
                    scope.ensure_current(&mut ReadExecutor::default()),
                    Err(IdentityError::ValidationError(_))
                ));
                executor.next_read();
                assert!(matches!(
                    scope.ensure_current(&mut executor),
                    Err(IdentityError::ValidationError(_))
                ));
            }
        }
    }

    #[test]
    fn static_mapping_excludes_deleted_rows_and_rejects_unsupported_facts() {
        let mapping = mapping();
        let context = AccessContext {
            actor: AuditActor::new("a".into(), "A".into(), AccountKind::Admin),
            permission: supplier::LIST,
            resource: "supplier".into(),
            model: None,
            as_of: Instant::from_unix_secs(0),
        };
        assert!(mapping.preparation().is_none());
        assert_eq!(mapping.key(&context, None).unwrap(), Bson::String("$id".into()));
        assert!(matches!(mapping.records(&context, None).unwrap(), QueryFragment::Match(filter)
            if filter == doc! {"deleted_at": NOT_DELETED_TIMESTAMP_BSON}));

        assert!(mapping.predicate(&AccessFact::HistoryParticipant, &context, None).is_err());
        assert!(mapping.relation(&RelationKey::parse("source").unwrap(), &context, None).is_err());
    }
    // 验证真实维护人与组织在对象及查询两端一致；创建人不能补负责人，历史缺值保持已知空。
    #[test]
    fn real_responsibility_and_legacy_empty_values_match_object_and_query_checks() {
        use erp_identity::entity::access_control::{ScopeClause, ScopedObject};
        let mapping = mapping();
        for row in [
            doc! {"id":"row","version":1_i64,"maintainer_user_id":"owner","business_org_unit_id":"org","created_by":"creator"},
            doc! {"id":"row","version":1_i64,"created_by":"creator"},
            doc! {"id":"row","version":1_i64,"maintainer_user_id":"","business_org_unit_id":""},
            doc! {"id":"row","version":1_i64,"maintainer_user_id":null,"business_org_unit_id":null},
        ] {
            let facts = object("supplier", row.clone()).unwrap();
            let owner = facts.owner_id.get().expect("Owner必须已知").as_deref();
            let org = facts.org_unit_id.get().expect("Organization必须已知").as_deref();
            for actor in ["owner", "creator", "unrelated"] {
                let context = AccessContext {
                    actor: AuditActor::new(actor.into(), actor.into(), AccountKind::Admin),
                    permission: supplier::DETAIL,
                    resource: "supplier".into(),
                    model: None,
                    as_of: Instant::from_unix_secs(0),
                };
                let QueryFragment::Match(filter) =
                    mapping.predicate(&AccessFact::Owned, &context, None).unwrap()
                else {
                    panic!("负责人必须使用真实字段");
                };
                let query_owned =
                    row.get_str("maintainer_user_id").ok() == filter.get_str("maintainer_user_id").ok();
                let scoped = ScopedObject {
                    owned: owner == Some(actor),
                    collaborating: false,
                    historical_read_participant: false,
                    org_unit_id: org,
                    settlement_party_id: None,
                    warehouse_id: None,
                };
                assert_eq!(
                    ScopeClause { self_owned: true, ..Default::default() }.covers(&scoped),
                    query_owned
                );

                let organizations = ["org".into()].into();
                let QueryFragment::Match(filter) =
                    mapping.predicate(&AccessFact::OrganizationIn(organizations), &context, None).unwrap()
                else {
                    panic!("组织必须使用真实字段");
                };
                let accepted = filter.get_document("business_org_unit_id").unwrap().get_array("$in").unwrap();
                let query_org = row.get("business_org_unit_id").is_some_and(|value| accepted.contains(value));
                assert_eq!(
                    ScopeClause { org_unit_ids: ["org".into()].into(), ..Default::default() }.covers(&scoped),
                    query_org
                );
                assert!(ScopeClause { company: true, ..Default::default() }.covers(&scoped));
            }
        }

        let no_owner = object("supplier", doc! {"id":"row","version":1_i64,"created_by":"creator"}).unwrap();
        assert_eq!(no_owner.owner_id.get().unwrap(), &None);
        assert_eq!(no_owner.org_unit_id.get().unwrap(), &None);

        for field in ["maintainer_user_id", "business_org_unit_id"] {
            let mut malformed = doc! {"id":"row","version":1_i64};
            malformed.insert(field, 123);
            assert!(object("supplier", malformed).is_err());
        }
    }
}
