//! 本域授权登记和持久化事实；所有判定与条件均由身份域生成。
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::ports::AccessMapping;
use erp_identity::repository::OrganizationRepository;
use erp_identity::{
    AccessSubject, AccessTarget, AuthorizationService, CompiledAccess, OrganizationDirectoryService,
    Permission, ResourceTargets, SharedRbacService,
};
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::Executor;

use crate::ports::ContractCreationPort;
use crate::{Error, Result};

/// 组合根装配的固定合同读取、写入及客户来源模型。
#[derive(Clone)]
pub struct ContractModels {
    /// 合同列表和详情的统一授权服务及映射。
    pub read: (AuthorizationService, Arc<dyn AccessMapping>),
    /// 合同写入的统一授权服务及映射。
    pub write: (AuthorizationService, Arc<dyn AccessMapping>),
    /// 合同归档的实际客户来源服务及映射。
    pub creation: Arc<dyn ContractCreationPort>,
}

/// 合同授权设施；事实由组合根装配，范围规则由 identity 执行。
#[derive(Clone)]
pub struct ContractAuthorization {
    db: Database,
    rbac: Option<SharedRbacService>,
    models: Option<ContractModels>,
}

impl ContractAuthorization {
    /// 绑定完整固定模型和共享身份设施。
    /// # 参数
    /// db、rbac 为身份设施，models 为可信组合根登记的模型。
    /// # 返回
    /// 不缓存请求级授权结果的能力。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn new(db: Database, rbac: SharedRbacService, models: ContractModels) -> Self {
        Self { db, rbac: Some(rbac), models: Some(models) }
    }

    /// 构造未接线的失败关闭能力。
    /// # 参数
    /// db 为业务数据库。
    /// # 返回
    /// 所有授权入口拒绝的能力。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn unwired(db: Database) -> Self {
        Self { db, rbac: None, models: None }
    }

    /// 按固定动作选择已登记模型。
    fn model(&self, action: &str) -> Result<&(AuthorizationService, Arc<dyn AccessMapping>)> {
        let models = self.models.as_ref().ok_or_else(|| Error::Internal("合同身份设施未接线".into()))?;
        match action {
            "list" | "detail" => Ok(&models.read),
            "update" => Ok(&models.write),
            _ => Err(Error::ValidationError("未登记合同动作".into())),
        }
    }

    /// 编译当前读取阶段的完整合同条件。
    /// # 参数
    /// actor、action 为可信入口，executor 为原执行器。
    /// # 返回
    /// 包含完整复验能力的当前阶段凭证。
    /// # 错误
    /// 身份、模型、功能资格或事实取证失败时拒绝。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<CompiledScope> {
        let (service, mapping) = self.model(action)?;
        let permission = Permission::parse(format!("contract:{action}"))?;
        let query = service.query(actor, &permission, AccessSubject::Direct, executor).await?;
        let compiled = query.compile(mapping.as_ref(), executor).await?;
        service.validate(&compiled, executor).await?;
        let rbac = self.rbac.as_ref().expect("固定模型和身份设施同时装配");
        Ok(scope(
            compiled,
            service.clone(),
            rbac.policy_revision_with_executor(executor).await?,
            OrganizationRepository::new(&self.db).state(executor).await?.version,
        ))
    }

    /// 在原业务事务内检查完整合同对象。
    /// # 参数
    /// actor、action、id 为可信入口及目标，executor 为原执行器。
    /// # 返回
    /// 对象授权通过时成功。
    /// # 错误
    /// 缺少对象、功能或来源资格时拒绝。
    pub async fn require(
        &self,
        actor: &AuditActor,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let (service, _) = self.model(action)?;
        Ok(service
            .require(
                actor,
                &Permission::parse(format!("contract:{action}"))?,
                AccessTarget::Records { subject: AccessSubject::Direct, ids: ResourceTargets::new(&[id])? },
                executor,
            )
            .await?)
    }

    /// 检查真实客户来源的归档资格。
    /// # 参数
    /// actor 为当前账号，customer_id 为完整来源，executor 为原执行器。
    /// # 返回
    /// 当前阶段的来源授权凭证。
    /// # 错误
    /// 来源缺失、无权或版本变化时拒绝。
    pub async fn require_create(
        &self,
        actor: &AuditActor,
        customer_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<CompiledScope> {
        let models = self.models.as_ref().ok_or_else(|| Error::Internal("合同身份设施未接线".into()))?;
        models.creation.require_create(actor, customer_id, executor).await
    }

    /// 展开业务筛选使用的有效内部组织。
    /// # 参数
    /// ids 为组织集合，desc 指定包含下级，executor 为原执行器。
    /// # 返回
    /// 启用组织集合。
    /// # 错误
    /// 未知组织、组织状态非法或读取失败时拒绝。
    pub async fn expand_org_units(
        &self,
        ids: &[String],
        desc: bool,
        executor: &mut dyn Executor,
    ) -> Result<std::collections::BTreeSet<String>> {
        Ok(OrganizationDirectoryService::new(self.db.clone()).expand_org_units(ids, desc, executor).await?)
    }

    /// 读取指定时点的有效组织成员，供业务筛选收窄。
    /// # 参数
    /// ids 为已展开组织，at 为业务时点，executor 为原执行器。
    /// # 返回
    /// 去重后的成员身份。
    /// # 错误
    /// 事实读取失败时返回错误。
    pub async fn org_member_ids(
        &self,
        ids: &std::collections::BTreeSet<String>,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        Ok(OrganizationDirectoryService::new(self.db.clone()).org_member_ids(ids, at, executor).await?)
    }
}

/// 身份域编译的当前阶段凭证；不含可由业务组合的权限条款。
#[derive(Clone)]
pub struct CompiledScope {
    compiled: Option<Arc<CompiledAccess>>,
    authorization: Option<AuthorizationService>,
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
            doc! {}
        } else {
            doc! {"$expr":false}
        }
    }
    /// 读取身份域完整编译阶段。
    /// # 参数
    /// 无。
    /// # 返回
    /// 在分页与统计前执行的阶段。
    /// # 错误
    /// 未接线时返回恒假阶段。
    pub fn stages(&self) -> Vec<Document> {
        self.compiled
            .as_ref()
            .map_or_else(|| vec![doc! {"$match":{"$expr":false}}], |compiled| compiled.stages().to_vec())
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

/// 组合根将统一编译结果绑定原服务，业务无法自行构造有效凭证。
/// # 参数
/// compiled为identity生成的凭证，service为原服务，其余为显示元数据。
/// # 返回
/// 本阶段载体。
/// # 错误
/// 构造不执行I/O。
pub fn scope(
    compiled: CompiledAccess,
    service: AuthorizationService,
    policy_version: u64,
    organization_version: u64,
) -> CompiledScope {
    let actor = compiled.actor().id().to_owned();
    let permission = compiled.permission().clone();
    CompiledScope {
        scope_version: compiled.version(),
        user_id: actor,
        resource: permission.resource().into(),
        action: permission.action().into(),
        compiled: Some(Arc::new(compiled)),
        authorization: Some(service),
        policy_version,
        organization_version,
        as_of: Instant::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn unwired_models_reject_queries_objects_and_creation() {
        let db = mongodb::Client::with_options(Default::default()).unwrap().database("unused");
        let authorization = ContractAuthorization::unwired(db);
        let actor = AuditActor::new("actor".into(), "Actor".into(), erp_core::AccountKind::Admin);
        let mut executor = persistence_core::NoTransaction;
        assert!(matches!(
            authorization.resolve(&actor, "list", &mut executor).await,
            Err(Error::Internal(_))
        ));
        assert!(matches!(
            authorization.require(&actor, "update", "contract-a", &mut executor).await,
            Err(Error::Internal(_))
        ));
        assert!(matches!(
            authorization.require_create(&actor, "customer-a", &mut executor).await,
            Err(Error::Internal(_))
        ));
    }

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
