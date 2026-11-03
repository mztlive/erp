//! 只从已授权销售单沿权威外键解析当前任务来源。
mod approval;

use std::collections::HashMap;

use erp_finance::repository::ReceivableExt;
use erp_fulfillment::repository::FulfillmentExt;
use erp_procurement::repository::PurchaseOrderExt;
use erp_sales::entity::sales_order::SalesOrder;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::WorkItem;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::NoTransaction;

use crate::Result;

/// 当前办理所需的跨域只读身份查询。
pub(super) struct CurrentWorkRepository<'a> {
    pub(super) db: &'a Database,
}

/// 关联任务及其已授权来源单据的展示编号。
pub(super) struct CurrentWorkTasks {
    pub(super) tasks: Vec<WorkItem>,
    pub(super) purchase_numbers: HashMap<String, String>,
}

impl CurrentWorkRepository<'_> {
    /// 读取已授权销售单及其直接来源对象的开放任务。
    /// # 参数
    /// `order` 为调用方已授权的销售单。
    /// # 返回
    /// 返回按创建时间、任务 ID 稳定排序的开放任务。
    /// # 错误
    /// 任一来源或任务查询失败时返回错误。
    pub(super) async fn tasks(&self, order: &SalesOrder) -> Result<CurrentWorkTasks> {
        let mut purchase_numbers = HashMap::new();
        let objects = self.objects(order, &mut purchase_numbers).await?;
        // 复用 business_object_type + business_object_id 索引；每个分支均限定来源。
        let tasks = self
            .db
            .work_items()
            .find_many_sorted(
                doc! { "status": "OPEN", "$or": objects },
                doc! { "created_at": 1, "id": 1 },
                &mut NoTransaction,
            )
            .await?;
        Ok(CurrentWorkTasks { tasks: self.current_approvals(tasks).await?, purchase_numbers })
    }

    /// 按本单销售、采购、交付与应收关系构建对象查询，不扫描全量工作队列。
    /// # 参数
    /// `order` 为已经过销售详情范围授权的单据。
    /// # 返回
    /// 返回绑定本单及其关联身份的查询分支；空来源保持空匹配。
    /// # 错误
    /// 关联仓储读取失败时返回错误。
    async fn objects(
        &self,
        order: &SalesOrder,
        purchase_numbers: &mut HashMap<String, String>,
    ) -> Result<Vec<Document>> {
        let sale = &order.base.id;
        let mut objects = vec![doc! {
            "business_object_type": { "$in": ["sales_order", "voucher_sales_order"] },
            "business_object_id": sale,
        }];
        if order.current_revision_id().is_none() {
            return Ok(objects);
        }
        let purchases =
            self.db.purchase_orders().find_many(doc! { "sales_order_id": sale }, &mut NoTransaction).await?;
        purchase_numbers
            .extend(purchases.iter().map(|order| (order.base.id.clone(), order.purchase_no.clone())));
        let purchase_ids = purchases.into_iter().map(|row| row.base.id).collect::<Vec<_>>();
        objects.push(object_filter("purchase_order", purchase_ids.clone()));
        let deliveries =
            self.db.deliveries().find_many(doc! { "sales_order_id": sale }, &mut NoTransaction).await?;
        objects.push(object_filter("delivery", deliveries.into_iter().map(|row| row.base.id).collect()));
        self.fulfillment_objects(&purchase_ids, &mut objects).await?;
        let accounts = self
            .db
            .receivable_accounts()
            .find_many(doc! { "sales_order_id": sale }, &mut NoTransaction)
            .await?;
        objects
            .push(object_filter("receivable_account", accounts.into_iter().map(|row| row.base.id).collect()));
        let requests = self
            .db
            .sales_invoice_requests()
            .find_many(doc! { "sales_order_id": sale }, &mut NoTransaction)
            .await?;
        objects.push(object_filter(
            "sales_invoice_request",
            requests.into_iter().map(|row| row.base.id).collect(),
        ));
        Ok(objects)
    }

    /// 加载本单采购对应的入库、电子交付和服务交付身份，不输出其业务内容。
    async fn fulfillment_objects(&self, purchases: &[String], objects: &mut Vec<Document>) -> Result<()> {
        if purchases.is_empty() {
            return Ok(());
        }
        let filter = doc! { "purchase_order_id": { "$in": purchases } };
        let receipts = self.db.purchase_receipts().find_many(filter.clone(), &mut NoTransaction).await?;
        objects
            .push(object_filter("purchase_receipt", receipts.into_iter().map(|row| row.base.id).collect()));
        let electronic =
            self.db.electronic_deliveries().find_many(filter.clone(), &mut NoTransaction).await?;
        objects.push(object_filter(
            "electronic_delivery",
            electronic.into_iter().map(|row| row.base.id).collect(),
        ));
        let services = self.db.service_fulfillments().find_many(filter, &mut NoTransaction).await?;
        objects.push(object_filter(
            "service_fulfillment",
            services.into_iter().map(|row| row.base.id).collect(),
        ));
        Ok(())
    }
}

/// 空身份列表保持空匹配，防止扩大到整个对象类型。
fn object_filter(kind: &str, ids: Vec<String>) -> Document {
    doc! { "business_object_type": kind, "business_object_id": { "$in": ids } }
}
