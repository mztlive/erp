//! 本域授权登记和持久化事实；所有判定与条件均由身份域生成。
use std::sync::Arc;

use erp_core::common::time::Instant;
use erp_identity::{AuthorizationService, CompiledAccess};
use mongodb::bson::{Document, doc};
use persistence_core::Executor;

/// 身份域编译的当前阶段凭证；不含可由业务组合的权限条款。
#[derive(Clone)]
pub struct CompiledScope {
    compiled: Option<Arc<CompiledAccess>>,
    authorization: Option<AuthorizationService>,
    pipeline: Vec<Document>,
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
            pipeline: Vec::new(),
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
        if self.compiled.is_some() { self.pipeline.clone() } else { vec![doc! {"$match":{"$expr":false}}] }
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
        pipeline: compiled.stages().to_vec(),
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
    async fn missing_identity_proof_rejects_execution() {
        let scope = CompiledScope::default();
        assert_eq!(scope.document(), doc! {"$expr":false});
        assert!(matches!(
            scope.validate(&mut persistence_core::NoTransaction).await,
            Err(erp_identity::Error::Forbidden(_))
        ));
    }
}
