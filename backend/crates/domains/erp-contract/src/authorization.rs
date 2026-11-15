//! 本域授权登记和持久化事实；所有判定与条件均由身份域生成。
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::{AccessTarget, AuthorizationService, CompiledAccess, OrganizationDirectoryService};
use erp_permissions::{Permission, contract, internal};
use mongodb::Database;
use mongodb::bson::Document;
use persistence_core::{Executor, Transactional};

use crate::ports::ContractCreationPort;
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
    /// 真实客户当前归属满足合同创建范围时成功。
    /// # 错误
    /// 来源缺失、无权或版本变化时拒绝。
    pub async fn require_create(
        &self,
        actor: &AuditActor,
        customer_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
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
        self.compiled.ensure_current(executor)
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
mod permission_tests {
    use erp_permissions::sales_order;

    use super::*;

    #[test]
    fn accessor_accepts_its_constant_and_rejects_foreign_resource() {
        assert!(ensure_permission(&contract::DETAIL).is_ok());
        assert!(matches!(ensure_permission(&sales_order::DETAIL), Err(Error::ValidationError(_))));
    }
}
