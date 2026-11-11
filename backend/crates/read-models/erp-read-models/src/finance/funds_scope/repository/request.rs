//! 开票申请在数据库关联授权与业务条件后读取完整有界结果。
use erp_finance::dto::receivable::InvoiceRequestQuery;
use erp_finance::entity::receivable::SalesInvoiceRequest;
use erp_finance::repository::ReceivableExt;
use erp_finance::repository::prelude::*;
use erp_sales::repository::SalesOrderExt;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::Executor;

use super::super::{FundsAccess, FundsAuthorization};
use crate::Result;
impl FundsAccess {
    pub(in crate::finance::funds_scope) async fn request_candidates(
        &self,
        query: &InvoiceRequestQuery,
        authorization: &FundsAuthorization,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesInvoiceRequest>> {
        authorization.validate(executor).await?;
        let result: crate::Result<_> = async {
        let condition = self.request_condition(query, executor).await?;
        let filter = doc! { "$and": [authorization.sales.document(),
        super::linked_condition_document(&condition, "sales_owner_user_id")] };
        let mut stages = super::source_stages(
            <Database as SalesOrderExt>::SALES_ORDERS,
            "$sales_order_id".into(),
            "_source",
            filter,
            "sales_owner_user_id",
            "order_no",
            &authorization.sales.stages(),
        );
        stages.push(doc! { "$match": { "_source_allowed": true } });
        if let Some(ids) = condition.operator_user_ids {
            stages.push(doc! { "$match": { "created_by": { "$in": ids } } });
        }
        if let Some(ids) = condition.secondary_operator_user_ids {
            stages.push(doc! { "$lookup": { "from": "work_items", "localField": "work_item_id", "foreignField": "id", "as": "_task", "pipeline": [{ "$match": handler_filter(&ids) }] } });
            stages.push(doc! { "$match": { "_task.0": { "$exists": true } } });
        }
        super::super::request::bounded_request_candidates(
            self.db.sales_invoice_requests().scope_candidates(query, stages, executor).await?,
        )

    }.await;
        let result = result?;
        authorization.validate(executor).await?;
        Ok(result)
    }
}

/// 工作项处理人采用负责人优先、缺负责人时回退完成人，与展示事实保持一致。
fn handler_filter(ids: &[String]) -> Document {
    doc! { "deleted_at": 0, "$expr": { "$in": [
        { "$ifNull": ["$owner_user_id", "$completed_by"] }, ids,
    ] } }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::Bson;

    use super::*;

    /// 负责人优先；仅空值或缺失才回退完成人，不把两人同时当作处理人。
    #[test]
    fn handler_condition_keeps_owner_precedence_and_completed_fallback() {
        let filter = handler_filter(&["handler".into()]);
        assert_eq!(filter.get_i32("deleted_at").unwrap(), 0);
        let operands = filter.get_document("$expr").unwrap().get_array("$in").unwrap();
        assert_eq!(operands[0], Bson::Document(doc! { "$ifNull": ["$owner_user_id", "$completed_by"] }));
        assert_eq!(operands[1], Bson::Array(vec![Bson::String("handler".into())]));
        let empty = handler_filter(&[]);
        assert!(
            empty.get_document("$expr").unwrap().get_array("$in").unwrap()[1].as_array().unwrap().is_empty()
        );
    }
}
