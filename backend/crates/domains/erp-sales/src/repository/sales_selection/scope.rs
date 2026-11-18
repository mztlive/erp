//! identity 编译凭证在选品仓储的执行载体；不解释权限规则。
use std::sync::Arc;

use erp_identity::entity::authorization::QueryOutput;
use erp_identity::{AuthorizationService, CompiledAccess};
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, aggregate};

use super::SalesSelectionDomainRepository;
use crate::entity::sales_selection::{SalesSelectionBooklet, SalesSelectionProposal};
use crate::repository::SalesSelectionExt;

/// 必须持有完整授权凭证的仓储读取载体。
#[derive(Clone)]
pub struct SelectionReadScope {
    access: Arc<CompiledAccess>,
    authorization: Arc<AuthorizationService>,
}
impl std::fmt::Debug for SelectionReadScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SelectionReadScope").field("access", &self.access).finish()
    }
}
impl SelectionReadScope {
    /// 绑定本次 identity 编译结果和同一服务。
    /// 结果：仅用于当前事务读取的执行载体。
    /// 失败条件：执行前后须复验。
    ///
    /// 输入约定：access 为完整选品模型凭证，authorization 为生成它的服务。
    pub fn from_compiled(access: Arc<CompiledAccess>, authorization: Arc<AuthorizationService>) -> Self {
        Self { access, authorization }
    }
    /// 返回绑定身份和业务映射的跨页版本。
    /// 结果：当前凭证版本。
    pub fn version(&self) -> String {
        self.access.version()
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
    pub fn ensure_current(&self, collection: &str, executor: &mut dyn Executor) -> crate::Result<()> {
        let resource = match collection {
            Database::SALES_SELECTION_BOOKLETS => "sales_selection_booklet",
            Database::SALES_SELECTION_PROPOSALS => "sales_selection_proposal",
            _ => return Err(crate::Error::ValidationError("选品授权查询集合不匹配".into())),
        };
        if self.access.model().resource != resource
            || self.access.model().collection != collection
            || self.access.model().output != QueryOutput::Object
        {
            return Err(crate::Error::ValidationError("选品授权查询模型不匹配".into()));
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
        self.ensure_current(&self.access.model().collection, executor)?;
        self.authorization.validate(&self.access, executor).await.map_err(Into::into)
    }
}

impl SalesSelectionDomainRepository<'_> {
    /// 使用完整查询凭证读取一张选品册，授权过滤在单行限制之前执行。
    /// 返回授权条件命中的选品册；不存在和无权均返回 None。
    /// 模型、读取阶段、期限失效或数据库读取失败时拒绝。
    ///
    /// 输入约定：id 为稳定册身份，scope 为当前入口的查询凭证，executor 为原读取执行器。
    pub async fn find_authorized_booklet(
        &self,
        id: &str,
        scope: &SelectionReadScope,
        executor: &mut dyn Executor,
    ) -> crate::Result<Option<SalesSelectionBooklet>> {
        scope.ensure_current(Database::SALES_SELECTION_BOOKLETS, executor)?;
        let collection = self.db.collection::<SalesSelectionBooklet>(Database::SALES_SELECTION_BOOKLETS);
        let mut stages = scope.lookup_after(doc! { "id": id });
        stages.push(doc! { "$limit": 1_i64 });
        let mut rows = aggregate::rows(&collection, stages, executor).await?;
        scope.ensure_current(Database::SALES_SELECTION_BOOKLETS, executor)?;
        Ok(rows.pop())
    }

    /// 使用完整查询凭证读取一份方案，授权过滤先于单行限制。
    /// 结果：命中完整授权条件时返回方案；不存在和不可见均返回 None。
    /// 模型、阶段或期限失效，以及数据库读取失败时返回错误。
    ///
    /// 输入约定：id 为方案身份，scope 为当前方案入口的查询凭证，executor 为原快照执行器。
    pub async fn find_authorized_proposal(
        &self,
        id: &str,
        scope: &SelectionReadScope,
        executor: &mut dyn Executor,
    ) -> crate::Result<Option<SalesSelectionProposal>> {
        scope.ensure_current(Database::SALES_SELECTION_PROPOSALS, executor)?;
        let collection = self.db.collection::<SalesSelectionProposal>(Database::SALES_SELECTION_PROPOSALS);
        let mut stages = scope.lookup_after(doc! { "id": id });
        stages.push(doc! { "$limit": 1_i64 });
        let mut rows = aggregate::rows(&collection, stages, executor).await?;
        scope.ensure_current(Database::SALES_SELECTION_PROPOSALS, executor)?;
        Ok(rows.pop())
    }
}
