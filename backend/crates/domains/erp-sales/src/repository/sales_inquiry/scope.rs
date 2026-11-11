//! identity 编译凭证在询价仓储的执行载体；不解释权限规则。
use std::sync::Arc;

use erp_identity::entity::authorization::QueryOutput;
use erp_identity::{AuthorizationService, CompiledAccess};
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::Executor;

use crate::repository::SalesInquiryExt;
use crate::service::authorization_error::identity_error;

/// 必须持有完整授权凭证的仓储读取载体。
#[derive(Clone)]
pub struct InquiryReadScope {
    access: Arc<CompiledAccess>,
    authorization: AuthorizationService,
}
impl std::fmt::Debug for InquiryReadScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InquiryReadScope").field("access", &self.access).finish()
    }
}
impl InquiryReadScope {
    /// 绑定本次 identity 编译结果和同一服务。
    /// # 参数
    /// access 为完整询价模型凭证，authorization 为生成它的服务。
    /// # 返回
    /// 仅用于当前事务读取的执行载体。
    /// # 错误
    /// 无；执行前后须复验。
    pub fn from_compiled(access: Arc<CompiledAccess>, authorization: AuthorizationService) -> Self {
        Self { access, authorization }
    }
    /// 返回完整授权阶段。
    /// # 参数
    /// 无。
    /// # 返回
    /// identity 生成的完整阶段。
    /// # 错误
    /// 不执行 I/O。
    pub fn stages(&self) -> Vec<Document> {
        self.access.stages().to_vec()
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
    /// 在原执行器复验 identity 凭证。
    /// # 参数
    /// executor 为生成凭证时的执行器。
    /// # 返回
    /// 当前授权仍有效时成功。
    /// # 错误
    /// 身份、映射、版本或会话变化返回原始错误。
    pub async fn validate(&self, executor: &mut dyn Executor) -> crate::Result<()> {
        if self.access.model().collection != Database::SALES_INQUIRIES
            || self.access.model().output != QueryOutput::Object
        {
            return Err(crate::Error::ValidationError("询价授权查询模型不匹配".into()));
        }
        self.authorization.validate(&self.access, executor).await.map_err(identity_error)
    }
}
