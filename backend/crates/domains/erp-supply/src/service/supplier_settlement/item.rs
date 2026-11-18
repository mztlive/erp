use application_core::AuditActor;
use erp_permissions::supplier_settlement;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::SupplierSettlementService;
use super::dto::{SettlementPageView, SupplierSettlementItemListParams, SupplierSettlementItemView};
use crate::Result;
use crate::dto::supplier_fulfillment::SortDir;
use crate::repository::SupplierSettlementExt;
use crate::repository::prelude::*;
use crate::repository::supplier_settlement::SupplierSettlementItemFilter;

impl SupplierSettlementService {
    /// 分页查询供应商结算明细列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`statement_id` 扁平筛选）
    /// * `actor` - 当前认证账号。
    ///
    /// # 返回
    /// 功能资格、分页和计数共用同一快照；提交成功后返回分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn supplier_settlement_item_list(
        &self,
        params: &SupplierSettlementItemListParams,
        actor: &AuditActor,
    ) -> Result<SettlementPageView<SupplierSettlementItemView>> {
        let service = Self::new(self.db.clone()).with_data_scope(self.access());
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.item_page(&params, &actor, executor).await })
            })
            .await
    }

    /// 在原读取快照检查功能资格，并使用原执行器读取分页和计数。
    async fn item_page(
        &self,
        params: &SupplierSettlementItemListParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SettlementPageView<SupplierSettlementItemView>> {
        self.access().require_operation(actor, &supplier_settlement::ITEM_LIST, executor).await?;
        params.validate()?;
        let query = params.normalized()?;
        let filter = SupplierSettlementItemFilter {
            statement_id: query.statement_id,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page =
            self.db.supplier_settlement_items().search_supplier_settlement_items(&filter, executor).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierSettlementItemView {
                id: row.id,
                statement_id: row.statement_id.to_string(),
                supplier_fulfillment_order_id: row.supplier_fulfillment_order_id.to_string(),
                supplier_fulfillment_item_id: row.supplier_fulfillment_item_id.to_string(),
                quantity: row.quantity,
                order_amount: row.order_amount,
                freight_amount: row.freight_amount,
                service_fee_amount: row.service_fee_amount,
                refund_amount: row.refund_amount,
                erp_calculated_amount: row.erp_calculated_amount,
                erp_calculated_net_amount: row.erp_calculated_net_amount,
                erp_calculated_tax_amount: row.erp_calculated_tax_amount,
                supplier_billed_amount: row.supplier_billed_amount,
                supplier_billed_net_amount: row.supplier_billed_net_amount,
                supplier_billed_tax_amount: row.supplier_billed_tax_amount,
                created_at: row.created_at,
            })
            .collect();

        Ok(SettlementPageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;
    use serde_json::json;

    use super::*;
    use crate::Error;
    use crate::service::supplier_settlement::SettlementAccess;

    /// 原读取步骤必须先完成授权；设施失败时不得读取明细、差异或来源证据。
    #[tokio::test]
    async fn settlement_reads_stop_before_business_io_when_authorization_fails() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("unit_test");
        let access = SettlementAccess::new(db.clone(), unavailable_authorization());
        let service = SupplierSettlementService::new(db).with_data_scope(access);
        let actor = AuditActor::new("actor".into(), "operator".into(), AccountKind::Admin);
        let item_params = serde_json::from_value(json!({})).unwrap();
        let difference_params = serde_json::from_value(json!({})).unwrap();
        let source_query = serde_json::from_value(json!({
            "supplier_id": "supplier",
            "period_start": "2026-11-01",
            "period_end": "2026-11-30"
        }))
        .unwrap();
        let mut executor = SnapshotExecutor::default();
        let errors = [
            service.item_page(&item_params, &actor, &mut executor).await.expect_err("必须拒绝"),
            service.difference_page(&difference_params, &actor, &mut executor).await.expect_err("必须拒绝"),
            service.source_evidence_view(&source_query, &actor, &mut executor).await.expect_err("必须拒绝"),
        ];
        for error in errors {
            assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
        }
    }
}
