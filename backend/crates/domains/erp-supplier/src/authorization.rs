//! 本域授权登记和持久化事实；所有判定与条件均由身份域生成。
use std::sync::Arc;

use async_trait::async_trait;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::Instant;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, MappingFacts, ObjectFacts, QueryModel, QueryModelKey, QueryOutput, ReadAccess,
    RelationKey,
};
use erp_identity::ports::{AccessFactsPort, AccessMapping, QueryFragment, RelationMapping};
use erp_identity::{
    AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AuthorizationRegistry, AuthorizationService,
    CompiledAccess, DataAccess, Permission, ResourceTargets,
};
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::{Executor, mongo_ops};

/// 身份域编译的当前阶段凭证；不含可由业务组合的权限条款。
#[derive(Clone)]
pub struct CompiledScope {
    pub(crate) compiled: Arc<CompiledAccess>,
    pub(crate) authorization: AuthorizationService,
    pub scope_version: String,
}
impl std::fmt::Debug for CompiledScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledScope").field("compiled", &self.compiled).finish()
    }
}
impl CompiledScope {
    /// 读取当前凭证的策略版本。
    /// # 参数
    /// 无。
    /// # 返回
    /// identity 绑定的策略版本。
    /// # 错误
    /// 无。
    pub fn policy_version(&self) -> u64 {
        self.compiled.policy_version()
    }

    /// 读取当前凭证的组织版本。
    /// # 参数
    /// 无。
    /// # 返回
    /// identity 绑定的组织版本。
    /// # 错误
    /// 无。
    pub fn organization_version(&self) -> u64 {
        self.compiled.organization_version()
    }

    /// 读取本次授权时点。
    /// # 参数
    /// 无。
    /// # 返回
    /// identity 绑定的授权时点。
    /// # 错误
    /// 无。
    pub fn as_of(&self) -> Instant {
        self.compiled.as_of()
    }

    /// 返回身份域编译的完整授权阶段。
    /// # 参数
    /// 无。
    /// # 返回
    /// 必须在分页、计数与投影前完整执行的聚合阶段。
    /// # 错误
    /// 不执行 I/O。
    pub fn stages(&self) -> &[Document] {
        self.compiled.stages()
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

/// 构造供应商固定授权目录和共享资料映射。
/// # 参数
/// db 为供应商集合所在数据库。
/// # 返回
/// 供 identity 执行的固定目录及同一 Mapping 实例。
/// # 错误
/// 固定目录或模型无效时返回接入错误。
pub(crate) fn registration(
    db: &Database,
) -> erp_identity::Result<(AuthorizationRegistry, Arc<dyn AccessMapping>)> {
    let resource = "supplier";
    let collection = "supplier_accounts";
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
    let entries = ["list", "detail", "create", "update", "delete"]
        .into_iter()
        .map(|action| {
            let permission = Permission::parse(format!("{resource}:{action}"))?;
            Ok(AccessRegistration {
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
            })
        })
        .collect::<erp_identity::Result<Vec<_>>>()?;
    Ok((AuthorizationRegistry::new(entries)?, mapping))
}
