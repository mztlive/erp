//! identity 编译凭证在询价仓储的执行载体；不解释权限规则。
use std::sync::Arc;

use erp_identity::entity::authorization::QueryOutput;
use erp_identity::{AuthorizationService, CompiledAccess};
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, aggregate};

use super::SalesInquiryRepository;
use crate::entity::sales_inquiry::SalesInquiry;
use crate::repository::SalesInquiryExt;

/// 必须持有完整授权凭证的仓储读取载体。
#[derive(Clone)]
pub struct InquiryReadScope {
    access: Arc<CompiledAccess>,
    authorization: Arc<AuthorizationService>,
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
    pub fn from_compiled(access: Arc<CompiledAccess>, authorization: Arc<AuthorizationService>) -> Self {
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
    /// 在仓储执行前后检查模型、原执行边界和有效期，不读取数据库。
    /// # 参数
    /// executor 为生成凭证时的原执行器。
    /// # 返回
    /// 模型、边界和期限均符合当前读取时成功。
    /// # 错误
    /// 模型不匹配、跨阶段或到期时拒绝。
    pub fn ensure_current(&self, executor: &mut dyn Executor) -> crate::Result<()> {
        if !matches!(self.access.model().resource.as_str(), "sales_inquiry" | "procurement_inquiry")
            || self.access.model().collection != Database::SALES_INQUIRIES
            || self.access.model().output != QueryOutput::Object
        {
            return Err(crate::Error::ValidationError("询价授权查询模型不匹配".into()));
        }

        self.access.ensure_current(executor)?;
        Ok(())
    }

    /// 在读取用例交付结果前复验本次实际授权依赖。
    /// # 参数
    /// executor 为查询时的原执行器。
    /// # 返回
    /// 当前边界、期限及依赖仍有效时成功。
    /// # 错误
    /// 身份、范围或动态依赖失效时拒绝交付。
    pub async fn validate(&self, executor: &mut dyn Executor) -> crate::Result<()> {
        self.ensure_current(executor)?;
        self.authorization.validate(&self.access, executor).await.map_err(Into::into)
    }
}

impl SalesInquiryRepository<'_> {
    /// 使用完整查询凭证读取一张询价，授权过滤在单行限制之前执行。
    ///
    /// # 参数
    /// id 为稳定询价身份，scope 为当前入口的查询凭证，executor 为原读取执行器。
    ///
    /// # 返回
    /// 返回授权条件命中的询价；不存在和无权均返回 None。
    ///
    /// # 错误
    /// 模型、读取阶段、期限失效或数据库读取失败时拒绝。
    pub async fn find_authorized(
        &self,
        id: &str,
        scope: &InquiryReadScope,
        executor: &mut dyn Executor,
    ) -> crate::Result<Option<SalesInquiry>> {
        scope.ensure_current(executor)?;

        let collection = self.db.collection::<SalesInquiry>(Database::SALES_INQUIRIES);
        let mut stages = scope.lookup_after(doc! { "id": id });
        stages.push(doc! { "$limit": 1_i64 });
        let mut rows = aggregate::rows(&collection, stages, executor).await?;

        scope.ensure_current(executor)?;
        Ok(rows.pop())
    }
}
