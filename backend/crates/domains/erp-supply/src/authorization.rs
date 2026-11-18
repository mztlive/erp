//! 本域授权登记和持久化事实；所有判定与条件均由身份域生成。
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::Instant;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, FactValue, MappingFacts, ObjectFacts, QueryModel, QueryModelKey,
    QueryOutput, RelationKey,
};
use erp_identity::ports::{AccessFactsPort, AccessMapping, QueryFragment, RelationMapping};
use erp_identity::{AccessTarget, AuthorizationService, CompiledAccess, Permission, ResourceTargets};
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::{Executor, mongo_ops};

/// 身份域编译的当前阶段凭证；不含可由业务组合的权限条款。
#[derive(Clone)]
pub struct CompiledScope {
    compiled: Arc<CompiledAccess>,
    authorization: Arc<AuthorizationService>,
    pub scope_version: String,
}
impl std::fmt::Debug for CompiledScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledScope").field("compiled", &self.compiled).finish()
    }
}
impl CompiledScope {
    // 仓储执行列表前核对固定动作、对象模型与原执行器。同资源的其他动作、跨阶段或已过期凭证均不能替用。
    pub(crate) fn ensure_permission(
        &self,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<()> {
        self.ensure_model(permission.resource(), executor)?;
        if self.compiled.permission() != permission {
            return Err(erp_identity::Error::ValidationError("供应查询授权动作不匹配".into()));
        }
        Ok(())
    }

    /// 交付身份域已经编译的完整 MongoDB 授权阶段。仓储须在投影、分页或计数之前应用，不能只取其中一部分。
    pub fn stages(&self) -> &[Document] {
        self.compiled.stages()
    }
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

    /// 在使用查询凭证前校验原执行器、读取阶段及有效期。此处不读取数据库；跨事务、跨阶段或到期均拒绝。
    pub fn ensure_current(&self, executor: &mut dyn Executor) -> erp_identity::Result<()> {
        self.compiled.ensure_current(executor)
    }

    /// 仓储使用凭证前核对当前阶段、固定资源、集合、模型键和完整对象输出粒度；不允许用其他资源或行级投影
    /// 的凭证替代。
    pub fn ensure_model(&self, resource: &str, executor: &mut dyn Executor) -> erp_identity::Result<()> {
        self.ensure_current(executor)?;
        let model = self.compiled.model();
        if model.resource != resource
            || model.collection != resource_spec(resource)?
            || model.key.as_str() != resource
            || model.output != QueryOutput::Object
        {
            return Err(erp_identity::Error::ValidationError("供应查询授权模型不匹配".into()));
        }
        Ok(())
    }

    /// 在查询结果交付前用原执行器请身份域复验账号、权限、事实版本和有效期；凭证失效或授权失败时不得返回
    /// 旧结果。
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
            AccessFact::Owned => {
                Ok(QueryFragment::Match(doc! { (owner_field(&self.model.resource)?): context.actor.id() }))
            },
            AccessFact::OrganizationIn(ids) => Ok(QueryFragment::Match(
                doc! { "business_org_unit_id": { "$in": ids.iter().collect::<Vec<_>>() } },
            )),
            _ => Err(erp_identity::Error::ValidationError("共享资料未登记该授权维度".into())),
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
        rows.iter().map(|row| object_facts(&self.resource, row)).collect()
    }
}

// 空的旧责任字段是已知无归属，禁止回退创建人或供应商结算主体。
// 非空身份按原值返回，空字段返回 None。
pub(crate) fn optional_identity(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_owned())
}

// 只从真实身份字段取值；缺失和空值保留为无归属，错误类型拒绝。
fn identity_field(row: &Document, field: &str) -> erp_identity::Result<Option<String>> {
    match row.get(field) {
        None | Some(Bson::Null) => Ok(None),
        Some(Bson::String(value)) => Ok(optional_identity(value)),
        _ => Err(erp_identity::Error::ValidationError("供应对象归属字段不是账号或组织身份".into())),
    }
}

// 锁定资源的责任字段由实际业务实体决定。
fn owner_field(resource: &str) -> erp_identity::Result<&'static str> {
    match resource {
        "supplier_offering" => Ok("maintainer_user_id"),
        "supplier_fulfillment_order" => Ok("follow_up_user_id"),
        "supplier_settlement_statement" => Ok("prepared_by"),
        _ => Err(erp_identity::Error::ValidationError("未登记本域归属字段".into())),
    }
}

// 继续保留实际根身份、版本与责任归属，不借用往来主体填充内部组织。
fn object_facts(resource: &str, row: &Document) -> erp_identity::Result<ObjectFacts> {
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
        owner_id: FactValue::Known(identity_field(row, owner_field(resource)?)?),
        org_unit_id: FactValue::Known(identity_field(row, "business_org_unit_id")?),
        ..Default::default()
    })
}

/// 固定资源目录及事实映射；每次调用独立读取当前授权。
#[derive(Clone)]
pub(crate) struct Authorization {
    pub(crate) service: Arc<AuthorizationService>,
    resource: String,
}

impl Authorization {
    // 为供给、履约或结算接入固定的本域资源目录及已注入的身份服务；未登记的资源拒绝，构造时不查询账号权限。
    pub(crate) fn new(service: Arc<AuthorizationService>, resource: &str) -> erp_identity::Result<Self> {
        resource_spec(resource)?;
        Ok(Self { service, resource: resource.into() })
    }
}

impl Authorization {
    // 调用共享身份域前先拒绝不属于本访问器资源的内置权限；只做接线校验，不读取账号或推导动作政策。
    pub(crate) fn ensure_permission(&self, permission: &Permission) -> erp_identity::Result<()> {
        if permission.resource() != self.resource {
            return Err(erp_identity::Error::ValidationError("供应权限资源与访问器不符".into()));
        }
        Ok(())
    }

    // 在调用方当前执行器内核对动作所属资源并向身份域编译查询凭证。账号、功能权限或模型校验没通过时不生成
    // 凭证。
    pub(crate) async fn resolve(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<CompiledScope> {
        self.ensure_permission(permission)?;
        let compiled = Arc::new(self.service.query(actor, permission, executor).await?);
        Ok(CompiledScope { scope_version: compiled.version(), compiled, authorization: self.service.clone() })
    }

    // 在读写对象的原执行器内，按固定资源动作和真实记录 ID 请求身份域判定；无权限或缺少对象事实时停止后续
    // 处理。
    pub(crate) async fn require(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<()> {
        self.ensure_permission(permission)?;
        self.service.require(actor, permission, AccessTarget::record(id)?, executor).await
    }
}

// 锁定资源目录；不从HTTP参数创建权限政策。
fn resource_spec(resource: &str) -> erp_identity::Result<&'static str> {
    match resource {
        "supplier_offering" => Ok("supplier_offerings"),
        "supplier_fulfillment_order" => Ok("supplier_fulfillment_orders"),
        "supplier_settlement_statement" => Ok("supplier_settlement_statements"),
        _ => Err(erp_identity::Error::ValidationError("未登记本域授权资源".into())),
    }
}

/// 提供共享供应资源的静态查询映射。
/// 固定集合与对象输出的 Mapping。
/// 未知资源拒绝。
pub fn mapping(resource: &str) -> erp_identity::Result<Arc<dyn AccessMapping>> {
    let collection = resource_spec(resource)?;
    Ok(Arc::new(Mapping {
        model: QueryModel {
            key: QueryModelKey::parse(resource)?,
            resource: resource.into(),
            collection: collection.into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: [FactKind::Owner, FactKind::Organization].into(),
        },
    }))
}

/// 提供本域持久化事实读取器。
/// 不持有请求状态的共享 Facts。
/// 未知资源拒绝。
pub fn facts(db: Database, resource: &str) -> erp_identity::Result<Arc<dyn AccessFactsPort>> {
    let collection = resource_spec(resource)?;
    if resource == "supplier_settlement_statement" {
        return Ok(super::service::supplier_settlement::authorization::facts(db));
    }
    Ok(Arc::new(Facts { db, resource: resource.into(), collection: collection.into() }))
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::entity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::testing::{fixed_authorization, unavailable_authorization};
    use erp_permissions::{supplier_fulfillment, supplier_offering, supplier_settlement};
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    // 验证模型构造拒绝未知资源，不从展示目录决定接入能力。
    #[test]
    fn unknown_supplier_resource_is_rejected() {
        assert!(matches!(mapping("unknown"), Err(erp_identity::Error::ValidationError(_))));
    }

    // 验证直接常量接入仍拒绝其他资源，业务展示目录不参与验证。
    #[test]
    fn permission_resource_must_match_the_owned_accessor() {
        let authorization = Authorization::new(unavailable_authorization(), "supplier_offering").unwrap();
        assert!(authorization.ensure_permission(&supplier_offering::REVISE).is_ok());
        assert!(matches!(
            authorization.ensure_permission(&supplier_fulfillment::ORDER_CANCEL),
            Err(erp_identity::Error::ValidationError(_))
        ));
    }

    #[tokio::test]
    async fn settlement_list_scope_rejects_other_action_and_other_execution_boundary() {
        let permissions = [supplier_settlement::STATEMENT_LIST, supplier_settlement::STATEMENT_DETAIL];
        let service = fixed_authorization(
            &permissions,
            ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                user_limit: None,
            },
            Vec::new(),
            permissions
                .iter()
                .cloned()
                .map(|permission| (permission, mapping("supplier_settlement_statement").unwrap()))
                .collect(),
        )
        .unwrap();
        let authorization = Authorization::new(service, "supplier_settlement_statement").unwrap();
        let actor = AuditActor::new("responsible".into(), "login".into(), AccountKind::Admin);
        let mut executor = SnapshotExecutor::default();

        let list = authorization.resolve(&actor, &permissions[0], &mut executor).await.unwrap();
        assert!(list.ensure_permission(&permissions[0], &mut executor).is_ok());
        let detail = authorization.resolve(&actor, &permissions[1], &mut executor).await.unwrap();
        assert!(matches!(
            detail.ensure_permission(&permissions[0], &mut executor),
            Err(erp_identity::Error::ValidationError(_))
        ));

        executor.restart();
        assert!(list.ensure_permission(&permissions[0], &mut executor).is_err());
    }

    // 验证对象事实和查询谓词使用同一真实责任字段，不能借创建人或供应商授予范围。
    #[test]
    fn supplier_roots_map_actual_owner_and_internal_organization() {
        for (resource, owner, permission) in [
            ("supplier_offering", "maintainer_user_id", supplier_offering::LIST),
            ("supplier_fulfillment_order", "follow_up_user_id", supplier_fulfillment::ORDER_LIST),
            ("supplier_settlement_statement", "prepared_by", supplier_settlement::STATEMENT_LIST),
        ] {
            let context = AccessContext {
                actor: AuditActor::new("responsible".into(), "login".into(), AccountKind::Admin),
                permission,
                resource: resource.into(),
                model: None,
                as_of: Instant::from_unix_secs(10),
            };
            let row = doc! {
                "id": "record", "version": 7_i64, (owner): "responsible",
                "business_org_unit_id": "internal_org", "created_by": "creator", "supplier_id": "supplier"
            };
            let object = object_facts(resource, &row).unwrap();
            assert_eq!(object.id, "record");
            assert_eq!(object.version, "7");
            assert_eq!(object.owner_id, FactValue::Known(Some("responsible".into())));
            assert_eq!(object.org_unit_id, FactValue::Known(Some("internal_org".into())));

            let mapping = mapping(resource).unwrap();
            for (fact, kind) in [
                (AccessFact::Owned, FactKind::Owner),
                (AccessFact::OrganizationIn(["internal_org".into()].into()), FactKind::Organization),
            ] {
                assert!(mapping.model().facts.contains(&kind));
                let expected = if fact == AccessFact::Owned {
                    doc! { (owner): "responsible" }
                } else {
                    doc! { "business_org_unit_id": { "$in": ["internal_org"] } }
                };
                assert!(
                    matches!(mapping.predicate(&fact, &context, None).unwrap(), QueryFragment::Match(actual) if actual == expected)
                );
            }
        }
    }

    // 验证旧对象无责任归属时保留已知空值，不用创建人或往来主体填补；错误类型拒绝。
    #[test]
    fn absent_responsibility_is_known_empty_and_malformed_identity_is_rejected() {
        for resource in ["supplier_offering", "supplier_fulfillment_order", "supplier_settlement_statement"] {
            let mut row =
                doc! { "id": "record", "version": 1_i64, "created_by": "creator", "supplier_id": "supplier" };
            let empty = object_facts(resource, &row).unwrap();
            assert_eq!(empty.owner_id, FactValue::Known(None));
            assert_eq!(empty.org_unit_id, FactValue::Known(None));
            row.insert(owner_field(resource).unwrap(), "  ");
            row.insert("business_org_unit_id", Bson::Null);
            let blank = object_facts(resource, &row).unwrap();
            assert_eq!(blank.owner_id, empty.owner_id);
            assert_eq!(blank.org_unit_id, empty.org_unit_id);
            row.insert(owner_field(resource).unwrap(), 42_i64);
            assert!(matches!(object_facts(resource, &row), Err(erp_identity::Error::ValidationError(_))));
        }
    }
}
