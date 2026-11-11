//! identity 编译凭证在销售仓储的执行载体；不解释权限规则。
use std::sync::Arc;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_identity::entity::authorization::QueryOutput;
use erp_identity::{AuthorizationService, CompiledAccess};
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, aggregate};

use crate::Result;
use crate::repository::SalesOrderExt;
use crate::service::authorization_error::identity_error;

/// 不可构造权限条件的仓储读取载体；缺凭证始终为空。
#[derive(Clone, Default)]
pub struct SalesReadScope {
    access: Option<Arc<CompiledAccess>>,
    authorization: Option<AuthorizationService>,
}
impl std::fmt::Debug for SalesReadScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SalesReadScope").field("access", &self.access).finish()
    }
}
impl SalesReadScope {
    /// 绑定本次 identity 编译结果和同一服务。
    /// # 参数
    /// access 为完整销售模型凭证，authorization 为生成它的服务。
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
    /// 按关联行的实际单个销售来源执行完整授权，不预载来源集合。
    /// # 参数
    /// source_field 为关联行中保存单个销售单身份的固定业务字段。
    /// # 返回
    /// 来源存在且完整授权命中时保留关联行，不增加根记录数量。
    /// # 错误
    /// 无；执行前后由调用仓储复验凭证。
    pub fn related_stages(&self, source_field: &str) -> Vec<Document> {
        related_sales_stages(source_field, self.stages())
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
                if access.model().collection != Database::SALES_ORDERS
                    || access.model().output != QueryOutput::Object
                {
                    return Err(crate::Error::ValidationError("销售授权查询模型不匹配".into()));
                }
                service.validate(access, executor).await.map_err(identity_error)
            },
            _ => Err(crate::Error::Forbidden("缺少销售授权凭证".into())),
        }
    }
}

fn related_sales_stages(source_field: &str, authorization: Vec<Document>) -> Vec<Document> {
    let mut source = vec![doc! {"$match": {"$expr": {"$eq": ["$id", "$$sales_source_id"]}}}];
    source.extend(authorization);
    source.extend([doc! {"$project": {"_id": 0, "id": 1}}, doc! {"$limit": 1}]);
    vec![
        doc! {"$lookup": {
            "from": Database::SALES_ORDERS,
            "let": {"sales_source_id": format!("${source_field}")},
            "pipeline": source,
            "as": "__authorized_sales_source",
        }},
        doc! {"$match": {"__authorized_sales_source.0": {"$exists": true}}},
        doc! {"$unset": "__authorized_sales_source"},
    ]
}
/// 销售单责任范围查询。
#[allow(async_fn_in_trait)]
pub trait SalesOrderRepositoryScopeExt {
    /// 按两组独立授权交集批量读取关联销售责任事实。
    ///
    /// # 参数
    /// * `ids` - 待核对的销售单 ID；空集合直接返回空结果。
    /// * `sales` - 已证明的销售范围。
    /// * `resource` - 已证明的消费资源范围。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 仅返回指定 ID 内同时满足两组范围的订单；调用方负责分批和总上限。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    async fn scope_orders(
        &self,
        ids: &[String],
        sales: &SalesReadScope,
        resource: &SalesReadScope,
        executor: &mut dyn persistence_core::Executor,
    ) -> crate::Result<Vec<crate::entity::sales_order::SalesOrder>>;

    /// 只读取关联销售范围内的身份与版本，避免授权重验装载完整订单。
    ///
    /// # 参数
    /// * `ids` - 关联销售身份，调用方按批次限制大小。
    /// * `sales` - 已证明的销售范围。
    /// * `resource` - 已证明的消费资源范围。
    /// * `executor` - 调用方事务执行器。
    /// # 返回
    /// 返回同时满足两组范围的身份和版本，按身份稳定排序。
    /// # 错误
    /// 数据库查询或反序列化失败时返回仓储错误。
    async fn scope_order_versions(
        &self,
        ids: &[String],
        sales: &SalesReadScope,
        resource: &SalesReadScope,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesVersion>>;

    /// 按独立授权条件读取销售单，ID 条件不能替换范围交集。
    ///
    /// # 参数
    /// * `id` - 销售单主键
    /// * `scope` - 已证明的对象范围
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 不存在或不在范围内均返回 None。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    ///
    /// # 关键业务约束
    /// 仓储不得按登录用户自行推断权限。
    async fn find_authorized(
        &self,
        id: &str,
        scope: &SalesReadScope,
        executor: &mut dyn persistence_core::Executor,
    ) -> crate::Result<Option<crate::entity::sales_order::SalesOrder>>;
}

impl SalesOrderRepositoryScopeExt for Repository<'_, crate::entity::sales_order::SalesOrder> {
    async fn scope_orders(
        &self,
        ids: &[String],
        sales: &SalesReadScope,
        resource: &SalesReadScope,
        executor: &mut dyn persistence_core::Executor,
    ) -> crate::Result<Vec<crate::entity::sales_order::SalesOrder>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        sales.validate(executor).await?;
        resource.validate(executor).await?;
        let result = aggregate::find(
            &self.collection(),
            doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON, "$and": [
                { "id": { "$in": ids } }, sales.document(), resource.document()
            ] },
            FindOptions::default(),
            {
                let mut stages = sales.lookup_after(doc! { "deleted_at": 0, "id": { "$in": ids } });
                stages.extend(resource.stages());
                stages
            },
            executor,
        )
        .await?;
        sales.validate(executor).await?;
        resource.validate(executor).await?;
        Ok(result)
    }

    async fn scope_order_versions(
        &self,
        ids: &[String],
        sales: &SalesReadScope,
        resource: &SalesReadScope,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesVersion>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        sales.validate(executor).await?;
        resource.validate(executor).await?;
        let result = aggregate::find(
            &self.collection().clone_with_type::<SalesVersion>(),
            doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON, "$and": [
                { "id": { "$in": ids } }, sales.document(), resource.document()
            ] },
            FindOptions::builder().projection(doc! { "id": 1, "version": 1 }).sort(doc! { "id": 1 }).build(),
            {
                let mut stages = sales.lookup_after(doc! { "deleted_at": 0, "id": { "$in": ids } });
                stages.extend(resource.stages());
                stages
            },
            executor,
        )
        .await?;
        sales.validate(executor).await?;
        resource.validate(executor).await?;
        Ok(result)
    }

    async fn find_authorized(
        &self,
        id: &str,
        scope: &SalesReadScope,
        executor: &mut dyn persistence_core::Executor,
    ) -> crate::Result<Option<crate::entity::sales_order::SalesOrder>> {
        scope.validate(executor).await?;
        let rows = aggregate::find(
            &self.collection(),
            doc! { "deleted_at": 0, "$and": [{ "id": id }, scope.document()] },
            FindOptions::builder().limit(1).build(),
            scope.lookup_after(doc! { "deleted_at": 0, "id": id }),
            executor,
        )
        .await?;
        scope.validate(executor).await?;
        Ok(rows.into_iter().next())
    }
}

/// 跨页校验使用的业务身份和版本，不包含金额及其他展示字段。
#[derive(Debug, serde::Deserialize, Hash)]
pub struct SalesVersion {
    pub id: String,
    pub version: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_identity_credential_cannot_expose_sales_rows() {
        let scope = SalesReadScope::default();
        assert!(scope.is_empty());
        assert_eq!(scope.stages(), vec![doc! {"$match":{"$expr":false}}]);
        assert_eq!(
            scope.lookup_after(doc! {"customer_id":"c"}),
            vec![doc! {"$match":{"customer_id":"c"}}, doc! {"$match":{"$expr":false}}]
        );
    }

    #[test]
    fn related_query_correlates_one_source_then_preserves_every_authorization_stage() {
        let authorization = vec![
            doc! {"$lookup": {"from": "record_grants", "pipeline": [], "as": "_grants"}},
            doc! {"$match": {"$or": [{"owner_user_id": "u1"}, {"_grants.0": {"$exists": true}}]}},
            doc! {"$unset": "_grants"},
        ];
        let stages = related_sales_stages("sales_order_id", authorization.clone());
        let lookup = stages[0].get_document("$lookup").unwrap();
        assert_eq!(lookup.get_str("from").unwrap(), Database::SALES_ORDERS);
        assert_eq!(lookup.get_document("let").unwrap(), &doc! {"sales_source_id": "$sales_order_id"});
        let source = lookup.get_array("pipeline").unwrap();
        assert_eq!(
            source[0].as_document().unwrap(),
            &doc! {"$match": {"$expr": {"$eq": ["$id", "$$sales_source_id"]}}}
        );
        for (actual, expected) in source[1..=authorization.len()].iter().zip(authorization) {
            assert_eq!(actual.as_document().unwrap(), &expected);
        }
        assert_eq!(source.last().unwrap().as_document().unwrap(), &doc! {"$limit": 1});
        assert_eq!(stages[1], doc! {"$match": {"__authorized_sales_source.0": {"$exists": true}}});
        assert_eq!(stages[2], doc! {"$unset": "__authorized_sales_source"});
    }
}
