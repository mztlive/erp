//! 费用采购来源：以采购创建资格读取销售协同范围，不要求销售菜单权限。
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::ids::{SalesOrderRevisionId, SalesOrderRevisionLineId};
use erp_identity::AuthorizationService;
use erp_permissions::internal;
use erp_sales::entity::sales_order::{BusinessType, CommercialStatus, SalesOrder};
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_sales::repository::sales_order::scope::SalesReadScope;
use erp_sales::repository::sales_order::{SalesOrderFilter, SalesOrderRow};
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::sales_center::access::{SalesAccess, SalesReadContext};
use crate::sales_center::order::SalesOrderReadService;
use crate::support::{data_scope_changed, ensure_deep_page};
use crate::{Error, Result};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ExpenseSourceParams {
    pub q: Option<String>,
    pub page: Option<u64>,
    pub scope_version: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ExpenseSourceSummary {
    pub id: String,
    pub order_no: String,
    pub customer_name: Option<String>,
    pub status_label: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ExpenseSourceList {
    pub items: Vec<ExpenseSourceSummary>,
    pub total: i64,
    pub page: u64,
    pub page_size: u32,
    pub scope_version: String,
    pub empty_reason: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct ExpenseSourceLine {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct ExpenseSourceDetail {
    #[serde(flatten)]
    pub summary: ExpenseSourceSummary,
    pub lines: Vec<ExpenseSourceLine>,
}

/// 费用采购候选与提交共用的来源资格读取器。
#[derive(Clone)]
pub struct ExpenseSourceReadService {
    db: Database,
    rbac: Arc<AuthorizationService>,
}

impl ExpenseSourceReadService {
    /// 绑定来源数据与当前授权服务。
    /// # 参数
    /// `db` 为业务库，`rbac` 为当前授权服务。
    /// # 返回
    /// 不执行查询的读取器。
    /// # 错误
    /// 无。
    pub fn new(db: Database, rbac: Arc<AuthorizationService>) -> Self {
        Self { db, rbac }
    }

    async fn scope(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(SalesReadContext, SalesReadScope)> {
        let (access, scope) = SalesAccess::new(self.db.clone(), self.rbac.clone())
            .resolve(actor, &internal::PURCHASE_EXPENSE_SOURCE, executor)
            .await?;
        Ok((access, scope))
    }

    /// 分页搜索可追加费用的销售单摘要。
    /// # 参数
    /// `params` 为关键词、页码及范围版本，`actor` 为当前采购人员。
    /// # 返回
    /// 已授权、已生效的实物服务单；已关闭单仍可选。
    /// # 错误
    /// 权限、范围版本、参数或读取失败时拒绝。
    pub async fn list(&self, params: &ExpenseSourceParams, actor: &AuditActor) -> Result<ExpenseSourceList> {
        let service = self.clone();
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.list_page(&params, &actor, executor).await })
            })
            .await
    }

    async fn list_page(
        &self,
        params: &ExpenseSourceParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ExpenseSourceList> {
        let page = params.page.unwrap_or(1);
        if page == 0 || params.q.as_ref().is_some_and(|q| q.chars().count() > 100) {
            return Err(Error::ValidationError("页码须大于零，搜索内容不能超过 100 字".into()));
        }
        ensure_deep_page(page, params.scope_version.as_deref())?;
        let (access, scope) = self.scope(actor, executor).await?;
        let version = access.scope_version.clone();
        if params.scope_version.as_ref().is_some_and(|expected| expected != &version) {
            return Err(data_scope_changed("数据范围已变化，请从第一页刷新"));
        }
        let sales = SalesOrderReadService::new(self.db.clone(), self.rbac.clone());
        let filter = SalesOrderFilter {
            search: sales.keyword_search(params.q.as_deref(), executor).await?,
            business_type: Some(BusinessType::GoodsService),
            commercial_status: Some(CommercialStatus::Effective),
            page,
            page_size: 20,
            ..Default::default()
        };
        let result = self.db.sales_orders().search_sales_orders(&filter, &scope, executor).await?;
        let names = sales
            .customer_names_batch(
                &result.items.iter().map(|row| row.customer_id.clone()).collect::<Vec<_>>(),
                executor,
            )
            .await?;
        let items = result
            .items
            .into_iter()
            .map(|row| {
                let name = names.get(&row.customer_id).cloned();
                summary(row, name)
            })
            .collect();
        access.validate(executor).await?;
        Ok(ExpenseSourceList {
            items,
            total: result.total,
            page,
            page_size: 20,
            scope_version: version,
            empty_reason: None,
        })
    }

    /// 在调用方执行器内重验来源资格；写命令必须传原事务。
    /// # 参数
    /// `id` 为来源销售单，`actor` 为操作人，`executor` 为调用方执行器。
    /// # 返回
    /// 已授权且适用费用采购的销售单。
    /// # 错误
    /// 不可见与不存在统一拒绝；状态不适用时返回业务错误。
    pub async fn require_order(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SalesOrder> {
        let (access, scope) = self.scope(actor, executor).await?;
        let order = self.authorized_order(id, &scope, executor).await?;
        access.validate(executor).await?;
        Ok(order)
    }

    /// 在已经签发的同一查询阶段读取来源，不另开授权阶段。
    async fn authorized_order(
        &self,
        id: &str,
        scope: &SalesReadScope,
        executor: &mut dyn Executor,
    ) -> Result<SalesOrder> {
        let order = self
            .db
            .sales_orders()
            .find_authorized(id, scope, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在或不在费用采购可选范围内".into()))?;
        order.ensure_extra_cost_source()?;
        Ok(order)
    }

    /// 读取当前生效版本中可归属费用的明细，不返回销售价格或审批资料。
    /// # 参数
    /// `id` 为候选销售单，`actor` 为当前操作人。
    /// # 返回
    /// 来源摘要及数量大于零的实物服务明细。
    /// # 错误
    /// 来源不可选、版本缺失、读取失败或期间授权变化时拒绝。
    pub async fn detail(&self, id: &str, actor: &AuditActor) -> Result<ExpenseSourceDetail> {
        let service = self.clone();
        let id = id.to_string();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.detail_view(&id, &actor, executor).await })
            })
            .await
    }

    async fn detail_view(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ExpenseSourceDetail> {
        let (access, scope) = self.scope(actor, executor).await?;
        let order = self.authorized_order(id, &scope, executor).await?;
        let lines = self.detail_lines(&order, executor).await?;
        let names = SalesOrderReadService::new(self.db.clone(), self.rbac.clone())
            .customer_names_batch(&[order.customer_id.to_string()], executor)
            .await?;
        access.validate(executor).await?;
        Ok(ExpenseSourceDetail {
            summary: ExpenseSourceSummary {
                id: order.base.id,
                order_no: order.order_no,
                customer_name: names.get(order.customer_id.as_ref()).cloned(),
                status_label: if order.closed_at.is_some() { "已关闭" } else { "已生效" },
            },
            lines,
        })
    }

    /// 仅从本次授权快照的当前版本读取可归属费用的业务行。
    async fn detail_lines(
        &self,
        order: &SalesOrder,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ExpenseSourceLine>> {
        let revision = order
            .stable
            .current_revision_id
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("销售单缺少当前生效版本".into()))?;
        let lines = self
            .db
            .sales_order_revision_lines()
            .list_lines_by_revision(&SalesOrderRevisionId::new(revision), executor)
            .await?;
        let ids = lines.iter().map(|line| SalesOrderRevisionLineId::new(&line.base.id)).collect::<Vec<_>>();
        let goods = self
            .db
            .sales_order_goods_service_line_revisions()
            .list_by_revision_line_ids(&ids, executor)
            .await?;
        Ok(lines
            .into_iter()
            .filter(|line| {
                goods.iter().any(|goods| {
                    goods.revision_line_id.as_ref() == line.base.id
                        && goods.quantity.to_decimal() > Decimal::ZERO
                })
            })
            .map(|line| ExpenseSourceLine {
                id: line.sales_order_line_id.to_string(),
                name: line.item_name_snapshot,
            })
            .collect())
    }
}

fn summary(row: SalesOrderRow, customer_name: Option<String>) -> ExpenseSourceSummary {
    ExpenseSourceSummary {
        id: row.id,
        order_no: row.order_no,
        customer_name,
        status_label: if row.closed_at.is_some() { "已关闭" } else { "已生效" },
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unbound_authorization;
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    #[tokio::test]
    async fn expense_sources_stop_before_business_reads_when_mapping_is_missing() {
        let client = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let db = client.database("expense_source_authorization");
        let service = ExpenseSourceReadService::new(db, unbound_authorization());
        let actor = AuditActor::new("buyer".into(), "buyer".into(), AccountKind::Admin);
        let errors = [
            service
                .detail_view("sales", &actor, &mut SnapshotExecutor::default())
                .await
                .expect_err("详情不得读取未授权销售资料"),
            service
                .require_order("sales", &actor, &mut SnapshotExecutor::default())
                .await
                .expect_err("写入来源不得读取未授权销售资料"),
        ];
        for error in errors {
            assert!(matches!(error, Error::ValidationError(reason) if reason == "缺少查询 Mapping 绑定"));
        }
    }
}
