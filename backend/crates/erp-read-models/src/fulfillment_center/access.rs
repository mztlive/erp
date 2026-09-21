//! 验收独立入口沿来源销售单重验范围，分页前应用授权条件。
use application_core::AuditActor;
use erp_fulfillment::dto::{
    CustomerAcceptanceDetailView, CustomerAcceptanceListParams, CustomerAcceptanceView, PageView,
};
use erp_fulfillment::service::FulfillmentService;
use erp_identity::SharedRbacService;
use mongodb::Database;
use persistence_core::Transactional;

use crate::Result;
use crate::sales_center::access::SalesAccess;

/// 客户验收读取授权入口。
#[derive(Clone)]
pub struct AcceptanceReadService {
    db: Database,
    rbac: SharedRbacService,
}

impl AcceptanceReadService {
    /// 装配来源订单授权。
    /// # 参数
    /// `db` 为数据库，`rbac` 为当前权限服务。
    /// # 返回
    /// 不缓存授权结论的读取服务。
    /// # 错误
    /// 无。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 按当前来源范围查询验收列表。
    /// # 参数
    /// `params` 为业务条件，`actor` 为认证操作人。
    /// # 返回
    /// 授权过滤后的分页与总数。
    /// # 错误
    /// 无动作权限、范围解析或查询失败时拒绝。
    pub async fn list(
        &self,
        params: &CustomerAcceptanceListParams,
        actor: &AuditActor,
    ) -> Result<PageView<CustomerAcceptanceView>> {
        let this = self.clone();
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let access = SalesAccess::new(this.db.clone(), this.rbac);
                    let (_, scope) = access.resolve(&actor, "list", &[], executor).await?;
                    let ids = access.authorized_source_ids(&scope, executor).await?;
                    Ok(FulfillmentService::new(this.db)
                        .customer_acceptance_list(&params, ids, executor)
                        .await?)
                })
            })
            .await
    }

    /// 独立详情在同一事务重验来源销售单。
    /// # 参数
    /// `id` 为验收身份，`actor` 为认证操作人。
    /// # 返回
    /// 已授权详情。
    /// # 错误
    /// 来源不可读时拒绝，不返回验收内容。
    pub async fn detail(&self, id: &str, actor: &AuditActor) -> Result<CustomerAcceptanceDetailView> {
        let this = self.clone();
        let id = id.to_string();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let detail = FulfillmentService::new(this.db.clone())
                        .load_customer_acceptance(&id, executor)
                        .await?;
                    SalesAccess::new(this.db, this.rbac)
                        .require_object(&actor, "detail", &detail.acceptance.sales_order_id, &[], executor)
                        .await?;
                    Ok(detail)
                })
            })
            .await
    }
    /// 在来源详情授权的同一事务读取可验收事实。
    /// # 参数
    /// `id` 为销售单，`actor` 为认证操作人。
    /// # 返回
    /// 当前可验收数量与历史。
    /// # 错误
    /// 无读取资格或事实读取失败时拒绝。
    pub async fn eligibility(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<super::dto::AcceptanceEligibilityView> {
        let this = self.clone();
        let id = id.to_string();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    SalesAccess::new(this.db.clone(), this.rbac)
                        .require_object(&actor, "detail", &id, &[], executor)
                        .await?;
                    super::FulfillmentReadService::new(this.db)
                        .load_acceptance_eligibility(&id, executor)
                        .await
                })
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::SalesOrderId;
    use erp_fulfillment::repository::fulfillment::CustomerAcceptanceFilter;
    use persistence_core::QueryFilter;
    use serde_json::json;
    use test_support::matches_filter;

    #[test]
    fn acceptance_source_query_never_overrides_authorization_and_empty_denies() {
        let mut filter = CustomerAcceptanceFilter {
            authorized_sales_order_ids: Some(vec!["allowed".into()]),
            sales_order_id: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let allowed = json!({"sales_order_id":"allowed", "deleted_at":0});
        let other = json!({"sales_order_id":"other", "deleted_at":0});
        assert!(matches_filter(&filter.to_doc(), &allowed));
        assert!(!matches_filter(&filter.to_doc(), &other));
        filter.sales_order_id = Some(SalesOrderId::new("other"));
        assert!(!matches_filter(&filter.to_doc(), &allowed));
        assert!(!matches_filter(&filter.to_doc(), &other));
        filter.sales_order_id = None;
        filter.authorized_sales_order_ids = Some(vec![]);
        assert!(!matches_filter(&filter.to_doc(), &allowed));
        filter.authorized_sales_order_ids = None;
        assert!(matches_filter(&filter.to_doc(), &other));
    }
}
