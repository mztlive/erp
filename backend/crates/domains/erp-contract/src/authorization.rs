//! 本域授权登记和持久化事实；所有判定与条件均由身份域生成。
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::ports::AccessMapping;
use erp_identity::{
    AccessSubject, AccessTarget, AuthorizationService, CompiledAccess, OrganizationDirectoryService,
    Permission, ResourceTargets,
};
use mongodb::Database;
use mongodb::bson::Document;
use persistence_core::{Executor, Transactional};

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
pub struct ContractAccess {
    db: Database,
    models: ContractModels,
}

impl ContractAccess {
    /// 绑定完整固定模型和共享身份设施。
    /// # 参数
    /// db 为组织事实数据库，models 为可信组合根登记的完整授权模型。
    /// # 返回
    /// 不缓存请求级授权结果的能力。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn new(db: Database, models: ContractModels) -> Self {
        Self { db, models }
    }

    /// 按固定动作选择已登记模型。
    fn model(&self, action: &str) -> Result<&(AuthorizationService, Arc<dyn AccessMapping>)> {
        let models = &self.models;
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
        Ok(scope(compiled, service.clone()))
    }

    /// 在独立事务中检查完整合同对象。
    /// # 参数
    /// actor 为当前账号，action/id 为实际入口和合同身份。
    /// # 返回
    /// 完整对象资格成立时成功，不编译列表查询。
    /// # 错误
    /// 对象缺失、无权、身份或业务事实失效时拒绝。
    pub async fn require(&self, actor: &AuditActor, action: &str, id: &str) -> Result<()> {
        let access = self.clone();
        let actor = actor.clone();
        let action = action.to_owned();
        let id = id.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { access.require_with(actor, &action, &id, executor).await })
            })
            .await
    }

    /// 在原业务事务内检查完整合同对象。
    /// # 参数
    /// actor、action、id 为可信入口及目标，executor 为原执行器。
    /// # 返回
    /// 对象授权通过时成功。
    /// # 错误
    /// 缺少对象、功能或来源资格时拒绝。
    pub async fn require_with(
        &self,
        actor: AuditActor,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let (service, _) = self.model(action)?;
        Ok(service
            .require(
                &actor,
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
        let models = &self.models;
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

/// 身份域编译的当前阶段凭证；业务分页版本可追加查询事实指纹。
#[derive(Clone)]
pub struct CompiledScope {
    compiled: Arc<CompiledAccess>,
    authorization: AuthorizationService,
    pub scope_version: String,
}
impl std::fmt::Debug for CompiledScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledScope").field("compiled", &self.compiled).finish()
    }
}
impl CompiledScope {
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
pub fn scope(compiled: CompiledAccess, service: AuthorizationService) -> CompiledScope {
    CompiledScope { scope_version: compiled.version(), compiled: Arc::new(compiled), authorization: service }
}
