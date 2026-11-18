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
    /// 核对固定列表动作及原执行器，禁止挪用同资源的其他动作凭证。
    /// # 参数
    /// permission 为入口固定动作，executor 为原授权执行器。
    /// # 返回
    /// 模型、动作及执行边界一致时成功。
    /// # 错误
    /// 动作、资源、模型或执行边界不一致时拒绝。
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

    /// 返回身份域已编译的完整授权阶段。
    /// # 参数
    /// 无。
    /// # 返回
    /// 必须先于分页、计数与汇总执行的全部 MongoDB 阶段。
    /// # 错误
    /// 不执行 I/O。
    pub fn stages(&self) -> &[Document] {
        self.compiled.stages()
    }
    /// 返回当前凭证的权限策略版本。
    /// # 参数
    /// 无。
    /// # 返回
    /// identity 绑定的策略版本。
    /// # 错误
    /// 无。
    pub fn policy_version(&self) -> u64 {
        self.compiled.policy_version()
    }

    /// 返回当前凭证的组织版本。
    /// # 参数
    /// 无。
    /// # 返回
    /// identity 绑定的组织版本。
    /// # 错误
    /// 无。
    pub fn organization_version(&self) -> u64 {
        self.compiled.organization_version()
    }

    /// 返回当前凭证的授权时点。
    /// # 参数
    /// 无。
    /// # 返回
    /// identity 冻结的授权时点。
    /// # 错误
    /// 无。
    pub fn as_of(&self) -> Instant {
        self.compiled.as_of()
    }

    /// 核对仓储使用边界与有效期，不执行数据库读取。
    /// # 参数
    /// executor 为原读取执行器。
    /// # 返回
    /// 原阶段仍有效时成功。
    /// # 错误
    /// 跨事务、跨阶段或到期时拒绝。
    pub fn ensure_current(&self, executor: &mut dyn Executor) -> erp_identity::Result<()> {
        self.compiled.ensure_current(executor)
    }

    /// 核对仓储模型及当前读取边界。
    /// # 参数
    /// resource 为拥有领域的固定资源，executor 为原读取执行器。
    /// # 返回
    /// 模型、完整对象输出及阶段均匹配时成功。
    /// # 错误
    /// 其他资源凭证、输出粒度错配或阶段失效时拒绝。
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

    /// 复验账号、权限、版本和有效期。
    /// # 参数
    /// executor 必须为原执行器。
    /// # 返回
    /// 凭证仍有效时成功。
    /// # 错误
    /// 未接线、撤权或版本变化拒绝。
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

/// 空的旧责任字段是已知无归属，禁止回退创建人或供应商结算主体。
/// # 参数
/// value 为持久化责任人或业务组织字段。
/// # 返回
/// 非空身份按原值返回，空字段返回 None。
/// # 错误
/// 不返回错误。
pub(crate) fn optional_identity(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_owned())
}

/// 只从真实身份字段取值；缺失和空值保留为无归属，错误类型拒绝。
fn identity_field(row: &Document, field: &str) -> erp_identity::Result<Option<String>> {
    match row.get(field) {
        None | Some(Bson::Null) => Ok(None),
        Some(Bson::String(value)) => Ok(optional_identity(value)),
        _ => Err(erp_identity::Error::ValidationError("供应对象归属字段不是账号或组织身份".into())),
    }
}

/// 固定资源的责任字段由实际业务实体决定。
fn owner_field(resource: &str) -> erp_identity::Result<&'static str> {
    match resource {
        "supplier_offering" => Ok("maintainer_user_id"),
        "supplier_fulfillment_order" => Ok("follow_up_user_id"),
        "supplier_settlement_statement" => Ok("prepared_by"),
        _ => Err(erp_identity::Error::ValidationError("未登记本域归属字段".into())),
    }
}

/// 保留实际根身份、版本与责任归属，不借用往来主体填充内部组织。
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
    /// 构造固定资源的服务及映射，不读取账号权限。
    /// # 参数
    /// db、rbac 为共享设施，resource 为本域固定资源。
    /// # 返回
    /// 可复用的固定授权目录。
    /// # 错误
    /// 固定目录或模型无效时返回接入错误。
    pub(crate) fn new(service: Arc<AuthorizationService>, resource: &str) -> erp_identity::Result<Self> {
        resource_spec(resource)?;
        Ok(Self { service, resource: resource.into() })
    }
}

impl Authorization {
    /// 只校验资源接线，不声明动作政策或从展示目录解析权限。
    /// # 参数
    /// permission 为业务入口直接引用的内置权限。
    /// # 返回
    /// 权限资源与当前访问器一致时成功。
    /// # 错误
    /// 跨资源传入权限时拒绝，不读取授权设施。
    pub(crate) fn ensure_permission(&self, permission: &Permission) -> erp_identity::Result<()> {
        if permission.resource() != self.resource {
            return Err(erp_identity::Error::ValidationError("供应权限资源与访问器不符".into()));
        }
        Ok(())
    }

    /// 编译当前资源动作的完整共享资料授权。
    /// # 参数
    /// actor 为当前账号，permission 为内置权限，executor 为原执行器。
    /// # 返回
    /// 本阶段不可伪造的编译凭证。
    /// # 错误
    /// 账号、功能权限、登记、编译或版本校验失败返回错误。
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

    /// 在原事务内检查完整对象访问。
    /// # 参数
    /// actor 为当前账号，permission/id 为实际操作对象，executor 为原执行器。
    /// # 返回
    /// 身份域允许对象操作时成功。
    /// # 错误
    /// 缺失对象、功能权限或数据授权拒绝。
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

/// 固定资源目录；不从HTTP参数创建权限政策。
fn resource_spec(resource: &str) -> erp_identity::Result<&'static str> {
    match resource {
        "supplier_offering" => Ok("supplier_offerings"),
        "supplier_fulfillment_order" => Ok("supplier_fulfillment_orders"),
        "supplier_settlement_statement" => Ok("supplier_settlement_statements"),
        _ => Err(erp_identity::Error::ValidationError("未登记本域授权资源".into())),
    }
}

/// 提供共享供应资源的静态查询映射。
/// # 参数
/// resource 为本域资源标识。
/// # 返回
/// 固定集合与对象输出的 Mapping。
/// # 错误
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
/// # 参数
/// db 为业务数据库，resource 为本域资源。
/// # 返回
/// 不持有请求状态的共享 Facts。
/// # 错误
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
    /// 模型构造拒绝未知资源，不从展示目录决定接入能力。
    #[test]
    fn unknown_supplier_resource_is_rejected() {
        assert!(matches!(mapping("unknown"), Err(erp_identity::Error::ValidationError(_))));
    }

    /// 直接常量接入仍拒绝其他资源，业务展示目录不参与验证。
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

    /// 对象事实和查询谓词使用同一真实责任字段，不能借创建人或供应商授予范围。
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

    /// 旧对象无责任归属时保留已知空值，不用创建人或往来主体填补；错误类型拒绝。
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
