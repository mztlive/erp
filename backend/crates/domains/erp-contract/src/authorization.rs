//! 本域授权登记和持久化事实；所有判定与条件均由身份域生成。
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::entity::authorization::{QueryModel, QueryOutput};
use erp_identity::{
    AccessTarget, AuthorizationService, CompiledAccess, Error as IdentityError, OrganizationDirectoryService,
};
use erp_permissions::{Permission, contract, internal};
use mongodb::Database;
use mongodb::bson::Document;
use persistence_core::{Executor, Transactional};

use crate::ports::ContractCreationPort;
use crate::repository::ContractExt;
use crate::{Error, Result};

/// 合同授权设施；事实由组合根装配，范围规则由 identity 执行。
#[derive(Clone)]
pub struct ContractAccess {
    db: Database,
    pub(crate) authorization: Arc<AuthorizationService>,
    creation: Arc<dyn ContractCreationPort>,
}

impl ContractAccess {
    /// 在原事务中检查合同识别任务的功能资格；归档仍须检查真实创建候选。
    /// # 参数
    /// actor 为当前账号，executor 为任务读取或写入的快照执行器。
    /// # 返回
    /// 当前有效账号具备合同创建功能资格时成功。
    /// # 错误
    /// 无资格、非快照执行器或身份设施失败时拒绝。
    pub async fn require_import(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<()> {
        Ok(self
            .authorization
            .require(actor, &internal::CONTRACT_IMPORT, AccessTarget::Operation, executor)
            .await?)
    }

    /// 绑定完整固定模型和共享身份设施。
    /// # 参数
    /// db 为组织事实数据库，authorization 为共享授权服务，creation 提供真实客户候选归属。
    /// # 返回
    /// 不缓存请求级授权结果的能力。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn new(
        db: Database,
        authorization: Arc<AuthorizationService>,
        creation: Arc<dyn ContractCreationPort>,
    ) -> Self {
        Self { db, authorization, creation }
    }

    /// 编译当前读取阶段的完整合同条件。
    /// # 参数
    /// actor、permission 为可信入口，executor 为原执行器。
    /// # 返回
    /// 包含完整复验能力的当前阶段凭证。
    /// # 错误
    /// 身份、模型、功能资格或事实取证失败时拒绝。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<CompiledScope> {
        ensure_permission(permission)?;
        let compiled = self.authorization.query(actor, permission, executor).await?;
        Ok(scope(compiled, self.authorization.clone()))
    }

    /// 在独立事务中检查完整合同对象。
    /// # 参数
    /// actor 为当前账号，permission/id 为实际入口和合同身份。
    /// # 返回
    /// 完整对象资格成立时成功，不编译列表查询。
    /// # 错误
    /// 对象缺失、无权、身份或业务事实失效时拒绝。
    pub async fn require(&self, actor: &AuditActor, permission: &Permission, id: &str) -> Result<()> {
        let access = self.clone();
        let actor = actor.clone();
        let permission = permission.clone();
        let id = id.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { access.require_with(actor, &permission, &id, executor).await })
            })
            .await
    }

    /// 在原业务事务内检查完整合同对象。
    /// # 参数
    /// actor、permission、id 为可信入口及目标，executor 为原执行器。
    /// # 返回
    /// 对象授权通过时成功。
    /// # 错误
    /// 缺少对象、功能或来源资格时拒绝。
    pub async fn require_with(
        &self,
        actor: AuditActor,
        permission: &Permission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        ensure_permission(permission)?;
        Ok(self.authorization.require(&actor, permission, AccessTarget::record(id)?, executor).await?)
    }

    /// 检查真实客户来源的归档资格。
    /// # 参数
    /// actor 为当前账号，customer_id 为完整来源，executor 为原执行器。
    /// # 返回
    /// 拟写入合同归属与真实客户当前来源分别满足固定规则时成功。
    /// # 错误
    /// 来源缺失、无权或版本变化时拒绝。
    pub async fn require_create(
        &self,
        actor: &AuditActor,
        customer_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.require_import(actor, executor).await?;
        let candidate = self.creation.candidate(actor, customer_id, executor).await?;
        Ok(self
            .authorization
            .require(actor, &contract::CREATE, AccessTarget::Candidate(candidate), executor)
            .await?)
    }

    /// 返回筛选所需的身份域组织目录。
    /// # 参数
    /// 无。
    /// # 返回
    /// 使用当前数据库的组织事实查询能力。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn organization(&self) -> OrganizationDirectoryService {
        OrganizationDirectoryService::new(self.db.clone())
    }
}

/// 身份域编译的当前阶段凭证；业务分页版本可追加查询事实指纹。
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
    /// 返回身份域已取证的正向角色范围摘要，不从业务结果推断权限。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前凭证存在正向角色范围时为 true。
    /// # 错误
    /// 无；仓储消费及交付仍须分别检查当前边界和 validate。
    pub fn has_role_scope(&self) -> bool {
        self.compiled.has_role_scope()
    }

    /// 读取本次授权账号。
    /// # 参数
    /// 无。
    /// # 返回
    /// identity 绑定的实际账号身份。
    /// # 错误
    /// 无。
    pub fn user_id(&self) -> &str {
        self.compiled.actor().id()
    }

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

    /// 读取身份域完整编译阶段。
    /// # 参数
    /// 无。
    /// # 返回
    /// 在分页与统计前执行的阶段。
    /// # 错误
    /// 无。
    pub fn stages(&self) -> Vec<Document> {
        self.compiled.stages().to_vec()
    }

    /// 核验当前事务、读取阶段及有效期，不读取数据库。
    /// # 参数
    /// executor 为原读取执行器。
    /// # 返回
    /// 凭证仍属于当前阶段时成功。
    /// # 错误
    /// 跨事务、跨阶段或已经到期时拒绝。
    pub fn ensure_current(&self, executor: &mut dyn Executor) -> erp_identity::Result<()> {
        Self::ensure_model(self.compiled.model())?;
        self.compiled.ensure_current(executor)
    }

    /// 合同仓储只执行合同集合的完整对象凭证，拒绝跨资源或受限输出复用。
    fn ensure_model(model: &QueryModel) -> erp_identity::Result<()> {
        if model.resource != "contract"
            || model.collection != <Database as ContractExt>::CONTRACTS
            || model.id_field != "id"
            || model.output != QueryOutput::Object
        {
            return Err(IdentityError::ValidationError("合同查询凭证的资源、集合或输出不符".into()));
        }
        Ok(())
    }

    /// 复验账号、权限、版本和有效期。
    /// # 参数
    /// executor 必须为原执行器。
    /// # 返回
    /// 凭证仍有效时成功。
    /// # 错误
    /// 撤权、到期或版本变化拒绝。
    pub async fn validate(&self, executor: &mut dyn Executor) -> erp_identity::Result<()> {
        self.authorization.validate(&self.compiled, executor).await
    }
}

/// 组合根将统一编译结果绑定原服务，业务无法自行构造有效凭证。
/// # 参数
/// compiled 为 identity 生成的凭证，service 为原服务。
/// # 返回
/// 本阶段载体。
/// # 错误
/// 构造不执行 I/O。
pub fn scope(compiled: CompiledAccess, service: Arc<AuthorizationService>) -> CompiledScope {
    CompiledScope { scope_version: compiled.version(), compiled: Arc::new(compiled), authorization: service }
}

/// 内置权限直接接入时仍校验拥有资源，禁止跨域凭证替换。
fn ensure_permission(permission: &Permission) -> Result<()> {
    if permission.resource() != "contract" {
        return Err(Error::ValidationError("权限资源与对象访问器不符".into()));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod query_tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use async_trait::async_trait;
    use erp_core::AccountKind;
    use erp_identity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::entity::authorization::{
        AccessContext, AccessFact, FactKind, MappingFacts, QueryModelKey, RelationKey,
    };
    use erp_identity::ports::{MappingPreparation, QueryFragment, RelationMapping};
    use erp_identity::testing::{AuthorizationFixture, expire_access};
    use erp_identity::{AccessMapping, Result as IdentityResult};
    use mongodb::bson::{Bson, doc};
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    pub(crate) struct Mapping {
        model: QueryModel,
        revision: AtomicU64,
    }

    impl AccessMapping for Mapping {
        fn model(&self) -> &QueryModel {
            &self.model
        }
        fn preparation(&self) -> Option<&dyn MappingPreparation> {
            Some(self)
        }
        fn key(&self, _: &AccessContext, _: Option<&MappingFacts>) -> IdentityResult<Bson> {
            Ok("$id".into())
        }
        fn records(&self, _: &AccessContext, _: Option<&MappingFacts>) -> IdentityResult<QueryFragment> {
            Ok(QueryFragment::Match(doc! { "deleted_at": 0 }))
        }
        fn predicate(
            &self,
            fact: &AccessFact,
            context: &AccessContext,
            _: Option<&MappingFacts>,
        ) -> IdentityResult<QueryFragment> {
            Ok(QueryFragment::Match(match fact {
                AccessFact::Owned => doc! { "sales_owner_user_id": context.actor.id() },
                AccessFact::Collaborating => doc! { "shared_sales_user_ids": context.actor.id() },
                AccessFact::HistoryParticipant => doc! { "history": context.actor.id() },
                AccessFact::OrganizationIn(ids) => {
                    doc! { "owner_org": { "$in": ids.iter().cloned().collect::<Vec<_>>() } }
                },
                _ => return Err(IdentityError::ValidationError("测试不支持该事实".into())),
            }))
        }
        fn relation(
            &self,
            _: &RelationKey,
            _: &AccessContext,
            _: Option<&MappingFacts>,
        ) -> IdentityResult<RelationMapping> {
            Err(IdentityError::ValidationError("测试模型没有来源关系".into()))
        }
    }

    #[async_trait]
    impl MappingPreparation for Mapping {
        async fn prepare(&self, _: &AccessContext, _: &mut dyn Executor) -> IdentityResult<MappingFacts> {
            let revision = self.revision.load(Ordering::SeqCst);
            Ok(MappingFacts {
                data: serde_json::json!({ "revision": revision }),
                version: Some(revision.to_string()),
                valid_until: None,
            })
        }
    }

    pub(crate) fn fixture(resolved: ResolvedScope) -> (Arc<AuthorizationService>, Arc<Mapping>) {
        let mapping = Arc::new(Mapping {
            model: QueryModel {
                key: QueryModelKey::parse("contract_responsibility").unwrap(),
                resource: "contract".into(),
                collection: <Database as ContractExt>::CONTRACTS.into(),
                id_field: "id".into(),
                output: QueryOutput::Object,
                facts: [
                    FactKind::Owner,
                    FactKind::Organization,
                    FactKind::Collaborators,
                    FactKind::HistoryParticipants,
                ]
                .into(),
            },
            revision: AtomicU64::new(1),
        });
        let service = AuthorizationFixture::new(&[("reader", &[contract::LIST, contract::DETAIL])])
            .scope(resolved)
            .mapping(contract::LIST, mapping.clone())
            .build();
        (service, mapping)
    }

    pub(crate) fn actor() -> AuditActor {
        AuditActor::new("user".into(), "login".into(), AccountKind::Admin)
    }

    fn own_scope() -> ResolvedScope {
        ResolvedScope {
            role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
            user_limit: None,
        }
    }

    #[tokio::test]
    async fn real_contract_query_proofs_reject_cross_stage_cross_transaction_and_expired_consumption() {
        let (service, _) = fixture(own_scope());
        let mut executor = SnapshotExecutor::default();
        let access =
            scope(service.query(&actor(), &contract::LIST, &mut executor).await.unwrap(), service.clone());
        assert!(access.has_role_scope());
        assert!(!access.stages().is_empty());
        access.ensure_current(&mut executor).unwrap();
        access.validate(&mut executor).await.unwrap();
        let copied = access.clone();
        executor.next_read();
        assert!(matches!(copied.ensure_current(&mut executor), Err(IdentityError::ValidationError(_))));
        assert!(matches!(access.validate(&mut executor).await, Err(IdentityError::ValidationError(_))));

        let access =
            scope(service.query(&actor(), &contract::LIST, &mut executor).await.unwrap(), service.clone());
        executor.restart();
        assert!(matches!(access.ensure_current(&mut executor), Err(IdentityError::ValidationError(_))));
        let mut access =
            scope(service.query(&actor(), &contract::LIST, &mut executor).await.unwrap(), service);
        expire_access(Arc::get_mut(&mut access.compiled).unwrap());
        assert!(
            matches!(access.ensure_current(&mut executor), Err(IdentityError::ConflictError(message)) if message.starts_with("DATA_SCOPE_CHANGED"))
        );
        assert!(
            matches!(access.validate(&mut executor).await, Err(IdentityError::ConflictError(message)) if message.starts_with("DATA_SCOPE_CHANGED"))
        );
    }

    #[tokio::test]
    async fn actual_dynamic_dependency_change_rejects_nontransactional_delivery() {
        let (service, mapping) = fixture(own_scope());
        let mut executor = ReadExecutor::default();
        let access =
            scope(service.query(&actor(), &contract::LIST, &mut executor).await.unwrap(), service.clone());
        access.validate(&mut executor).await.unwrap();
        assert!(matches!(access.ensure_current(&mut executor), Err(IdentityError::ValidationError(_))));
        let access = scope(service.query(&actor(), &contract::LIST, &mut executor).await.unwrap(), service);
        mapping.revision.fetch_add(1, Ordering::SeqCst);
        assert!(
            matches!(access.validate(&mut executor).await, Err(IdentityError::ConflictError(message)) if message.contains("业务授权事实已变化"))
        );
    }
}

#[cfg(test)]
mod permission_tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use erp_core::AccountKind;
    use erp_identity::entity::authorization::ObjectFacts;
    use erp_identity::testing::unavailable_authorization;
    use erp_permissions::sales_order;
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    #[test]
    fn accessor_accepts_its_constant_and_rejects_foreign_resource() {
        assert!(ensure_permission(&contract::DETAIL).is_ok());
        assert!(matches!(ensure_permission(&sales_order::DETAIL), Err(Error::ValidationError(_))));
    }

    struct CreationFacts(AtomicUsize);

    #[async_trait]
    impl ContractCreationPort for CreationFacts {
        async fn candidate(&self, _: &AuditActor, _: &str, _: &mut dyn Executor) -> Result<ObjectFacts> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(Error::NotFound("不应读取客户".into()))
        }
    }

    #[tokio::test]
    async fn create_qualification_and_snapshot_are_checked_before_customer_facts() {
        let db = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("contract_candidate_authorization");
        let creation = Arc::new(CreationFacts(AtomicUsize::new(0)));
        let access = ContractAccess::new(db, unavailable_authorization(), creation.clone());
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let error = access
            .require_create(&actor, "unread-customer", &mut SnapshotExecutor::default())
            .await
            .expect_err("资格取证失败必须先于客户候选事实读取");
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
        assert_eq!(creation.0.load(Ordering::SeqCst), 0);
        let error = access
            .require_create(&actor, "unread-customer", &mut NoTransaction)
            .await
            .expect_err("非快照调用不得读取客户候选事实");
        assert!(matches!(error, Error::ValidationError(message) if message == "写入授权必须使用快照事务"));
        assert_eq!(creation.0.load(Ordering::SeqCst), 0);
    }
}

#[cfg(test)]
mod model_tests {
    use erp_identity::entity::authorization::QueryModelKey;

    use super::*;

    #[test]
    fn contract_repository_rejects_other_resources_collections_and_restricted_outputs() {
        let model = QueryModel {
            key: QueryModelKey::parse("contract_responsibility").unwrap(),
            resource: "contract".into(),
            collection: <Database as ContractExt>::CONTRACTS.into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: Default::default(),
        };
        assert!(CompiledScope::ensure_model(&model).is_ok());
        let mut invalid = Vec::new();
        let mut other = model.clone();
        other.resource = "customer".into();
        invalid.push(other);
        let mut other = model.clone();
        other.collection = "customers".into();
        invalid.push(other);
        let mut other = model.clone();
        other.id_field = "source_id".into();
        invalid.push(other);
        for output in [
            QueryOutput::Share { fields: ["id".into()].into() },
            QueryOutput::Existence { fields: ["id".into()].into() },
        ] {
            let mut other = model.clone();
            other.output = output;
            invalid.push(other);
        }
        for other in invalid {
            assert!(matches!(CompiledScope::ensure_model(&other), Err(IdentityError::ValidationError(_))));
        }
    }
}
