//! 已授权开放履约任务的当前供给提示；只沿实际履约对象关联采购，不推断选源。

use std::collections::{BTreeSet, HashMap, HashSet};

use erp_core::ids::PurchaseOrderId;
use erp_fulfillment::entity::fulfillment::{
    Delivery, DeliveryState, DeliveryType, ElectronicDelivery, ElectronicDeliveryState, PurchaseReceipt,
    PurchaseReceiptState, ServiceFulfillment, ServiceFulfillmentState,
};
use erp_fulfillment::repository::FulfillmentExt;
use erp_procurement::entity::purchase_order::PurchaseOrder;
use erp_sales::entity::sales_order::SalesOrder;
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use erp_workflow::{WorkItemExt, WorkflowAuthorizationPort};
use persistence_core::Executor;

use super::brief::push_section;
use super::dto::WorkItemFields;
use super::facts::object_policy;
use super::{ObjectKind, WorkbenchReadService, object_ids};
use crate::errors::Result;
use crate::supplier_portal::PurchaseSupplyWarnings;

pub(super) type FulfillmentSupplySources = HashMap<(ObjectKind, String), FulfillmentSupplySource>;

/// 只保留当前草稿的实际采购关联与对象版本，用于展示前复验任务引用。
pub(super) struct FulfillmentSupplySource {
    purchase_order_id: PurchaseOrderId,
    version: u64,
    owner_role: &'static str,
    reason_code: &'static str,
    responsibility_key: String,
    warehouse_id: Option<String>,
    sales_order_id: Option<String>,
    validated_task_ids: HashSet<String>,
}

impl<A: WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 批量读取已授权开放任务所指向的当前履约草稿及其真实采购关联。
    ///
    /// # 参数
    /// * `keys` - 已授权开放履约任务的对象键
    /// * `executor` - 与任务授权共用的仓储执行器
    ///
    /// # 返回
    /// 当前草稿的采购关联；已完成、已冲正及仓库发货不返回关联。
    ///
    /// # 错误
    /// 任一履约对象读取失败时返回错误。
    pub(super) async fn current_fulfillment_supply_sources(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        executor: &mut dyn Executor,
    ) -> Result<FulfillmentSupplySources> {
        let mut sources = FulfillmentSupplySources::new();
        self.receipt_supply_sources(keys, &mut sources, executor).await?;
        self.delivery_supply_sources(keys, &mut sources, executor).await?;
        self.electronic_supply_sources(keys, &mut sources, executor).await?;
        self.service_supply_sources(keys, &mut sources, executor).await?;
        Ok(sources)
    }

    /// 用同一授权时点的任务实体、采购及销售来源复验冻结责任身份。
    ///
    /// # 参数
    /// * `fields` - 已授权当前页字段，不得扩大为请求中的任意任务
    /// * `sources` - 这些字段引用的实际履约草稿
    /// * `executor` - 与授权共用的执行器
    ///
    /// # 返回
    /// 为冻结身份、对象版本及实际来源均一致的任务标记可展示提示。
    ///
    /// # 错误
    /// 任务、采购或销售批量读取失败时返回错误。
    pub(super) async fn validate_fulfillment_supply_sources(
        &self,
        fields: &[WorkItemFields],
        sources: &mut FulfillmentSupplySources,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let task_ids = fields
            .iter()
            .filter(|fields| open_fulfillment_kind(fields).is_some())
            .map(|fields| fields.id.clone())
            .collect::<Vec<_>>();
        let tasks = self.db.work_items().list_active_by_ids(&task_ids, executor).await?;
        let ids = sources.values().map(|source| source.purchase_order_id.to_string()).collect::<Vec<_>>();
        let orders = self.facts_reader().read_purchase_orders(&ids, executor).await?;
        let ids = orders.iter().map(|order| order.sales_order_id.to_string()).collect::<Vec<_>>();
        let sales = self.facts_reader().read_sales_orders(&ids, executor).await?;
        validate_source_tasks(fields, sources, &tasks, &orders, &sales);
        Ok(())
    }

    /// 按正式采购责任人批量解析姓名，不使用仓储或履约处理人代替采购责任。
    ///
    /// # 参数
    /// * `warnings` - 已授权采购的当前提示及正式采购责任身份
    /// * `executor` - 与对象授权共用的执行器
    ///
    /// # 返回
    /// 正式采购责任人身份到显示姓名的映射；无法解析的姓名不回退账号 ID。
    ///
    /// # 错误
    /// 账号事实批量读取失败时返回错误。
    pub(super) async fn supply_warning_owner_names(
        &self,
        warnings: &HashMap<String, PurchaseSupplyWarnings>,
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let ids = warnings
            .values()
            .filter(|warnings| supply_warning_summary(warnings).is_some())
            .filter_map(|warnings| warnings.purchase_order_owner_user_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        Ok(self
            .auth
            .load_accounts(&ids, executor)
            .await?
            .into_iter()
            .filter(|account| !account.display_name.trim().is_empty())
            .map(|account| (account.id, account.display_name))
            .collect())
    }

    /// 采购入库只沿当前收货草稿的真实采购单读取提示。
    async fn receipt_supply_sources(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        sources: &mut FulfillmentSupplySources,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::PurchaseReceipt);
        if ids.is_empty() {
            return Ok(());
        }
        for row in self.db.purchase_receipts().list_active_by_ids(&ids, executor).await? {
            if let Some(source) = FulfillmentSupplySource::from_receipt(&row) {
                sources.insert((ObjectKind::PurchaseReceipt, row.base.id), source);
            }
        }
        Ok(())
    }

    /// 直发只沿发货草稿的采购关联读取，禁止按来源销售单或 SKU 猜测。
    async fn delivery_supply_sources(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        sources: &mut FulfillmentSupplySources,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::Delivery);
        if ids.is_empty() {
            return Ok(());
        }
        for row in self.db.deliveries().list_active_by_ids(&ids, executor).await? {
            if let Some(source) = FulfillmentSupplySource::from_delivery(&row) {
                sources.insert((ObjectKind::Delivery, row.base.id), source);
            }
        }
        Ok(())
    }

    /// 电子交付仅为当前未确认草稿装载实际采购关联。
    async fn electronic_supply_sources(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        sources: &mut FulfillmentSupplySources,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::ElectronicDelivery);
        if ids.is_empty() {
            return Ok(());
        }
        for row in self.db.electronic_deliveries().list_active_by_ids(&ids, executor).await? {
            if let Some(source) = FulfillmentSupplySource::from_electronic(&row) {
                sources.insert((ObjectKind::ElectronicDelivery, row.base.id), source);
            }
        }
        Ok(())
    }

    /// 服务履约仅为当前未确认草稿装载实际采购关联。
    async fn service_supply_sources(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        sources: &mut FulfillmentSupplySources,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::ServiceFulfillment);
        if ids.is_empty() {
            return Ok(());
        }
        for row in self.db.service_fulfillments().list_active_by_ids(&ids, executor).await? {
            if let Some(source) = FulfillmentSupplySource::from_service(&row) {
                sources.insert((ObjectKind::ServiceFulfillment, row.base.id), source);
            }
        }
        Ok(())
    }
}

impl FulfillmentSupplySource {
    fn from_receipt(row: &PurchaseReceipt) -> Option<Self> {
        (row.status == PurchaseReceiptState::Draft).then(|| Self {
            purchase_order_id: row.purchase_order_id.clone(),
            version: row.base.version,
            owner_role: "warehouse_inbound_handler",
            reason_code: "PURCHASE_RECEIPT_READY",
            responsibility_key: format!("warehouse:{}:receipt", row.warehouse_id),
            warehouse_id: Some(row.warehouse_id.to_string()),
            sales_order_id: None,
            validated_task_ids: HashSet::new(),
        })
    }

    fn from_delivery(row: &Delivery) -> Option<Self> {
        if row.status != DeliveryState::Draft || row.delivery_type != DeliveryType::SupplierDirect {
            return None;
        }
        Some(Self {
            purchase_order_id: row.purchase_order_id.clone()?,
            version: row.base.version,
            owner_role: "purchase_order_owner",
            reason_code: "SUPPLIER_DIRECT_DELIVERY_READY",
            responsibility_key: format!("purchase_order:{}", row.purchase_order_id.as_ref()?),
            warehouse_id: None,
            sales_order_id: Some(row.sales_order_id.to_string()),
            validated_task_ids: HashSet::new(),
        })
    }

    fn from_electronic(row: &ElectronicDelivery) -> Option<Self> {
        (row.status == ElectronicDeliveryState::Draft).then(|| Self {
            purchase_order_id: row.purchase_order_id.clone(),
            version: row.base.version,
            owner_role: "purchase_order_owner",
            reason_code: "ELECTRONIC_DELIVERY_READY",
            responsibility_key: format!("purchase_order:{}", row.purchase_order_id),
            warehouse_id: None,
            sales_order_id: None,
            validated_task_ids: HashSet::new(),
        })
    }

    fn from_service(row: &ServiceFulfillment) -> Option<Self> {
        (row.status == ServiceFulfillmentState::Draft).then(|| Self {
            purchase_order_id: row.purchase_order_id.clone(),
            version: row.base.version,
            owner_role: "purchase_order_owner",
            reason_code: "SERVICE_FULFILLMENT_READY",
            responsibility_key: format!("purchase_order:{}", row.purchase_order_id),
            warehouse_id: None,
            sales_order_id: None,
            validated_task_ids: HashSet::new(),
        })
    }

    /// 任务壳必须仍等于已授权字段，并与实际对象冻结的责任及当前来源组织一致。
    fn matches_task(
        &self,
        fields: &WorkItemFields,
        task: &WorkItem,
        orders: &HashMap<&str, &PurchaseOrder>,
        sales: &HashMap<&str, &SalesOrder>,
    ) -> bool {
        current_task_matches_fields(task, fields)
            && task.subject_version == self.version.to_string()
            && task.owner_role == self.owner_role
            && task.reason_code.as_deref() == Some(self.reason_code)
            && task.responsibility_key() == Some(self.responsibility_key.as_str())
            && self.owner_organization_id(orders, sales) == Some(task.owner_organization_id.as_str())
    }

    /// 收货责任按实际仓库核验；采购责任按真实采购来源销售单的结算主体核验。
    fn owner_organization_id<'a>(
        &'a self,
        orders: &HashMap<&str, &PurchaseOrder>,
        sales: &HashMap<&str, &'a SalesOrder>,
    ) -> Option<&'a str> {
        let order = orders.get(self.purchase_order_id.as_ref())?;
        if self.sales_order_id.as_deref().is_some_and(|id| id != order.sales_order_id.as_ref()) {
            return None;
        }
        if let Some(id) = &self.warehouse_id {
            return order
                .target_warehouse_for_receipt()
                .is_ok_and(|warehouse| warehouse.as_ref() == id)
                .then_some(id.as_str());
        }
        sales.get(order.sales_order_id.as_ref()).map(|order| order.settlement_party_id.as_ref())
    }
}

/// 新鲜读取只验证已经授权的任务，不改变授权范围或任务字段。
fn current_task_matches_fields(task: &WorkItem, fields: &WorkItemFields) -> bool {
    task.base.id == fields.id
        && task.base.version == fields.task_version
        && task.work_item_type == fields.work_item_type
        && task.status == fields.status
        && task.status == WorkItemStatus::Open
        && task.business_object_type == fields.business_object_type
        && task.business_object_id == fields.business_object_id
        && task.subject_version == fields.subject_version
        && task.owner_role == fields.owner_role
        && task.owner_organization_id == fields.owner_organization_id
        && task.owner_user_id == fields.owner_user_id
        && task.reason_code == fields.reason_code
}

/// 缺失任务、来源或任何冻结责任字段不匹配时不展示当前供给提示。
fn validate_source_tasks(
    fields: &[WorkItemFields],
    sources: &mut FulfillmentSupplySources,
    tasks: &[WorkItem],
    orders: &[PurchaseOrder],
    sales: &[SalesOrder],
) {
    let tasks = tasks.iter().map(|task| (task.base.id.as_str(), task)).collect::<HashMap<_, _>>();
    let orders = orders.iter().map(|order| (order.base.id.as_str(), order)).collect::<HashMap<_, _>>();
    let sales = sales.iter().map(|order| (order.base.id.as_str(), order)).collect::<HashMap<_, _>>();
    for fields in fields {
        let Some(kind) = open_fulfillment_kind(fields) else {
            continue;
        };
        let Some(task) = tasks.get(fields.id.as_str()) else {
            continue;
        };
        let Some(source) = sources.get_mut(&(kind, fields.business_object_id.clone())) else {
            continue;
        };
        if source.matches_task(fields, task, &orders, &sales) {
            source.validated_task_ids.insert(fields.id.clone());
        }
    }
}

/// 完成及历史任务不参加当前供给读取，即使旧对象仍保留草稿也不能混入。
fn open_fulfillment_kind(fields: &WorkItemFields) -> Option<ObjectKind> {
    if fields.status != WorkItemStatus::Open || fields.work_item_type != WorkItemType::FulfillmentOperation {
        return None;
    }
    object_policy(fields.work_item_type, &fields.business_object_type).map(|policy| policy.object_kind)
}

/// 收集已授权开放履约任务的精确对象键，不采用请求参数或来源单号。
pub(super) fn current_fulfillment_keys(fields: &[WorkItemFields]) -> HashSet<(ObjectKind, String)> {
    fields
        .iter()
        .filter_map(|fields| {
            open_fulfillment_kind(fields).map(|kind| (kind, fields.business_object_id.clone()))
        })
        .collect()
}

/// 只有任务仍指向当前草稿版本时，才读取该真实采购单的当前供给提示。
fn applicable_purchase_id<'a>(
    fields: &WorkItemFields,
    sources: &'a FulfillmentSupplySources,
) -> Option<&'a PurchaseOrderId> {
    let kind = open_fulfillment_kind(fields)?;
    let source = sources.get(&(kind, fields.business_object_id.clone()))?;
    (fields.subject_version == source.version.to_string() && source.validated_task_ids.contains(&fields.id))
        .then_some(&source.purchase_order_id)
}

/// 当前页同一采购单只读取一次；版本过期、历史和无实际关联的对象不参加。
pub(super) fn current_purchase_ids(
    fields: &[WorkItemFields],
    sources: &FulfillmentSupplySources,
) -> Vec<PurchaseOrderId> {
    fields
        .iter()
        .filter_map(|fields| applicable_purchase_id(fields, sources))
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(PurchaseOrderId::new)
        .collect()
}

/// 只保留供给合同提供的业务文案；内部供给 ID、代码和版本不进入界面文字。
pub(super) fn supply_warning_summary(warnings: &PurchaseSupplyWarnings) -> Option<String> {
    let mut seen = HashSet::new();
    let messages = warnings
        .warnings
        .iter()
        .map(|warning| warning.message.as_str())
        .chain(warnings.association_notice.as_deref().filter(|_| warnings.association_unknown))
        .map(str::trim)
        .filter(|message| !message.is_empty() && seen.insert(*message))
        .collect::<Vec<_>>();
    (!messages.is_empty()).then(|| messages.join("；"))
}

/// 仅追加非数值提示段，保持任务影响、金额、状态和付款历史的正式事实。
pub(super) fn apply_supply_warning_briefs(
    fields: &mut [WorkItemFields],
    sources: &FulfillmentSupplySources,
    warnings: &HashMap<String, PurchaseSupplyWarnings>,
    owner_names: &HashMap<String, String>,
) {
    for fields in fields {
        let Some(warnings) = applicable_purchase_id(fields, sources).and_then(|id| warnings.get(id.as_ref()))
        else {
            continue;
        };
        let Some(summary) = supply_warning_summary(warnings) else {
            continue;
        };
        let source = fields.brief_source.get_or_insert_with(Default::default);
        push_section(
            &mut source.extra_sections,
            "当前采购负责人",
            Some(purchase_owner_label(warnings, owner_names)),
            false,
        );
        push_section(&mut source.extra_sections, "供给影响提示", Some(&summary), false);
    }
}

/// 提示必须指向真实当前采购责任；缺失姓名时明确待核验，不展示内部身份。
pub(super) fn purchase_owner_label<'a>(
    warnings: &PurchaseSupplyWarnings,
    owner_names: &'a HashMap<String, String>,
) -> &'a str {
    warnings
        .purchase_order_owner_user_id
        .as_ref()
        .and_then(|id| owner_names.get(id))
        .map(String::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("采购负责人信息待核验")
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::ids::{CustomerAccountId, PartyId, SalesOrderId, WarehouseId, WorkItemId};
    use erp_sales::entity::sales_order::SalesOrderData;
    use erp_sales::entity::sales_order::types::{BusinessType, OriginSystem};
    use erp_workflow::entity::work_item::{AssignmentSource, WorkItemData, WorkItemPriority};
    use serde::de::DeserializeOwned;
    use serde_json::{Value, json};

    use super::*;
    use crate::supplier_portal::SupplyInterruptionWarning;
    use crate::workbench::brief::{ObjectBriefSource, assemble_brief};

    fn entity<T: DeserializeOwned>(id: &str, fields: Value) -> T {
        let mut value = serde_json::to_value(BaseModel::new(id.into())).unwrap();
        value.as_object_mut().unwrap().extend(fields.as_object().unwrap().clone());
        serde_json::from_value(value).unwrap()
    }

    fn receipt() -> PurchaseReceipt {
        entity(
            "receipt",
            json!({"receipt_no":"RC-1", "purchase_order_id":"purchase", "warehouse_id":"warehouse", "status":"DRAFT"}),
        )
    }

    fn purchase() -> PurchaseOrder {
        entity(
            "purchase",
            json!({
                "business_org_unit_id":"org", "status":"DRAFT", "created_by":"creator", "updated_by":"creator",
                "purchase_no":"PO-1", "sales_order_id":"sales", "sales_order_revision_id":"revision",
                "creation_basis_id":"basis", "supplier_id":"supplier", "purchase_type":"PHYSICAL",
                "payment_term_code":"PREPAY_100", "fulfillment_responsibility":"WAREHOUSE", "owner_user_id":"buyer",
                "target_warehouse_id":"warehouse", "review_status":"APPROVED", "payment_progress":"NONE",
                "invoice_progress":"NONE", "fulfillment_progress":"NONE"
            }),
        )
    }

    fn sales() -> SalesOrder {
        SalesOrder::new(
            SalesOrderId::new("sales"),
            SalesOrderData {
                business_org_unit_id: "org".into(),
                sales_owner_user_id: "seller".into(),
                order_no: "SO-1".into(),
                business_type: BusinessType::GoodsService,
                origin_system: OriginSystem::Erp,
                source_identity_id: None,
                customer_id: CustomerAccountId::new("customer"),
                contract_id: None,
                settlement_party_id: PartyId::new("company"),
                source_status_code: None,
            },
            "seller",
        )
        .unwrap()
    }

    fn task() -> WorkItem {
        WorkItem::new_with_responsibility_key(
            WorkItemId::new("task"),
            WorkItemData {
                work_item_type: WorkItemType::FulfillmentOperation,
                business_object_type: "purchase_receipt".into(),
                business_object_id: "receipt".into(),
                subject_version: "1".into(),
                owner_role: "warehouse_inbound_handler".into(),
                owner_organization_id: "warehouse".into(),
                owner_user_id: "warehouse-handler".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: Some("PURCHASE_RECEIPT_READY".into()),
                impact_summary: Some("采购入库待确认".into()),
            },
            "warehouse:warehouse:receipt",
        )
        .unwrap()
    }

    fn sources() -> FulfillmentSupplySources {
        HashMap::from([(
            (ObjectKind::PurchaseReceipt, "receipt".into()),
            FulfillmentSupplySource::from_receipt(&receipt()).unwrap(),
        )])
    }

    fn warnings() -> PurchaseSupplyWarnings {
        PurchaseSupplyWarnings {
            warnings: vec![SupplyInterruptionWarning {
                offering_id: "internal-offering".into(),
                status: None,
                availability_version: Some(4),
                code: "INTERNAL_STOPPED".into(),
                message: "供应商当前缺货，请采购负责人核验未完成履约".into(),
            }],
            purchase_order_owner_user_id: Some("buyer".into()),
            ..Default::default()
        }
    }

    #[test]
    fn current_fulfillment_warning_preserves_business_facts_and_uses_real_purchase_owner() {
        let task = task();
        let mut fields = WorkItemFields::from(task.clone());
        fields.brief_source =
            Some(ObjectBriefSource { amount_label: Some("￥125.50".into()), ..Default::default() });
        let before = fields.clone();
        let mut sources = sources();
        validate_source_tasks(std::slice::from_ref(&fields), &mut sources, &[task], &[purchase()], &[]);
        assert_eq!(
            current_purchase_ids(std::slice::from_ref(&fields), &sources),
            vec![PurchaseOrderId::new("purchase")]
        );
        let warnings = HashMap::from([("purchase".into(), warnings())]);
        let names =
            HashMap::from([("buyer".into(), "陈国平".into()), ("warehouse-handler".into(), "赵卫东".into())]);
        apply_supply_warning_briefs(std::slice::from_mut(&mut fields), &sources, &warnings, &names);
        let brief = assemble_brief(fields.brief_source.as_ref().unwrap(), None);
        assert!(
            brief
                .sections
                .iter()
                .any(|section| section.label == "当前采购负责人" && section.value == "陈国平")
        );
        assert!(brief.sections.iter().any(|section| section.label == "供给影响提示"
            && !section.numeric
            && section.object_id.is_none()));
        assert!(!format!("{brief:?}").contains("internal-offering"));
        assert!(!format!("{brief:?}").contains("INTERNAL_STOPPED"));
        assert!(!format!("{brief:?}").contains("赵卫东"));
        assert_eq!(
            fields.brief_source.as_ref().unwrap().amount_label,
            before.brief_source.as_ref().unwrap().amount_label
        );
        assert_eq!(fields.status, before.status);
        assert_eq!(fields.impact_summary, before.impact_summary);
        assert_eq!(fields.subject_version, before.subject_version);
        assert_eq!(fields.task_version, before.task_version);
    }

    #[test]
    fn mismatched_task_identity_or_source_cannot_receive_current_supply_warning() {
        let original = task();
        let mut malformed = vec![];
        let mut stale = original.clone();
        stale.subject_version = "2".into();
        malformed.push(stale);
        let mut wrong_role = original.clone();
        wrong_role.owner_role = "purchase_order_owner".into();
        malformed.push(wrong_role);
        let mut wrong_reason = original.clone();
        wrong_reason.reason_code = Some("SERVICE_FULFILLMENT_READY".into());
        malformed.push(wrong_reason);
        let mut wrong_org = original.clone();
        wrong_org.owner_organization_id = "other-warehouse".into();
        malformed.push(wrong_org);
        let mut wrong_key = serde_json::to_value(&original).unwrap();
        wrong_key["responsibility_key"] = json!("warehouse:other:receipt");
        malformed.push(serde_json::from_value(wrong_key).unwrap());
        for task in malformed {
            let fields = WorkItemFields::from(task.clone());
            let mut sources = sources();
            validate_source_tasks(std::slice::from_ref(&fields), &mut sources, &[task], &[purchase()], &[]);
            assert!(current_purchase_ids(&[fields], &sources).is_empty());
        }
        let fields = WorkItemFields::from(original.clone());
        for orders in [vec![], {
            let mut order = purchase();
            order.target_warehouse_id = Some(WarehouseId::new("other"));
            vec![order]
        }] {
            let mut sources = sources();
            validate_source_tasks(
                std::slice::from_ref(&fields),
                &mut sources,
                std::slice::from_ref(&original),
                &orders,
                &[],
            );
            assert!(current_purchase_ids(std::slice::from_ref(&fields), &sources).is_empty());
        }
        let mut changed = original.clone();
        changed.base.version += 1;
        let mut sources = sources();
        validate_source_tasks(std::slice::from_ref(&fields), &mut sources, &[changed], &[purchase()], &[]);
        assert!(current_purchase_ids(&[fields], &sources).is_empty());
    }

    #[test]
    fn completed_or_closed_tasks_and_finished_objects_do_not_show_current_warning() {
        for status in [WorkItemStatus::Completed, WorkItemStatus::Closed] {
            let mut fields = WorkItemFields::from(task());
            fields.status = status;
            assert!(current_fulfillment_keys(&[fields]).is_empty());
        }
        let mut receipt = receipt();
        for status in [PurchaseReceiptState::Posted, PurchaseReceiptState::Reversed] {
            receipt.status = status;
            assert!(FulfillmentSupplySource::from_receipt(&receipt).is_none());
        }
        let mut delivery: Delivery = entity(
            "delivery",
            json!({"delivery_no":"DL-1","delivery_type":"SUPPLIER_DIRECT","sales_order_id":"sales","purchase_order_id":"purchase","status":"DRAFT"}),
        );
        assert!(FulfillmentSupplySource::from_delivery(&delivery).is_some());
        delivery.purchase_order_id = None;
        assert!(FulfillmentSupplySource::from_delivery(&delivery).is_none());
        delivery.purchase_order_id = Some(PurchaseOrderId::new("purchase"));
        delivery.delivery_type = DeliveryType::WarehouseShip;
        assert!(FulfillmentSupplySource::from_delivery(&delivery).is_none());
        delivery.delivery_type = DeliveryType::SupplierDirect;
        for status in [DeliveryState::Shipped, DeliveryState::Signed, DeliveryState::Reversed] {
            delivery.status = status;
            assert!(FulfillmentSupplySource::from_delivery(&delivery).is_none());
        }
    }

    #[test]
    fn direct_fulfillment_requires_exact_purchase_sales_and_owner_organization() {
        let delivery: Delivery = entity(
            "delivery",
            json!({"delivery_no":"DL-1","delivery_type":"SUPPLIER_DIRECT","sales_order_id":"sales","purchase_order_id":"purchase","status":"DRAFT"}),
        );
        let source = FulfillmentSupplySource::from_delivery(&delivery).unwrap();
        let mut task = task();
        task.business_object_type = "delivery".into();
        task.business_object_id = "delivery".into();
        task.owner_role = "purchase_order_owner".into();
        task.owner_organization_id = "company".into();
        task.reason_code = Some("SUPPLIER_DIRECT_DELIVERY_READY".into());
        let mut value = serde_json::to_value(&task).unwrap();
        value["responsibility_key"] = json!("purchase_order:purchase");
        task = serde_json::from_value(value).unwrap();
        let fields = WorkItemFields::from(task.clone());
        let purchase = purchase();
        let sales = sales();
        let orders = HashMap::from([("purchase", &purchase)]);
        let sales = HashMap::from([("sales", &sales)]);
        assert!(source.matches_task(&fields, &task, &orders, &sales));
        let mut wrong_order = purchase.clone();
        wrong_order.sales_order_id = SalesOrderId::new("other-sales");
        assert!(!source.matches_task(&fields, &task, &HashMap::from([("purchase", &wrong_order)]), &sales));
        assert!(!source.matches_task(&fields, &task, &orders, &HashMap::new()));
        task.owner_organization_id = "unrelated-company".into();
        assert!(!source.matches_task(&WorkItemFields::from(task.clone()), &task, &orders, &sales));
    }

    #[test]
    fn warning_text_keeps_unknown_association_explicit_without_internal_metadata() {
        let mut warnings = warnings();
        warnings.warnings.push(warnings.warnings[0].clone());
        warnings.association_unknown = true;
        warnings.association_notice = Some("历史采购行未记录正式供给选源，关联未知".into());
        assert_eq!(
            supply_warning_summary(&warnings).as_deref(),
            Some("供应商当前缺货，请采购负责人核验未完成履约；历史采购行未记录正式供给选源，关联未知")
        );
        assert_eq!(purchase_owner_label(&warnings, &HashMap::new()), "采购负责人信息待核验");
        warnings.warnings.clear();
        warnings.association_unknown = false;
        assert!(supply_warning_summary(&warnings).is_none());
    }
}
