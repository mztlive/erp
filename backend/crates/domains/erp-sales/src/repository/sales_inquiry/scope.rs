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
    /// 结果：仅用于当前事务读取的执行载体。
    /// 失败条件：执行前后须复验。
    ///
    /// 输入约定：access 为完整询价模型凭证，authorization 为生成它的服务。
    pub fn from_compiled(access: Arc<CompiledAccess>, authorization: Arc<AuthorizationService>) -> Self {
        Self { access, authorization }
    }
    /// 提供 identity 编译的完整授权阶段；仓储在分页或计数前应用，不能替换成业务筛选。
    /// 结果：identity 生成的完整阶段。
    pub fn stages(&self) -> Vec<Document> {
        self.access.stages().to_vec()
    }
    /// 组装分页查询的前置阶段，先应用业务条件和完整授权，再排序或分页。
    /// 结果：查询前缀。
    ///
    /// 输入约定：filter 仅包含业务筛选。
    pub fn lookup_after(&self, filter: Document) -> Vec<Document> {
        let mut stages = vec![doc! {"$match": filter}];
        stages.extend(self.stages());
        stages
    }
    /// 查询前后用原执行器校验读取模型、执行边界和凭证有效期；此项检查不访问数据库。
    /// 结果：模型、边界和期限均符合当前读取时成功。
    /// 模型不匹配、跨阶段或到期时拒绝。
    ///
    /// 输入约定：executor 为生成凭证时的原执行器。
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

    /// 交付读取结果前，在原执行器复查此次授权实际依赖的身份、范围及动态事实。
    /// 结果：当前边界、期限及依赖仍有效时成功。
    /// 失败条件：身份、范围或动态依赖失效时拒绝交付。
    ///
    /// 输入约定：executor 为查询时的原执行器。
    pub async fn validate(&self, executor: &mut dyn Executor) -> crate::Result<()> {
        self.ensure_current(executor)?;
        self.authorization.validate(&self.access, executor).await.map_err(Into::into)
    }
}

impl SalesInquiryRepository<'_> {
    /// 使用完整查询凭证读取一张询价，授权过滤在单行限制之前执行。
    /// 返回授权条件命中的询价；不存在和无权均返回 None。
    /// 模型、读取阶段、期限失效或数据库读取失败时拒绝。
    ///
    /// 输入约定：id 为稳定询价身份，scope 为当前入口的查询凭证，executor 为原读取执行器。
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
