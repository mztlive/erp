//! 履约公共事实：批次、业务数量、物流与履约凭证；所有可读角色使用同一装载路径。
use super::brief::{format_instant_datetime, line_title, push_section, BriefLine, BriefSection};
use super::{object_ids, ObjectKind, WorkbenchObjectFactMap, WorkbenchReadService};
use crate::errors::Result;
use erp_core::money::Quantity;
use erp_fulfillment::entity::fulfillment::{
    Delivery, ElectronicDelivery, ElectronicDeliveryState, PurchaseReceipt, PurchaseReceiptLine,
    ServiceFulfillment, ServiceFulfillmentState,
};
use erp_fulfillment::repository::FulfillmentExt;
use erp_procurement::repository::PurchaseOrderExt;
use erp_sales::repository::SalesOrderExt;
use erp_warehouse::repository::WarehouseExt;
use persistence_core::Executor;
use std::collections::{HashMap, HashSet};

type Names = HashMap<String, (String, Option<String>)>;

impl<A: erp_workflow::WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 批量追加四种履约单据的公共业务字段；缺失字段明确显示待登记，不展示内部身份。
    pub(super) async fn load_fulfillment_details(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.receipt_details(keys, facts, executor).await?;
        self.delivery_details(keys, facts, executor).await?;
        self.electronic_details(keys, facts, executor).await?;
        self.service_details(keys, facts, executor).await
    }

    /// 入库数量和质检结果使用实际入库行，不用来源订单数量替代。
    async fn receipt_details(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::PurchaseReceipt);
        if ids.is_empty() {
            return Ok(());
        }
        let rows = self
            .db
            .purchase_receipts()
            .list_active_by_ids(&ids, executor)
            .await?;
        let receipt_ids = ids.iter().cloned().map(Into::into).collect::<Vec<_>>();
        let lines = self
            .db
            .fulfillment()
            .receipt_lines_by_receipt_ids(&receipt_ids, executor)
            .await?;
        let names = self.receipt_line_names(&lines, executor).await?;
        let warehouse_ids = rows
            .iter()
            .map(|r| r.warehouse_id.to_string())
            .collect::<Vec<_>>();
        let warehouses = self.warehouse_labels(&warehouse_ids, executor).await?;
        for row in rows {
            let fields = receipt_fields(&row, &warehouses);
            let visible = lines
                .iter()
                .filter(|l| l.purchase_receipt_id.as_ref() == row.base.id)
                .map(|l| receipt_line(l, &names))
                .collect();
            apply(facts, ObjectKind::PurchaseReceipt, &row.base.id, fields, visible);
        }
        Ok(())
    }

    /// 发货批次、实际发货数量和物流跟踪信息对阅读角色一致。
    async fn delivery_details(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::Delivery);
        if ids.is_empty() {
            return Ok(());
        }
        let rows = self.db.deliveries().list_active_by_ids(&ids, executor).await?;
        let delivery_ids = ids.iter().cloned().map(Into::into).collect::<Vec<_>>();
        let lines = self
            .db
            .fulfillment()
            .delivery_lines_by_delivery_ids(&delivery_ids, executor)
            .await?;
        let sales_ids = rows
            .iter()
            .map(|r| r.sales_order_id.to_string())
            .collect::<Vec<_>>();
        let names = self.sales_line_names(&sales_ids, executor).await?;
        let warehouse_ids = rows
            .iter()
            .filter_map(|r| r.warehouse_id.as_ref().map(ToString::to_string))
            .collect::<Vec<_>>();
        let warehouses = self.warehouse_labels(&warehouse_ids, executor).await?;
        for row in rows {
            let fields = delivery_fields(&row, &warehouses);
            let visible = lines
                .iter()
                .filter(|l| l.delivery_id.as_ref() == row.base.id)
                .map(|l| item_line(names.get(l.sales_order_line_id.as_ref()), l.line_no, &l.quantity))
                .collect();
            apply(facts, ObjectKind::Delivery, &row.base.id, fields, visible);
        }
        Ok(())
    }

    /// 电子交付只展示业务结果、数量、时间与凭证引用，不返回加密收件信息。
    async fn electronic_details(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::ElectronicDelivery);
        if ids.is_empty() {
            return Ok(());
        }
        let rows = self
            .db
            .electronic_deliveries()
            .list_active_by_ids(&ids, executor)
            .await?;
        let purchase_ids = rows
            .iter()
            .map(|r| r.purchase_order_id.to_string())
            .collect::<Vec<_>>();
        let names = self.purchase_sales_line_names(&purchase_ids, executor).await?;
        for row in rows {
            let fields = electronic_fields(&row);
            let line = item_line(names.get(row.sales_order_line_id.as_ref()), 1, &row.quantity);
            apply(
                facts,
                ObjectKind::ElectronicDelivery,
                &row.base.id,
                fields,
                vec![line],
            );
        }
        Ok(())
    }

    /// 服务履约共享服务数量、期间、完成说明与凭证；私密服务地址仍走原专门授权。
    async fn service_details(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::ServiceFulfillment);
        if ids.is_empty() {
            return Ok(());
        }
        let rows = self
            .db
            .service_fulfillments()
            .list_active_by_ids(&ids, executor)
            .await?;
        let purchase_ids = rows
            .iter()
            .map(|r| r.purchase_order_id.to_string())
            .collect::<Vec<_>>();
        let names = self.purchase_sales_line_names(&purchase_ids, executor).await?;
        for row in rows {
            let fields = service_fields(&row);
            let line = item_line(names.get(row.sales_order_line_id.as_ref()), 1, &row.quantity);
            apply(
                facts,
                ObjectKind::ServiceFulfillment,
                &row.base.id,
                fields,
                vec![line],
            );
        }
        Ok(())
    }

    /// 按入库行的精确采购版本读取品名与单位，避免当前版本覆盖批次来源。
    async fn receipt_line_names(
        &self,
        lines: &[PurchaseReceiptLine],
        executor: &mut dyn Executor,
    ) -> Result<Names> {
        let ids = lines
            .iter()
            .map(|l| l.purchase_order_revision_line_id.to_string())
            .collect::<Vec<_>>();
        Ok(self
            .db
            .purchase_order_revision_lines()
            .list_active_by_ids(&ids, executor)
            .await?
            .into_iter()
            .map(|l| {
                let title = line_title(
                    l.product_name_snapshot.as_deref().unwrap_or("入库明细"),
                    l.specification_snapshot.as_deref(),
                );
                (l.base.id, (title, l.base_unit_code))
            })
            .collect())
    }

    /// 从采购来源找到销售生效行，名称只用于展示。
    async fn purchase_sales_line_names(&self, ids: &[String], executor: &mut dyn Executor) -> Result<Names> {
        let orders = self
            .db
            .purchase_orders()
            .list_active_by_ids(ids, executor)
            .await?;
        self.sales_line_names(
            &orders
                .iter()
                .map(|r| r.sales_order_id.to_string())
                .collect::<Vec<_>>(),
            executor,
        )
        .await
    }

    /// 批量解析销售稳定行的当前业务名称与单位；缺失时使用行号，不展示 ID。
    async fn sales_line_names(&self, ids: &[String], executor: &mut dyn Executor) -> Result<Names> {
        let orders = self.db.sales_orders().list_active_by_ids(ids, executor).await?;
        let revisions = orders
            .iter()
            .filter_map(|r| r.stable.current_revision_id.clone().map(Into::into))
            .collect::<Vec<_>>();
        Ok(self
            .db
            .sales_order_revision_lines()
            .list_lines_by_revisions(&revisions, executor)
            .await?
            .into_iter()
            .map(|l| {
                (
                    l.sales_order_line_id.to_string(),
                    (
                        line_title(&l.item_name_snapshot, l.spec_snapshot.as_deref()),
                        l.unit_snapshot,
                    ),
                )
            })
            .collect())
    }

    /// 仓库主键仅用来批量解析名称。
    async fn warehouse_labels(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let warehouses = self.db.warehouses().list_active_by_ids(ids, executor).await?;
        let revision_ids = warehouses
            .iter()
            .filter_map(|w| w.stable.current_revision_id.clone())
            .collect::<Vec<_>>();
        let names = self
            .db
            .warehouse_revisions()
            .list_active_by_ids(&revision_ids, executor)
            .await?
            .into_iter()
            .map(|r| (r.base.id, r.name))
            .collect::<HashMap<_, _>>();
        Ok(warehouses
            .into_iter()
            .map(|w| {
                let name = w
                    .stable
                    .current_revision_id
                    .as_ref()
                    .and_then(|id| names.get(id))
                    .cloned()
                    .unwrap_or(w.warehouse_code);
                (w.base.id, name)
            })
            .collect())
    }
}

/// 履约草稿代表尚待业务处理，不将内部保存状态作为业务进度。
fn pending_status<'a>(status: &'a str, pending: &'a str) -> &'a str {
    if status == "草稿" {
        return pending;
    }
    status
}

/// 公共表头以业务单号表示本次履约批次。
fn head(number: &str, status: &str) -> Vec<BriefSection> {
    let mut fields = Vec::new();
    field(&mut fields, "履约批次", Some(number));
    field(&mut fields, "履约状态", Some(status));
    fields
}

/// 缺失业务字段明确显示尚未登记。
fn field(fields: &mut Vec<BriefSection>, label: &str, value: Option<&str>) {
    push_section(
        fields,
        label,
        Some(value.filter(|v| !v.trim().is_empty()).unwrap_or("未登记")),
        false,
    );
}

/// 凭证保留受控文件引用，界面渲染附件入口而非文件内部 ID。
fn evidence(fields: &mut Vec<BriefSection>, id: Option<&str>) {
    fields.push(BriefSection {
        label: "履约凭证".into(),
        value: id.map(|_| "查看凭证").unwrap_or("未上传").into(),
        numeric: false,
        object_id: id.map(str::to_string),
    });
}

fn qty(value: &Quantity, name: Option<&(String, Option<String>)>) -> String {
    super::brief::format_quantity(value, name.and_then(|n| n.1.as_deref()))
}

/// 产品资料缺失时保留业务行号，不把稳定主键显示为品名。
fn item_line(name: Option<&(String, Option<String>)>, line_no: u32, quantity: &Quantity) -> BriefLine {
    BriefLine {
        title: name
            .map(|n| n.0.clone())
            .unwrap_or_else(|| format!("履约明细 {line_no}")),
        quantity: Some(qty(quantity, name)),
        due_label: None,
    }
}

/// 只补充已装载的授权对象；不得以展示内容制造新对象权限。
fn apply(
    facts: &mut WorkbenchObjectFactMap,
    kind: ObjectKind,
    id: &str,
    sections: Vec<BriefSection>,
    lines: Vec<BriefLine>,
) {
    let Some(fact) = facts.get_mut(&(kind, id.to_string())) else {
        return;
    };
    let source = fact.display.brief_source.get_or_insert_with(Default::default);
    source.extra_sections.extend(sections);
    source.lines = lines;
    source.more_count = 0;
}

/// 收货批次共享已登记的仓库和过账时点。
fn receipt_fields(row: &PurchaseReceipt, warehouses: &HashMap<String, String>) -> Vec<BriefSection> {
    let mut fields = head(&row.receipt_no, pending_status(row.status.label(), "待入库"));
    field(
        &mut fields,
        "收货仓库",
        warehouses.get(row.warehouse_id.as_ref()).map(String::as_str),
    );
    field(
        &mut fields,
        "入库时间",
        row.posted_at.map(format_instant_datetime).as_deref(),
    );
    fields
}

/// 收货、合格与不合格数量分别呈现，不把零数量或质量结果省略。
fn receipt_line(row: &PurchaseReceiptLine, names: &Names) -> BriefLine {
    let name = names.get(row.purchase_order_revision_line_id.as_ref());
    BriefLine {
        title: name
            .map(|n| n.0.clone())
            .unwrap_or_else(|| format!("入库明细 {}", row.line_no)),
        quantity: Some(format!(
            "收货 {} · 合格 {} · 不合格 {}",
            qty(&row.received_quantity, name),
            qty(&row.qualified_quantity, name),
            qty(&row.rejected_quantity, name)
        )),
        due_label: Some(row.quality_result.label().into()),
    }
}

/// 发货批次固定展示业务状态与物流字段。
fn delivery_fields(row: &Delivery, warehouses: &HashMap<String, String>) -> Vec<BriefSection> {
    let mut fields = head(&row.delivery_no, pending_status(row.status.label(), "待发货"));
    field(&mut fields, "发货方式", Some(row.delivery_type.label()));
    if let Some(id) = &row.warehouse_id {
        field(
            &mut fields,
            "发货仓库",
            warehouses.get(id.as_ref()).map(String::as_str),
        );
    }
    field(&mut fields, "承运方", row.carrier.as_deref());
    field(&mut fields, "物流单号", row.tracking_no.as_deref());
    field(
        &mut fields,
        "发货时间",
        row.shipped_at.map(format_instant_datetime).as_deref(),
    );
    fields
}

/// 草稿默认结果不是已完成事实，确认前结果与完成时点均显示未登记。
fn electronic_fields(row: &ElectronicDelivery) -> Vec<BriefSection> {
    let mut fields = head(&row.fulfillment_no, pending_status(row.status.label(), "待交付"));
    let confirmed = row.status != ElectronicDeliveryState::Draft;
    field(&mut fields, "交付结果", confirmed.then_some(row.result.label()));
    field(
        &mut fields,
        "交付时间",
        confirmed
            .then(|| format_instant_datetime(row.fact.occurred_at))
            .as_deref(),
    );
    evidence(
        &mut fields,
        row.evidence_attachment_id.as_ref().map(|id| id.as_ref()),
    );
    fields
}

/// 服务详情保留期间、说明与凭证；草稿默认结果不得冒充实际结果。
fn service_fields(row: &ServiceFulfillment) -> Vec<BriefSection> {
    let mut fields = head(&row.fulfillment_no, pending_status(row.status.label(), "待履约"));
    field(
        &mut fields,
        "履约结果",
        (row.status != ServiceFulfillmentState::Draft).then_some(row.result.label()),
    );
    field(
        &mut fields,
        "服务开始",
        row.service_started_at.map(format_instant_datetime).as_deref(),
    );
    field(
        &mut fields,
        "服务结束",
        row.service_ended_at.map(format_instant_datetime).as_deref(),
    );
    field(&mut fields, "完成说明", row.completion_note.as_deref());
    evidence(
        &mut fields,
        row.evidence_attachment_id.as_ref().map(|id| id.as_ref()),
    );
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entity<T: serde::de::DeserializeOwned>(fields: serde_json::Value) -> T {
        let mut value = serde_json::to_value(entity_core::BaseModel::new("internal-id".into())).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        serde_json::from_value(value).unwrap()
    }

    /// 发货只展示业务单号和物流，私密地址与内部关联不得进入公共字段。
    #[test]
    fn delivery_fields_are_business_facts_without_private_address() {
        let row: Delivery = entity(
            json!({"delivery_no":"DL-001", "delivery_type":"SUPPLIER_DIRECT", "sales_order_id":"sales-id", "status":"DRAFT",
            "carrier":"顺丰", "tracking_no":"SF123", "address_snapshot_encrypted":"secret-cipher"}),
        );
        let fields = delivery_fields(&row, &HashMap::new());
        assert!(fields.iter().any(|s| s.value == "SF123"));
        assert!(fields
            .iter()
            .any(|s| s.label == "履约状态" && s.value == "待发货"));
        assert!(fields
            .iter()
            .any(|s| s.label == "履约批次" && s.value == "DL-001"));
        assert!(!format!("{fields:?}").contains("secret-cipher"));
        assert!(!format!("{fields:?}").contains("sales-id"));
    }

    /// 入库三种数量均按实际行保留，包含零值和业务单位。
    #[test]
    fn receipt_keeps_received_qualified_rejected_quantities() {
        let row: PurchaseReceiptLine = entity(
            json!({"purchase_receipt_id":"receipt", "line_no":1, "purchase_order_revision_line_id":"revision-line", "received_quantity":"2", "qualified_quantity":"2", "rejected_quantity":"0", "quality_result":"PASSED"}),
        );
        let names = Names::from([("revision-line".into(), ("龙井礼盒".into(), Some("盒".into())))]);
        let line = receipt_line(&row, &names);
        assert_eq!(line.title, "龙井礼盒");
        assert_eq!(
            line.quantity.as_deref(),
            Some("收货 2 盒 · 合格 2 盒 · 不合格 0 盒")
        );
        assert_eq!(receipt_line(&row, &Names::new()).title, "入库明细 1");
    }

    /// 两类履约草稿不得把默认成功结果和创建时间当成已履约事实。
    #[test]
    fn electronic_and_service_drafts_do_not_claim_completion() {
        let fields = json!({"fulfillment_no":"F-001", "sales_order_line_id":"line", "purchase_order_id":"purchase", "purchase_line_sales_allocation_id":"allocation",
            "recipient_snapshot":"secret", "recipient_snapshot_fingerprint":"hash", "quantity":"1", "result":"SUCCESS", "evidence_attachment_id":"file-id",
            "status":"DRAFT", "fact_no":"F-001", "occurred_at":1700000000, "recorded_at":1700000000, "recorded_by":"operator", "source_type":"erp",
            "service_location_encrypted":"private", "service_location_fingerprint":"hash", "completion_note":"已完成服务"});
        let mut electronic: ElectronicDelivery = entity(fields.clone());
        let mut service: ServiceFulfillment = entity(fields);
        assert!(electronic_fields(&electronic)
            .iter()
            .any(|s| s.label == "交付结果" && s.value == "未登记"));
        assert!(electronic_fields(&electronic)
            .iter()
            .any(|s| s.label == "交付时间" && s.value == "未登记"));
        assert!(service_fields(&service)
            .iter()
            .any(|s| s.label == "履约结果" && s.value == "未登记"));
        electronic.status = ElectronicDeliveryState::Confirmed;
        service.status = ServiceFulfillmentState::Confirmed;
        assert!(electronic_fields(&electronic)
            .iter()
            .any(|s| s.label == "交付结果" && s.value == "成功"));
        assert!(service_fields(&service)
            .iter()
            .any(|s| s.label == "完成说明" && s.value == "已完成服务"));
        let evidence = service_fields(&service)
            .into_iter()
            .find(|s| s.label == "履约凭证")
            .unwrap();
        assert_eq!(evidence.value, "查看凭证");
        assert_eq!(evidence.object_id.as_deref(), Some("file-id"));
        assert!(!format!("{:?}", service_fields(&service)).contains("private"));
    }
}
