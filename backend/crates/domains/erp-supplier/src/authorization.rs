//! 本域授权登记和持久化事实；所有判定与条件均由身份域生成。
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::Instant;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, MappingFacts, ObjectFacts, QueryModel, QueryModelKey, QueryOutput, ReadAccess,
    RelationKey,
};
use erp_identity::ports::{AccessFactsPort, AccessMapping, QueryFragment, RelationMapping};
use erp_identity::{
    AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AccessTarget, AuthorizationRegistry,
    AuthorizationService, CompiledAccess, DataAccess, Permission, ResourceTargets, SharedRbacService,
};
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::{Executor, mongo_ops};

/// 身份域编译的当前阶段凭证；不含可由业务组合的权限条款。
#[derive(Clone)]
pub struct CompiledScope {
    compiled: Option<Arc<CompiledAccess>>,
    authorization: Option<AuthorizationService>,
    filter: Document,
    pub user_id: String,
    pub resource: String,
    pub action: String,
    pub scope_version: String,
    pub policy_version: u64,
    pub organization_version: u64,
    pub as_of: Instant,
}
impl Default for CompiledScope {
    fn default() -> Self {
        Self {
            compiled: None,
            authorization: None,
            filter: Document::new(),
            user_id: String::new(),
            resource: String::new(),
            action: String::new(),
            scope_version: String::new(),
            policy_version: 0,
            organization_version: 0,
            as_of: Instant::from_unix_secs(0),
        }
    }
}
impl std::fmt::Debug for CompiledScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledScope").field("compiled", &self.compiled).finish()
    }
}
impl CompiledScope {
    /// 返回身份域已编译的条件，未接线凭证保持失败关闭。
    /// # 参数
    /// 无。
    /// # 返回
    /// 完整 MongoDB 条件。
    /// # 错误
    /// 不执行 I/O。
    pub fn document(&self) -> Document {
        if self.compiled.is_some() {
            self.filter.clone()
        } else {
            doc! {"$expr":false}
        }
    }
    /// 复验账号、权限、版本和有效期。
    /// # 参数
    /// executor 必须为原执行器。
    /// # 返回
    /// 凭证仍有效时成功。
    /// # 错误
    /// 未接线、撤权或版本变化拒绝。
    pub async fn validate(&self, executor: &mut dyn Executor) -> erp_identity::Result<()> {
        let service = self
            .authorization
            .as_ref()
            .ok_or_else(|| erp_identity::Error::Forbidden("授权凭证未接线".into()))?;
        service.validate(self.compiled.as_ref().expect("服务和编译凭证同时构造"), executor).await
    }
    /// 判断是否已取得身份域查询凭证。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前阶段已经授权时为真。
    /// # 错误
    /// 不执行 I/O。
    pub fn has_scope_rules(&self) -> bool {
        self.compiled.is_some()
    }
    /// 判断是否缺少身份域查询凭证。
    /// # 参数
    /// 无。
    /// # 返回
    /// 未装配时为真。
    /// # 错误
    /// 不执行 I/O。
    pub fn is_empty(&self) -> bool {
        self.compiled.is_none()
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
    fn key(&self, _: &AccessContext, _: &MappingFacts) -> erp_identity::Result<Bson> {
        Ok(Bson::String("$id".into()))
    }
    async fn prepare(&self, _: &AccessContext, _: &mut dyn Executor) -> erp_identity::Result<MappingFacts> {
        Ok(MappingFacts { version: Some("company_shared_v1".into()), valid_until: None })
    }
    fn records(&self, _: &AccessContext, _: &MappingFacts) -> erp_identity::Result<QueryFragment> {
        Ok(QueryFragment::Match(doc! {"deleted_at":NOT_DELETED_TIMESTAMP_BSON}))
    }
    fn predicate(
        &self,
        _: &AccessFact,
        _: &AccessContext,
        _: &MappingFacts,
    ) -> erp_identity::Result<QueryFragment> {
        Err(erp_identity::Error::ValidationError("共享资料未登记人员范围映射".into()))
    }
    fn relation(
        &self,
        _: &RelationKey,
        _: &AccessContext,
        _: &MappingFacts,
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
        rows.into_iter()
            .map(|row| {
                Ok(ObjectFacts {
                    resource: self.resource.clone(),
                    id: row
                        .get_str("id")
                        .map_err(|_| erp_identity::Error::ValidationError("授权对象缺少主键".into()))?
                        .into(),
                    version: row
                        .get("version")
                        .ok_or_else(|| erp_identity::Error::ValidationError("授权对象缺少业务版本".into()))?
                        .to_string(),
                    ..Default::default()
                })
            })
            .collect()
    }
}

fn setup(
    db: &Database,
    rbac: SharedRbacService,
    resource: &str,
) -> erp_identity::Result<(AuthorizationService, Arc<dyn AccessMapping>)> {
    let (collection, actions): (&str, &[&str]) = match resource {
        "supplier" => ("supplier_accounts", &["list", "detail", "create", "update", "delete"]),
        _ => return Err(erp_identity::Error::ValidationError("未登记本域授权资源".into())),
    };
    let mapping: Arc<dyn AccessMapping> = Arc::new(Mapping {
        model: QueryModel {
            key: QueryModelKey::parse(resource)?,
            resource: resource.into(),
            collection: collection.into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: Default::default(),
        },
    });
    let facts: Arc<dyn AccessFactsPort> =
        Arc::new(Facts { db: db.clone(), resource: resource.into(), collection: collection.into() });
    let mut entries = Vec::new();
    for action in actions {
        let permission = Permission::parse(format!("{resource}:{action}"))?;
        entries.push(AccessRegistration {
            permission: permission.clone(),
            required_permissions: vec![],
            access: AccessMode::QueryAndObject {
                data: DataAccess {
                    subject: AccessSubject::Direct,
                    required_facts: Default::default(),
                    policy: AccessPolicy::Scope { permission, read: ReadAccess::default() },
                },
                mapping: mapping.clone(),
                facts: facts.clone(),
            },
        });
    }
    Ok((AuthorizationService::new(db.clone(), rbac, AuthorizationRegistry::new(entries)?), mapping))
}

/// 编译当前资源动作的完整共享资料授权。
/// # 参数
/// db、rbac 为身份设施；resource/action 为服务端固定入口；executor 为原执行器。
/// # 返回
/// 本阶段不可伪造的编译凭证。
/// # 错误
/// 账号、功能权限、登记、编译或版本校验失败返回错误。
pub async fn resolve(
    db: &Database,
    rbac: SharedRbacService,
    actor: &AuditActor,
    resource: &str,
    action: &str,
    executor: &mut dyn Executor,
) -> erp_identity::Result<CompiledScope> {
    let policy_version = rbac.policy_revision_with_executor(executor).await?;
    let (service, mapping) = setup(db, rbac, resource)?;
    let permission = Permission::parse(format!("{resource}:{action}"))?;
    let query = service.query(actor, &permission, AccessSubject::Direct, executor).await?;
    let compiled = Arc::new(query.compile(mapping.as_ref(), executor).await?);
    service.validate(&compiled, executor).await?;
    let [stage] = compiled.stages() else {
        return Err(erp_identity::Error::ValidationError("共享资料查询产生未支持的关联阶段".into()));
    };
    let filter = stage
        .get_document("$match")
        .map_err(|_| erp_identity::Error::ValidationError("共享资料查询缺少授权条件".into()))?
        .clone();
    Ok(CompiledScope {
        scope_version: compiled.version(),
        compiled: Some(compiled),
        authorization: Some(service),
        filter,
        user_id: actor.id().into(),
        resource: resource.into(),
        action: action.into(),
        policy_version,
        organization_version: erp_identity::repository::OrganizationRepository::new(db)
            .revision(executor)
            .await?
            .map_or(0, |revision| revision.revision),
        as_of: Instant::now(),
    })
}

/// 在原事务内检查完整对象访问。
/// # 参数
/// db、rbac、actor 为可信身份设施；id 为实际对象，executor 为业务执行器。
/// # 返回
/// 身份域允许对象操作时成功。
/// # 错误
/// 缺失对象、功能权限或数据授权拒绝。
pub async fn require(
    db: &Database,
    rbac: SharedRbacService,
    actor: &AuditActor,
    resource: &str,
    action: &str,
    id: &str,
    executor: &mut dyn Executor,
) -> erp_identity::Result<()> {
    let (service, _) = setup(db, rbac, resource)?;
    service
        .require(
            actor,
            &Permission::parse(format!("{resource}:{action}"))?,
            AccessTarget::Records { subject: AccessSubject::Direct, ids: ResourceTargets::new(&[id])? },
            executor,
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn missing_identity_proof_rejects_execution() {
        let scope = CompiledScope::default();
        assert_eq!(scope.document(), doc! {"$expr":false});
        assert!(matches!(
            scope.validate(&mut persistence_core::NoTransaction).await,
            Err(erp_identity::Error::Forbidden(_))
        ));
    }
}
