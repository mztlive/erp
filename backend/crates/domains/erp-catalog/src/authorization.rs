//! 本域授权登记和持久化事实；所有判定与条件均由身份域生成。
use std::sync::Arc;

use async_trait::async_trait;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::Instant;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, FactValue, ObjectFacts, QueryModel, QueryModelKey, QueryOutput,
    RelationKey,
};
use erp_identity::ports::{AccessFactsPort, AccessMapping, QueryFragment, RelationMapping};
use erp_identity::{AuthorizationService, CompiledAccess, Error as IdentityError, ResourceTargets};
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::{Executor, mongo_ops};

use crate::repository::CatalogExt;

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
    /// 读取当前凭证的策略版本。identity 绑定的策略版本。
    pub fn policy_version(&self) -> u64 {
        self.compiled.policy_version()
    }

    /// 读取当前凭证的组织版本。identity 绑定的组织版本。
    pub fn organization_version(&self) -> u64 {
        self.compiled.organization_version()
    }

    /// 读取本次授权时点。identity 绑定的授权时点。
    pub fn as_of(&self) -> Instant {
        self.compiled.as_of()
    }

    /// 返回身份域编译的完整授权阶段。
    ///
    /// 必须在分页、计数与投影前完整执行的聚合阶段。不执行 I/O。
    pub fn stages(&self) -> &[Document] {
        self.compiled.stages()
    }
    /// 核验当前事务、读取阶段及有效期，不读取数据库。
    ///
    /// 凭证仍属于当前阶段时成功。跨事务、跨阶段或已经到期时拒绝。
    ///
    /// executor 为原读取执行器。
    pub fn ensure_current(&self, executor: &mut dyn Executor) -> erp_identity::Result<()> {
        Self::ensure_model(self.compiled.model())?;
        self.compiled.ensure_current(executor)
    }

    // 商品仓储只接受真实商品集合的完整对象凭证。
    fn ensure_model(model: &QueryModel) -> erp_identity::Result<()> {
        if model.resource != "product"
            || model.collection != <Database as CatalogExt>::PRODUCTS
            || model.id_field != "id"
            || model.output != QueryOutput::Object
        {
            return Err(IdentityError::ValidationError("商品查询凭证的资源、集合或输出不符".into()));
        }
        Ok(())
    }

    /// 复验账号、权限、版本和有效期。
    ///
    /// 凭证仍有效时成功。未接线、撤权或版本变化拒绝。
    ///
    /// executor 必须为原执行器。
    pub async fn validate(&self, executor: &mut dyn Executor) -> erp_identity::Result<()> {
        self.authorization.validate(&self.compiled, executor).await
    }
}

struct Mapping {
    model: QueryModel,
}
#[async_trait]
impl AccessMapping for Mapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn key(&self, _: &AccessContext) -> erp_identity::Result<Bson> {
        Ok(Bson::String("$id".into()))
    }
    fn records(&self, _: &AccessContext) -> erp_identity::Result<QueryFragment> {
        Ok(QueryFragment::Match(doc! {"deleted_at":NOT_DELETED_TIMESTAMP_BSON}))
    }
    fn predicate(&self, fact: &AccessFact, context: &AccessContext) -> erp_identity::Result<QueryFragment> {
        match fact {
            AccessFact::Owned => Ok(QueryFragment::Match(doc! {"maintainer_user_id":context.actor.id()})),
            AccessFact::OrganizationIn(orgs) => Ok(QueryFragment::Match(
                doc! {"business_org_unit_id":{"$in":orgs.iter().collect::<Vec<_>>()}},
            )),
            _ => Err(IdentityError::ValidationError("共享资料未登记该授权事实".into())),
        }
    }
    fn relation(&self, _: &RelationKey, _: &AccessContext) -> erp_identity::Result<RelationMapping> {
        Err(IdentityError::ValidationError("共享资料未登记来源关系".into()))
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
        id: row.get_str("id").map_err(|_| IdentityError::ValidationError("授权对象缺少主键".into()))?.into(),
        version: row
            .get("version")
            .ok_or_else(|| IdentityError::ValidationError("授权对象缺少业务版本".into()))?
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
        _ => Err(IdentityError::ValidationError(format!("授权责任字段{field}类型无效"))),
    }
}

/// 构造真实对象事实与静态查询映射，供统一授权装配复用。
///
/// 不包含授权政策的事实读取器与映射。固定查询模型非法时返回错误，构造不执行 I/O。
pub fn bindings(db: Database) -> erp_identity::Result<(Arc<dyn AccessFactsPort>, Arc<dyn AccessMapping>)> {
    let facts = Arc::new(Facts { db, resource: "product".into(), collection: "products".into() });
    let mapping = Arc::new(Mapping {
        model: QueryModel {
            key: QueryModelKey::parse("product")?,
            resource: "product".into(),
            collection: "products".into(),
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
    use erp_permissions::catalog;

    use super::*;

    fn mapping() -> Mapping {
        Mapping {
            model: QueryModel {
                key: QueryModelKey::parse("product").unwrap(),
                resource: "product".into(),
                collection: "products".into(),
                id_field: "id".into(),
                output: QueryOutput::Object,
                facts: [FactKind::Owner, FactKind::Organization].into(),
            },
        }
    }

    #[test]
    fn product_repository_rejects_other_resources_collections_and_restricted_outputs() {
        let model = mapping().model;
        assert!(CompiledScope::ensure_model(&model).is_ok());

        for (resource, collection, id_field, output) in [
            ("customer", "products", "id", QueryOutput::Object),
            ("product", "customers", "id", QueryOutput::Object),
            ("product", "products", "source_id", QueryOutput::Object),
            ("product", "products", "id", QueryOutput::Share { fields: ["id".into()].into() }),
            ("product", "products", "id", QueryOutput::Existence { fields: ["id".into()].into() }),
        ] {
            let invalid = QueryModel {
                resource: resource.into(),
                collection: collection.into(),
                id_field: id_field.into(),
                output,
                ..model.clone()
            };
            assert!(matches!(CompiledScope::ensure_model(&invalid), Err(IdentityError::ValidationError(_))));
        }
    }

    #[test]
    fn static_mapping_excludes_deleted_rows_and_rejects_unsupported_facts() {
        let mapping = mapping();
        let context = AccessContext {
            actor: AuditActor::new("a".into(), "A".into(), AccountKind::Admin),
            permission: catalog::PRODUCT_PRODUCT_LIST,
            resource: "product".into(),
            model: None,
            as_of: Instant::from_unix_secs(0),
        };

        assert_eq!(mapping.key(&context).unwrap(), Bson::String("$id".into()));
        assert!(matches!(mapping.records(&context).unwrap(), QueryFragment::Match(filter)
            if filter == doc! {"deleted_at": NOT_DELETED_TIMESTAMP_BSON}));

        assert!(mapping.predicate(&AccessFact::HistoryParticipant, &context).is_err());
        assert!(mapping.relation(&RelationKey::parse("source").unwrap(), &context).is_err());
    }
    // 真实维护人与组织在对象及查询两端一致；创建人不能补负责人，历史缺值保持已知空。
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
            let facts = object("product", row.clone()).unwrap();
            let owner = facts.owner_id.get().expect("Owner必须已知").as_deref();
            let org = facts.org_unit_id.get().expect("Organization必须已知").as_deref();
            for actor in ["owner", "creator", "unrelated"] {
                let context = AccessContext {
                    actor: AuditActor::new(actor.into(), actor.into(), AccountKind::Admin),
                    permission: catalog::PRODUCT_PRODUCT_DETAIL,
                    resource: "product".into(),
                    model: None,
                    as_of: Instant::from_unix_secs(0),
                };
                let QueryFragment::Match(filter) = mapping.predicate(&AccessFact::Owned, &context).unwrap()
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
                    mapping.predicate(&AccessFact::OrganizationIn(organizations), &context).unwrap()
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

        let no_owner = object("product", doc! {"id":"row","version":1_i64,"created_by":"creator"}).unwrap();
        assert_eq!(no_owner.owner_id.get().unwrap(), &None);
        assert_eq!(no_owner.org_unit_id.get().unwrap(), &None);

        for field in ["maintainer_user_id", "business_org_unit_id"] {
            let mut malformed = doc! {"id":"row","version":1_i64};
            malformed.insert(field, 123);
            assert!(object("product", malformed).is_err());
        }
    }
}
