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
    compiled: Arc<CompiledAccess>,
    authorization: AuthorizationService,
    filter: Document,
    pub scope_version: String,
    pub policy_version: u64,
    pub organization_version: u64,
    pub as_of: Instant,
}
impl std::fmt::Debug for CompiledScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledScope").field("compiled", &self.compiled).finish()
    }
}
impl CompiledScope {
    /// 返回身份域已编译的完整授权条件。
    /// # 参数
    /// 无。
    /// # 返回
    /// 完整 MongoDB 条件。
    /// # 错误
    /// 不执行 I/O。
    pub fn document(&self) -> Document {
        self.filter.clone()
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

/// 固定资源目录及事实映射；每次调用独立读取当前授权。
#[derive(Clone)]
pub(crate) struct Authorization {
    service: AuthorizationService,
    mapping: Arc<dyn AccessMapping>,
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
    pub(crate) fn new(db: &Database, rbac: SharedRbacService, resource: &str) -> erp_identity::Result<Self> {
        let (collection, actions): (&str, &[&str]) = match resource {
            "product" => ("products", &["list", "detail", "create", "update"]),
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
        Ok(Self {
            service: AuthorizationService::new(db.clone(), rbac, AuthorizationRegistry::new(entries)?),
            mapping,
            resource: resource.into(),
        })
    }
}

impl Authorization {
    /// 编译当前资源动作的完整共享资料授权。
    /// # 参数
    /// actor 为当前账号，action 为固定动作，executor 为原执行器。
    /// # 返回
    /// 本阶段不可伪造的编译凭证。
    /// # 错误
    /// 账号、功能权限、登记、编译或版本校验失败返回错误。
    pub(crate) async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<CompiledScope> {
        let permission = Permission::parse(format!("{}:{action}", self.resource))?;
        let query = self.service.query(actor, &permission, AccessSubject::Direct, executor).await?;
        let compiled = Arc::new(query.compile(self.mapping.as_ref(), executor).await?);
        self.service.validate(&compiled, executor).await?;
        let [stage] = compiled.stages() else {
            return Err(erp_identity::Error::ValidationError("共享资料查询产生未支持的关联阶段".into()));
        };
        let filter = stage
            .get_document("$match")
            .map_err(|_| erp_identity::Error::ValidationError("共享资料查询缺少授权条件".into()))?
            .clone();
        Ok(CompiledScope {
            scope_version: compiled.version(),
            policy_version: compiled.policy_version(),
            organization_version: compiled.organization_version(),
            as_of: compiled.as_of(),
            compiled,
            authorization: self.service.clone(),
            filter,
        })
    }

    /// 在原事务内检查完整对象访问。
    /// # 参数
    /// actor 为当前账号，action/id 为实际操作对象，executor 为原执行器。
    /// # 返回
    /// 身份域允许对象操作时成功。
    /// # 错误
    /// 缺失对象、功能权限或数据授权拒绝。
    pub(crate) async fn require(
        &self,
        actor: &AuditActor,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<()> {
        self.service
            .require(
                actor,
                &Permission::parse(format!("{}:{action}", self.resource))?,
                AccessTarget::Records { subject: AccessSubject::Direct, ids: ResourceTargets::new(&[id])? },
                executor,
            )
            .await
    }

    /// 使用固定目录的身份设施读取当前功能资格。
    /// # 参数
    /// actor 为当前账号，permission 为具体功能权限，executor 为原执行器。
    /// # 返回
    /// 当前账号具有功能资格时为真。
    /// # 错误
    /// 身份、策略或数据库读取失败时返回原错误。
    pub(crate) async fn has_permission(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<bool> {
        self.service.has_permission(actor, permission, executor).await
    }
}
