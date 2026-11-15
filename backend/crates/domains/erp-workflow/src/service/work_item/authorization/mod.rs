//! 工作项授权复用进程内共享服务；业务层不声明授权政策。
mod facts;
use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::entity::authorization::{FactValue, ObjectFacts};
use erp_identity::{
    AccessFactsPort, AccessMapping, AccessTarget, AuthorizationService, CompiledAccess, Permission,
};
use erp_permissions::work_item;
use facts::WorkItemFacts;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::work_item::WorkItemType;
use crate::repository::WorkItemRepositoryExt;
use crate::repository::work_item::authorization::WorkItemMapping;
use crate::{Error, Result, WorkItemExt};

/// 任务读取与管理共享的授权服务。
#[derive(Clone)]
pub struct WorkItemAuthorization {
    pub service: Arc<AuthorizationService>,
    db: Database,
}
impl WorkItemAuthorization {
    /// 注入已经完成启动绑定的共享服务。
    /// # 参数
    /// service 为进程共享授权服务。
    /// # 返回
    /// 无请求状态的任务访问器。
    /// # 错误
    /// 不执行 I/O。
    pub fn new(db: Database, service: Arc<AuthorizationService>) -> Self {
        Self { service, db }
    }
    /// 校验任务详情范围和真实当前指定人。
    /// # 参数
    /// actor、id 为真实账号和任务，executor 为原事务。
    /// # 返回
    /// 详情规则通过且当前仍分配给账号时成功。
    /// # 错误
    /// 权限或当前分配不符时拒绝。
    pub async fn require_assigned(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let task = self
            .db
            .work_items()
            .find_work_item(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("任务不存在".into()))?;
        self.service
            .require(
                actor,
                &work_item::DETAIL,
                AccessTarget::Task { id: task.base.id.clone(), version: task.base.version.to_string() },
                executor,
            )
            .await?;
        if task.owner_user_id.as_deref() != Some(actor.id()) {
            return Err(Error::Forbidden("任务已指派给其他人".into()));
        }
        Ok(())
    }
    /// 编译固定工作项权限的读取条件。
    /// # 参数
    /// actor 为当前账号，permission 为内置权限常量，executor 为原读取阶段。
    /// # 返回
    /// 绑定当前权限和读取阶段的查询凭证。
    /// # 错误
    /// 资格、映射或执行边界不符时拒绝。
    pub(crate) async fn query(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<Arc<CompiledAccess>> {
        Ok(Arc::new(self.service.query(actor, permission, executor).await?))
    }
    /// 检查已有任务的完整对象访问。
    /// # 参数
    /// actor、permission、id 为账号、固定权限和真实任务，executor 为原执行器。
    /// # 返回
    /// 入口和固定数据规则均通过时成功。
    /// # 错误
    /// 未知动作、缺失对象或拒绝时返回错误。
    pub async fn require(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self.service.require(actor, permission, AccessTarget::record(id)?, executor).await?)
    }
}
/// 提供任务的持久化事实读取器。
/// # 参数
/// db 为工作项数据库。
/// # 返回
/// 读取真实任务版本、责任人及组织的共享读取器。
/// # 错误
/// 构造不执行 I/O。
pub fn authorization_facts(db: Database) -> Arc<dyn AccessFactsPort> {
    Arc::new(WorkItemFacts { db })
}
/// 提供固定工作项查询模型。
/// # 参数
/// 无。
/// # 返回
/// 当前负责人和主属组织的静态 Mapping。
/// # 错误
/// 构造不执行 I/O。
pub fn authorization_mapping() -> Arc<dyn AccessMapping> {
    WorkItemMapping::shared()
}

/// 从服务端确定的任务业务身份构造尚未形成任务的功能资格候选。
/// # 参数
/// kind、source 为责任规则或已验证业务对象确定的真实任务种类和来源类型。
/// # 返回
/// 不携带对象身份、版本或权限列表的候选；不证明已存在任务的访问权。
/// # 错误
/// 构造不执行 I/O，种类及来源组合由 identity 固定入口校验。
pub(crate) fn execution_target(kind: WorkItemType, source: &str) -> AccessTarget {
    AccessTarget::Candidate(ObjectFacts {
        resource: "work_item".into(),
        subject_type: FactValue::Known(kind.as_str().into()),
        subject_source_resource: FactValue::Known(source.into()),
        ..Default::default()
    })
}
