//! W09 履约责任队列查询用例。
//!
//! 本模块只从当前账号持有的开放 `FULFILLMENT_OPERATION` WorkItem 形成页面投影。
//! 作业单据、来源单据和仓库由数据库批量关联；分页、筛选和指标均在服务端完成。

use std::collections::HashMap;

use database::{
    FulfillmentQueueFilter as RepositoryFilter, FulfillmentQueueItemRow, NoTransaction, WorkItemExt,
};
use entities::{
    common::time::Instant,
    work_item::{WorkItemPriority, WorkItemType},
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::{
    audit::AuditActor,
    errors::{Error, Result},
    query::{normalized_text, page_or_default, page_size_or_default},
};

use super::{
    business_day_bounds, ensure_queue_context, has_execution_permissions, stable_digest, ActorAccess,
    WorkItemDueFilter, WorkItemService,
};

const ALL_OPERATION_TYPES: [FulfillmentQueueOperationType; 5] = [
    FulfillmentQueueOperationType::Receipt,
    FulfillmentQueueOperationType::WarehouseShip,
    FulfillmentQueueOperationType::SupplierDirect,
    FulfillmentQueueOperationType::Electronic,
    FulfillmentQueueOperationType::Service,
];
const DEFAULT_TIMEZONE: &str = "Asia/Shanghai";

/// W09 稳定履约作业类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FulfillmentQueueOperationType {
    /// 采购到货入库。
    Receipt,
    /// 公司仓库发货。
    WarehouseShip,
    /// 供应商直发。
    SupplierDirect,
    /// 电子交付。
    Electronic,
    /// 线下服务履约。
    Service,
}

impl FulfillmentQueueOperationType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Receipt => "RECEIPT",
            Self::WarehouseShip => "WAREHOUSE_SHIP",
            Self::SupplierDirect => "SUPPLIER_DIRECT",
            Self::Electronic => "ELECTRONIC",
            Self::Service => "SERVICE",
        }
    }

    fn business_object_type(self) -> &'static str {
        match self {
            Self::Receipt => "purchase_receipt",
            Self::WarehouseShip | Self::SupplierDirect => "delivery",
            Self::Electronic => "electronic_delivery",
            Self::Service => "service_fulfillment",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "RECEIPT" => Some(Self::Receipt),
            "WAREHOUSE_SHIP" => Some(Self::WarehouseShip),
            "SUPPLIER_DIRECT" => Some(Self::SupplierDirect),
            "ELECTRONIC" => Some(Self::Electronic),
            "SERVICE" => Some(Self::Service),
            _ => None,
        }
    }
}

/// W09 作业先决条件筛选。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FulfillmentQueueGateFilter {
    /// 先决条件受阻。
    Blocked,
    /// 先决条件已满足。
    Satisfied,
}

impl FulfillmentQueueGateFilter {
    fn as_repository_str(self) -> &'static str {
        match self {
            Self::Blocked => "BLOCKED",
            Self::Satisfied => "SATISFIED",
        }
    }
}

/// W09 作业先决条件状态。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FulfillmentQueueGateState {
    /// 先决条件已满足。
    Satisfied,
    /// 先决条件受阻。
    Blocked,
    /// 当前作业类型没有先决条件。
    NotApplicable,
}

impl FulfillmentQueueGateState {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "SATISFIED" => Some(Self::Satisfied),
            "BLOCKED" => Some(Self::Blocked),
            "NOT_APPLICABLE" => Some(Self::NotApplicable),
            _ => None,
        }
    }
}

/// W09 履约责任队列查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct FulfillmentQueueListParams {
    /// 逗号分隔的稳定作业类型；缺省表示全部已授权类型。
    #[validate(length(max = 128, message = "作业类型筛选不能超过128个字符"))]
    pub operation_types: Option<String>,
    /// 精确履约对象 ID。
    #[validate(length(max = 128, message = "履约对象ID不能超过128个字符"))]
    pub operation_id: Option<String>,
    /// 来源销售单 ID。
    #[validate(length(max = 128, message = "销售单ID不能超过128个字符"))]
    pub sales_order_id: Option<String>,
    /// 来源采购单 ID。
    #[validate(length(max = 128, message = "采购单ID不能超过128个字符"))]
    pub purchase_order_id: Option<String>,
    /// 作业仓库 ID。
    #[validate(length(max = 128, message = "仓库ID不能超过128个字符"))]
    pub warehouse_id: Option<String>,
    /// 在授权结果内按作业号或来源单号检索。
    #[validate(length(max = 128, message = "检索词不能超过128个字符"))]
    pub q: Option<String>,
    /// 业务日到期筛选。
    pub due: Option<WorkItemDueFilter>,
    /// 作业先决条件筛选。
    pub gate: Option<FulfillmentQueueGateFilter>,
    /// 服务端返回的稳定队列上下文。
    #[validate(length(max = 128, message = "队列上下文不能超过128个字符"))]
    pub queue_context_id: Option<String>,
    /// IANA 时区；当前固定为 Asia/Shanghai。
    pub timezone: Option<String>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1 至 100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FulfillmentQueueQuery {
    operation_types: Vec<FulfillmentQueueOperationType>,
    operation_id: Option<String>,
    sales_order_id: Option<String>,
    purchase_order_id: Option<String>,
    warehouse_id: Option<String>,
    query: Option<String>,
    due: Option<WorkItemDueFilter>,
    gate: Option<FulfillmentQueueGateFilter>,
    queue_context_id: Option<String>,
    page: u64,
    page_size: u32,
}

impl FulfillmentQueueListParams {
    fn normalized(&self) -> Result<FulfillmentQueueQuery> {
        ensure_timezone(self.timezone.as_deref())?;
        Ok(FulfillmentQueueQuery {
            operation_types: parse_operation_types(self.operation_types.as_deref())?,
            operation_id: normalized_text(self.operation_id.as_deref()),
            sales_order_id: normalized_text(self.sales_order_id.as_deref()),
            purchase_order_id: normalized_text(self.purchase_order_id.as_deref()),
            warehouse_id: normalized_text(self.warehouse_id.as_deref()),
            query: normalized_text(self.q.as_deref()),
            due: self.due,
            gate: self.gate,
            queue_context_id: normalized_text(self.queue_context_id.as_deref()),
            page: page_or_default(self.page),
            page_size: page_size_or_default(self.page_size),
        })
    }
}

/// W09 当前页作业摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FulfillmentQueueItemView {
    /// WorkItem 稳定 ID。
    pub work_item_id: String,
    /// WorkItem 乐观锁版本。
    pub task_version: String,
    /// 履约对象版本。
    pub source_version: String,
    /// 当前责任岗位代码。
    pub owner_role: String,
    /// 当前责任组织 ID。
    pub owner_organization_id: String,
    /// 责任优先级。
    pub priority: WorkItemPriority,
    /// 责任形成原因代码。
    pub reason_code: String,
    /// 权限安全的影响摘要。
    pub impact_summary: String,
    /// 履约对象 ID。
    pub operation_id: String,
    /// 稳定作业类型。
    pub operation_type: FulfillmentQueueOperationType,
    /// 业务对象类型。
    pub business_object_type: String,
    /// 作业单号。
    pub summary: String,
    /// 履约对象乐观锁版本。
    pub edit_version: u64,
    /// 作业时间。
    pub due_at: Instant,
    /// 来源销售单 ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sales_order_id: Option<String>,
    /// 来源销售单号。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sales_order_no: Option<String>,
    /// 来源采购单 ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purchase_order_id: Option<String>,
    /// 来源采购单号。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purchase_order_no: Option<String>,
    /// 作业仓库 ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warehouse_id: Option<String>,
    /// 作业仓库标签。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warehouse_label: Option<String>,
    /// 销售责任明细 ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sales_order_line_id: Option<String>,
    /// 采购到销售分配 ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purchase_line_sales_allocation_id: Option<String>,
    /// 作业数量的十进制字符串。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<String>,
    /// 履约结果代码。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    /// 承运人。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub carrier: Option<String>,
    /// 运单号。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracking_no: Option<String>,
    /// 先决条件状态。
    pub gate_state: FulfillmentQueueGateState,
}

/// W09 作业类型跨页指标。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FulfillmentQueueMetricView {
    /// 稳定作业类型。
    pub operation_type: FulfillmentQueueOperationType,
    /// 当前筛选内的作业数量。
    pub count: i64,
}

/// W09 仓库筛选选项。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FulfillmentQueueWarehouseView {
    /// 仓库 ID。
    pub id: String,
    /// 仓库代码或安全回退标签。
    pub label: String,
}

/// W09 服务端分页读模型。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FulfillmentQueuePageView {
    /// 当前页作业。
    pub items: Vec<FulfillmentQueueItemView>,
    /// 当前筛选内跨页总数。
    pub total: i64,
    /// 当前页码。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 服务端形成的稳定队列上下文。
    pub queue_context_id: String,
    /// 当前账号具备完整执行权限的作业类型。
    pub visible_types: Vec<FulfillmentQueueOperationType>,
    /// 当前筛选内按类型汇总的跨页指标。
    pub metrics: Vec<FulfillmentQueueMetricView>,
    /// 当前筛选内可用的仓库选项。
    pub warehouse_options: Vec<FulfillmentQueueWarehouseView>,
    /// 服务端快照时点。
    pub as_of: Instant,
}

impl WorkItemService {
    /// 查询当前个人责任范围内的 W09 履约队列。
    ///
    /// # 参数
    /// * `params` - 履约筛选与分页参数
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回单次 MongoDB 聚合形成的分页、指标和仓库选项。
    ///
    /// # 错误
    /// 参数非法、队列上下文变化、授权事实或聚合读取失败时返回错误。
    pub async fn fulfillment_queue_list(
        &self,
        params: &FulfillmentQueueListParams,
        actor: &AuditActor,
    ) -> Result<FulfillmentQueuePageView> {
        params.validate()?;
        let query = params.normalized()?;
        let access = self.actor_access(actor).await?;
        let visible_types = visible_operation_types(&query.operation_types, &access);
        let context_id = fulfillment_queue_context_id(actor.id(), &query, &visible_types);
        ensure_queue_context(&query.queue_context_id, &context_id)?;

        if visible_types.is_empty() {
            return Ok(empty_page(query.page, query.page_size, context_id));
        }

        let offset = query
            .page
            .checked_sub(1)
            .and_then(|page| page.checked_mul(u64::from(query.page_size)))
            .ok_or_else(|| Error::ValidationError("履约队列分页偏移超出支持范围".to_string()))?;
        let (due_from, due_before) = due_bounds(query.due)?;
        let repository_page = self
            .db
            .work_items()
            .search_fulfillment_queue(
                &RepositoryFilter {
                    owner_user_id: actor.id().to_string(),
                    operation_types: visible_types
                        .iter()
                        .map(|operation_type| operation_type.as_str().to_string())
                        .collect(),
                    operation_id: query.operation_id,
                    sales_order_id: query.sales_order_id,
                    purchase_order_id: query.purchase_order_id,
                    warehouse_id: query.warehouse_id,
                    query: query.query,
                    due_from,
                    due_before,
                    gate: query.gate.map(|gate| gate.as_repository_str().to_string()),
                    offset,
                    page_size: query.page_size,
                },
                &mut NoTransaction,
            )
            .await?;

        let items = repository_page
            .items
            .into_iter()
            .map(map_item)
            .collect::<Result<Vec<_>>>()?;
        let counts: HashMap<_, _> = repository_page
            .metrics
            .into_iter()
            .map(|metric| (metric.operation_type, metric.count))
            .collect();
        let metrics = visible_types
            .iter()
            .copied()
            .map(|operation_type| FulfillmentQueueMetricView {
                count: counts.get(operation_type.as_str()).copied().unwrap_or(0),
                operation_type,
            })
            .collect();
        let warehouse_options = repository_page
            .warehouses
            .into_iter()
            .map(|warehouse| FulfillmentQueueWarehouseView {
                id: warehouse.id,
                label: warehouse.label,
            })
            .collect();

        Ok(FulfillmentQueuePageView {
            items,
            total: repository_page.total,
            page: query.page,
            page_size: query.page_size,
            queue_context_id: context_id,
            visible_types,
            metrics,
            warehouse_options,
            as_of: Instant::now(),
        })
    }
}

fn parse_operation_types(value: Option<&str>) -> Result<Vec<FulfillmentQueueOperationType>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(ALL_OPERATION_TYPES.to_vec());
    };
    let requested = value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            FulfillmentQueueOperationType::parse(value)
                .ok_or_else(|| Error::ValidationError(format!("不支持的履约作业类型: {value}")))
        })
        .collect::<Result<Vec<_>>>()?;
    if requested.is_empty() {
        return Err(Error::ValidationError("履约作业类型不能为空".to_string()));
    }
    Ok(ALL_OPERATION_TYPES
        .into_iter()
        .filter(|operation_type| requested.contains(operation_type))
        .collect())
}

fn visible_operation_types(
    requested: &[FulfillmentQueueOperationType],
    access: &ActorAccess,
) -> Vec<FulfillmentQueueOperationType> {
    requested
        .iter()
        .copied()
        .filter(|operation_type| {
            has_execution_permissions(
                WorkItemType::FulfillmentOperation,
                operation_type.business_object_type(),
                access,
            )
        })
        .collect()
}

fn ensure_timezone(timezone: Option<&str>) -> Result<()> {
    if timezone
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none_or(|timezone| timezone == DEFAULT_TIMEZONE)
    {
        return Ok(());
    }
    Err(Error::ValidationError(
        "履约队列时区只支持 Asia/Shanghai".to_string(),
    ))
}

fn due_bounds(due: Option<WorkItemDueFilter>) -> Result<(Option<i64>, Option<i64>)> {
    let Some(due) = due else {
        return Ok((None, None));
    };
    let (start, tomorrow) = business_day_bounds()?;
    Ok(match due {
        WorkItemDueFilter::Today => (Some(start.unix_secs()), Some(tomorrow.unix_secs())),
        WorkItemDueFilter::Overdue => (None, Some(start.unix_secs())),
    })
}

fn fulfillment_queue_context_id(
    actor_id: &str,
    query: &FulfillmentQueueQuery,
    visible_types: &[FulfillmentQueueOperationType],
) -> String {
    stable_digest(&format!(
        "fulfillment-queue|{actor_id}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
        visible_types,
        query.operation_id,
        query.sales_order_id,
        query.purchase_order_id,
        query.warehouse_id,
        query.query,
        query.due,
        query.gate,
    ))
}

fn map_item(row: FulfillmentQueueItemRow) -> Result<FulfillmentQueueItemView> {
    let operation_type = FulfillmentQueueOperationType::parse(&row.operation_type)
        .ok_or_else(|| Error::Internal("履约队列返回未注册作业类型".to_string()))?;
    let gate_state = FulfillmentQueueGateState::parse(&row.gate_state)
        .ok_or_else(|| Error::Internal("履约队列返回未注册先决条件状态".to_string()))?;
    Ok(FulfillmentQueueItemView {
        work_item_id: row.work_item_id,
        task_version: row.task_version.to_string(),
        source_version: row.subject_version,
        owner_role: row.owner_role,
        owner_organization_id: row.owner_organization_id,
        priority: row.priority,
        reason_code: row.reason_code,
        impact_summary: row.impact_summary,
        operation_id: row.operation_id,
        operation_type,
        business_object_type: row.business_object_type,
        summary: row.summary,
        edit_version: row.edit_version,
        due_at: Instant::from_unix_secs(row.due_at),
        sales_order_id: row.sales_order_id,
        sales_order_no: row.sales_order_no,
        purchase_order_id: row.purchase_order_id,
        purchase_order_no: row.purchase_order_no,
        warehouse_id: row.warehouse_id,
        warehouse_label: row.warehouse_label,
        sales_order_line_id: row.sales_order_line_id,
        purchase_line_sales_allocation_id: row.purchase_line_sales_allocation_id,
        quantity: row.quantity,
        result: row.result,
        carrier: row.carrier,
        tracking_no: row.tracking_no,
        gate_state,
    })
}

fn empty_page(page: u64, page_size: u32, context_id: String) -> FulfillmentQueuePageView {
    FulfillmentQueuePageView {
        items: Vec::new(),
        total: 0,
        page,
        page_size,
        queue_context_id: context_id,
        visible_types: Vec::new(),
        metrics: Vec::new(),
        warehouse_options: Vec::new(),
        as_of: Instant::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::{ensure_timezone, parse_operation_types, FulfillmentQueueOperationType, ALL_OPERATION_TYPES};

    #[test]
    fn operation_types_are_canonical_and_deduplicated() {
        let parsed = parse_operation_types(Some("SERVICE,RECEIPT,SERVICE")).unwrap();
        assert_eq!(
            parsed,
            vec![
                FulfillmentQueueOperationType::Receipt,
                FulfillmentQueueOperationType::Service,
            ]
        );
    }

    #[test]
    fn missing_operation_types_selects_registered_contract() {
        assert_eq!(parse_operation_types(None).unwrap(), ALL_OPERATION_TYPES);
    }

    #[test]
    fn unknown_operation_type_fails_closed() {
        assert!(parse_operation_types(Some("RECEIPT,UNKNOWN")).is_err());
    }

    #[test]
    fn queue_timezone_is_fixed_to_business_timezone() {
        assert!(ensure_timezone(None).is_ok());
        assert!(ensure_timezone(Some("Asia/Shanghai")).is_ok());
        assert!(ensure_timezone(Some("UTC")).is_err());
    }
}
