//! 履约执行任务的对象标题。
//!
//! 工作台只展示往来单据号与作业类型，不把内部主键或「草稿」状态写进标题。
//! 「草稿」是履约单据未过账的内部状态，对执行人就是待处理任务。

use std::collections::{HashMap, HashSet};

use database::{Executor, FulfillmentExt, PurchaseOrderExt, SalesOrderExt};

use super::{object_ids, ObjectFact, ObjectFactMap, ObjectKind, WorkItemService};
use crate::errors::Result;

impl WorkItemService {
    /// 装载入库、发货、电子交付与服务履约工作项的权威对象事实。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入全部存在的履约对象事实。
    ///
    /// # 错误
    /// 任一仓储查询失败时返回错误。
    ///
    /// # 关键业务约束
    /// 标题只使用作业类型与来源销售/采购单号；不得拼接履约对象主键或单据草稿状态。
    pub(super) async fn load_fulfillment_operation_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let receipt_ids = object_ids(keys, ObjectKind::PurchaseReceipt);
        let receipts = self
            .db
            .purchase_receipts()
            .list_work_item_brief_entities_by_ids(&receipt_ids, executor)
            .await?;
        let receipt_purchase_nos = purchase_order_numbers(
            self,
            receipts
                .iter()
                .map(|receipt| receipt.purchase_order_id.to_string())
                .collect(),
            executor,
        )
        .await?;
        for receipt in receipts {
            let purchase_no = receipt_purchase_nos.get(receipt.purchase_order_id.as_ref());
            facts.insert(
                (ObjectKind::PurchaseReceipt, receipt.base.id.clone()),
                ObjectFact::new(
                    receipt.purchase_order_id.to_string(),
                    fulfillment_source_label("采购入库", "采购单", purchase_no.map(String::as_str)),
                    SYSTEM_OBJECT_OWNER,
                ),
            );
        }

        let delivery_ids = object_ids(keys, ObjectKind::Delivery);
        let deliveries = self
            .db
            .deliveries()
            .list_work_item_brief_entities_by_ids(&delivery_ids, executor)
            .await?;
        let delivery_sales_nos = sales_order_numbers(
            self,
            deliveries
                .iter()
                .map(|delivery| delivery.sales_order_id.to_string())
                .collect(),
            executor,
        )
        .await?;
        for delivery in deliveries {
            let sales_no = delivery_sales_nos.get(delivery.sales_order_id.as_ref());
            facts.insert(
                (ObjectKind::Delivery, delivery.base.id.clone()),
                ObjectFact::new(
                    delivery.sales_order_id.to_string(),
                    fulfillment_source_label(
                        delivery.delivery_type.label(),
                        "销售单",
                        sales_no.map(String::as_str),
                    ),
                    SYSTEM_OBJECT_OWNER,
                ),
            );
        }

        let electronic_ids = object_ids(keys, ObjectKind::ElectronicDelivery);
        let electronics = self
            .db
            .electronic_deliveries()
            .list_work_item_brief_entities_by_ids(&electronic_ids, executor)
            .await?;
        let electronic_purchase_nos = purchase_order_numbers(
            self,
            electronics
                .iter()
                .map(|delivery| delivery.purchase_order_id.to_string())
                .collect(),
            executor,
        )
        .await?;
        for delivery in electronics {
            let purchase_no = electronic_purchase_nos.get(delivery.purchase_order_id.as_ref());
            facts.insert(
                (ObjectKind::ElectronicDelivery, delivery.base.id.clone()),
                ObjectFact::new(
                    delivery.purchase_order_id.to_string(),
                    fulfillment_source_label("电子交付", "采购单", purchase_no.map(String::as_str)),
                    SYSTEM_OBJECT_OWNER,
                ),
            );
        }

        let service_ids = object_ids(keys, ObjectKind::ServiceFulfillment);
        let services = self
            .db
            .service_fulfillments()
            .list_work_item_brief_entities_by_ids(&service_ids, executor)
            .await?;
        let service_purchase_nos = purchase_order_numbers(
            self,
            services
                .iter()
                .map(|fulfillment| fulfillment.purchase_order_id.to_string())
                .collect(),
            executor,
        )
        .await?;
        for fulfillment in services {
            let purchase_no = service_purchase_nos.get(fulfillment.purchase_order_id.as_ref());
            facts.insert(
                (ObjectKind::ServiceFulfillment, fulfillment.base.id.clone()),
                ObjectFact::new(
                    fulfillment.purchase_order_id.to_string(),
                    fulfillment_source_label("服务履约", "采购单", purchase_no.map(String::as_str)),
                    SYSTEM_OBJECT_OWNER,
                ),
            );
        }
        Ok(())
    }
}

const SYSTEM_OBJECT_OWNER: &str = "__system__";

/// 批量读取销售单号，避免按履约对象 N+1。
///
/// # 参数
/// * `service` - 工作项服务
/// * `sales_order_ids` - 销售单主键
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回销售单 ID 到单号的映射；空输入返回空表。
///
/// # 错误
/// 仓储查询失败时返回错误。
///
/// # 关键业务约束
/// 单号缺失时调用方回退为只有作业类型的标题，不得把销售单 ID 写进标题。
async fn sales_order_numbers(
    service: &WorkItemService,
    sales_order_ids: Vec<String>,
    executor: &mut dyn Executor,
) -> Result<HashMap<String, String>> {
    if sales_order_ids.is_empty() {
        return Ok(HashMap::new());
    }
    Ok(service
        .db
        .sales_orders()
        .list_work_item_brief_entities_by_ids(&sales_order_ids, executor)
        .await?
        .into_iter()
        .filter_map(|order| {
            let order_no = order.order_no.trim();
            if order_no.is_empty() {
                None
            } else {
                Some((order.base.id.clone(), order_no.to_string()))
            }
        })
        .collect())
}

/// 批量读取采购单号，避免按履约对象 N+1。
///
/// # 参数
/// * `service` - 工作项服务
/// * `purchase_order_ids` - 采购单主键
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回采购单 ID 到单号的映射；空输入返回空表。
///
/// # 错误
/// 仓储查询失败时返回错误。
///
/// # 关键业务约束
/// 单号缺失时调用方回退为只有作业类型的标题，不得把采购单 ID 写进标题。
async fn purchase_order_numbers(
    service: &WorkItemService,
    purchase_order_ids: Vec<String>,
    executor: &mut dyn Executor,
) -> Result<HashMap<String, String>> {
    if purchase_order_ids.is_empty() {
        return Ok(HashMap::new());
    }
    Ok(service
        .db
        .purchase_orders()
        .list_work_item_brief_entities_by_ids(&purchase_order_ids, executor)
        .await?
        .into_iter()
        .filter_map(|order| {
            let purchase_no = order.purchase_no.trim();
            if purchase_no.is_empty() {
                None
            } else {
                Some((order.base.id.clone(), purchase_no.to_string()))
            }
        })
        .collect())
}

/// 拼工作台履约任务标题。
///
/// # 参数
/// * `operation_label` - 作业类型中文名
/// * `source_kind` - 「销售单」或「采购单」
/// * `source_no` - 已解析的来源单号
///
/// # 返回
/// 有单号时返回「供应商直发 · 销售单 SO-1」；否则只返回作业类型。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 不附加草稿/已发货等内部状态；空单号不得回退成对象 ID。
fn fulfillment_source_label(operation_label: &str, source_kind: &str, source_no: Option<&str>) -> String {
    match source_no.map(str::trim).filter(|value| !value.is_empty()) {
        Some(source_no) => format!("{operation_label} · {source_kind} {source_no}"),
        None => operation_label.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::fulfillment_source_label;

    #[test]
    fn fulfillment_title_uses_source_document_number() {
        assert_eq!(
            fulfillment_source_label("供应商直发", "销售单", Some("SO20260826-000001")),
            "供应商直发 · 销售单 SO20260826-000001"
        );
        assert_eq!(
            fulfillment_source_label("供应商直发", "销售单", Some("  ")),
            "供应商直发"
        );
        assert_eq!(fulfillment_source_label("采购入库", "采购单", None), "采购入库");
    }
}
