//! identity 编译凭证在询价仓储的执行载体；不解释权限规则。
use std::sync::Arc;

use erp_identity::entity::authorization::QueryOutput;
use erp_identity::{AuthorizationService, CompiledAccess};
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::Executor;

use crate::repository::SalesInquiryExt;
use crate::service::authorization_error::identity_error;

/// 不可构造权限条件的仓储读取载体；缺凭证始终为空。
#[derive(Clone, Default)]
pub struct InquiryReadScope {
    access: Option<Arc<CompiledAccess>>,
    authorization: Option<AuthorizationService>,
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
        Self { access: Some(access), authorization: Some(authorization) }
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
    /// 业务匹配不重复解释已经执行的授权阶段。
    /// # 参数
    /// 无。
    /// # 返回
    /// 有凭证为空条件，缺凭证恒假。
    /// # 错误
    /// 无。
    pub fn document(&self) -> Document {
        if self.access.is_some() {
            doc! {}
        } else {
            doc! {"$expr": false}
        }
    }
    /// 在原执行器复验 identity 凭证。
    /// # 参数
    /// executor 为生成凭证时的执行器。
    /// # 返回
    /// 当前授权仍有效时成功。
    /// # 错误
    /// 身份、映射、版本或会话变化返回原始错误。
    pub async fn validate(&self, executor: &mut dyn Executor) -> crate::Result<()> {
        match (&self.access, &self.authorization) {
            (Some(access), Some(service)) => {
                if access.model().collection != Database::SALES_INQUIRIES
                    || access.model().output != QueryOutput::Object
                {
                    return Err(crate::Error::ValidationError("询价授权查询模型不匹配".into()));
                }
                service.validate(access, executor).await.map_err(identity_error)
            },
            _ => Err(crate::Error::Forbidden("缺少询价授权凭证".into())),
        }
    }
}
