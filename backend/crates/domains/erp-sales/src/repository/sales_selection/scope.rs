//! identity 编译凭证在选品仓储的执行载体；不解释权限规则。
use std::sync::Arc;

use erp_identity::{AuthorizationService, CompiledAccess};
use mongodb::bson::{Document, doc};
use persistence_core::Executor;

/// 不可构造权限条件的仓储读取载体；缺凭证始终为空。
#[derive(Clone, Default)]
pub struct SelectionReadScope {
    access: Option<Arc<CompiledAccess>>,
    authorization: Option<AuthorizationService>,
}
impl std::fmt::Debug for SelectionReadScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SelectionReadScope").field("access", &self.access).finish()
    }
}
impl SelectionReadScope {
    /// 绑定本次 identity 编译结果和同一服务。
    /// # 参数
    /// access 为完整选品模型凭证，authorization 为生成它的服务。
    /// # 返回
    /// 仅用于当前事务读取的执行载体。
    /// # 错误
    /// 无；执行前后须复验。
    pub fn from_compiled(access: Arc<CompiledAccess>, authorization: AuthorizationService) -> Self {
        Self { access: Some(access), authorization: Some(authorization) }
    }
    /// 返回绑定身份和业务映射的跨页版本。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前凭证版本；缺少凭证时为空字符串。
    /// # 错误
    /// 不执行 I/O。
    pub fn version(&self) -> String {
        self.access.as_ref().map(|access| access.version()).unwrap_or_default()
    }

    /// 返回完整授权阶段。
    /// # 参数
    /// 无。
    /// # 返回
    /// identity 生成的完整阶段。
    /// # 错误
    /// 无凭证时返回恒假阶段。
    pub fn stages(&self) -> Vec<Document> {
        self.access
            .as_ref()
            .map(|a| a.stages().to_vec())
            .unwrap_or_else(|| vec![doc! {"$match": {"$expr": false}}])
    }
    /// 将业务筛选和完整授权前置于分页。
    /// # 参数
    /// filter 仅包含业务筛选。
    /// # 返回
    /// 查询前缀。
    /// # 错误
    /// 无。
    pub fn lookup_after(&self, filter: Document) -> Vec<Document> {
        let mut stages = vec![doc! {"$match": filter}];
        stages.extend(self.stages());
        stages
    }
    /// 只表示是否缺少凭证，不推断权限规则。
    /// # 参数
    /// 无。
    /// # 返回
    /// 无凭证为 true。
    /// # 错误
    /// 无。
    pub fn is_empty(&self) -> bool {
        self.access.is_none()
    }
    /// 在原执行器复验 identity 凭证。
    /// # 参数
    /// executor 为生成凭证时的执行器。
    /// # 返回
    /// 当前授权仍有效时成功。
    /// # 错误
    /// 身份、映射、版本或会话变化返回原始错误。
    pub async fn validate(&self, executor: &mut dyn Executor) -> erp_identity::Result<()> {
        match (&self.access, &self.authorization) {
            (Some(access), Some(service)) => service.validate(access, executor).await,
            _ => Err(erp_identity::Error::Forbidden("缺少选品授权凭证".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use persistence_core::NoTransaction;

    use super::*;

    #[tokio::test]
    async fn missing_proof_never_becomes_an_unrestricted_selection_query() {
        let access = SelectionReadScope::default();
        assert!(access.is_empty());
        assert!(access.version().is_empty());
        assert_eq!(access.stages(), vec![doc! {"$match": {"$expr": false}}]);
        assert!(matches!(access.validate(&mut NoTransaction).await, Err(erp_identity::Error::Forbidden(_))));
    }
}
