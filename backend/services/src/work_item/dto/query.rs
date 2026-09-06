pub use entities::work_item::WorkItemDueFilter;
use entities::work_item::{WorkItemPriority, WorkItemStatus, WorkItemType};
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::status::{family_of, WorkItemFamily, WorkItemScope, WorkItemSort, WORK_ITEM_TYPES};
use super::view::WorkItemView;
use crate::errors::{Error, Result};
use application_core::{normalized_text, page_or_default, page_size_or_default};

pub(super) const DEFAULT_TIMEZONE: &str = "Asia/Shanghai";

/// 队列查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct WorkItemListParams {
    /// 必填责任范围。
    pub scope: WorkItemScope,
    /// 可选任务族。
    pub family: Option<WorkItemFamily>,
    /// 可选固定任务类型。
    pub work_item_type: Option<WorkItemType>,
    /// 历史状态筛选；开放范围只允许 `OPEN`。
    pub status: Option<WorkItemStatus>,
    /// 到期时间筛选。
    pub due: Option<WorkItemDueFilter>,
    /// 逗号分隔优先级序号，1 至 4 对应紧急至低。
    pub priorities: Option<String>,
    /// 在授权结果内按固定安全摘要字段检索。
    #[validate(length(max = 128, message = "检索词不能超过128个字符"))]
    pub q: Option<String>,
    /// 排序方式。
    pub sort: Option<WorkItemSort>,
    /// 服务端返回的队列上下文；当前实现只接受同查询重算值。
    pub queue_context_id: Option<String>,
    /// 希望聚焦的任务；不可见时服务端失败关闭。
    #[validate(length(max = 128, message = "焦点任务ID不能超过128个字符"))]
    pub current_work_item_id: Option<String>,
    /// IANA 时区；当前版本固定支持 `Asia/Shanghai`。
    pub timezone: Option<String>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1 至 100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
}

/// Service 使用的规范化队列查询。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkItemListQuery {
    pub scope: WorkItemScope,
    pub work_item_types: Vec<WorkItemType>,
    pub statuses: Vec<WorkItemStatus>,
    pub due: Option<WorkItemDueFilter>,
    pub priorities: Vec<WorkItemPriority>,
    pub query: Option<String>,
    pub current_work_item_id: Option<String>,
    pub sort_by: &'static str,
    pub sort_ascending: bool,
    pub page: u64,
    pub page_size: u32,
    pub queue_context_id: Option<String>,
}

impl WorkItemListParams {
    /// 校验并规范化责任队列查询。
    ///
    /// # 返回
    /// 返回不包含客户端责任过滤条件的服务端查询事实。
    ///
    /// # 错误
    /// scope/status 不兼容、时区或暂不支持的查询参数非法时返回验证错误。
    pub(crate) fn normalized(&self) -> Result<WorkItemListQuery> {
        let statuses = normalize_statuses(self.scope, self.status)?;
        let work_item_types = normalize_work_item_types(self.family, self.work_item_type)?;
        let priorities = parse_priorities(self.priorities.as_deref())?;
        ensure_supported_query(self)?;
        let (sort_by, sort_ascending) = normalize_sort(self.sort);
        Ok(WorkItemListQuery {
            scope: self.scope,
            work_item_types,
            statuses,
            due: self.due,
            priorities,
            query: normalized_text(self.q.as_deref()),
            current_work_item_id: normalized_text(self.current_work_item_id.as_deref()),
            sort_by,
            sort_ascending,
            page: page_or_default(self.page),
            page_size: page_size_or_default(self.page_size),
            queue_context_id: normalized_text(self.queue_context_id.as_deref()),
        })
    }
}

/// 分页责任队列响应。
#[derive(Debug, Clone, Serialize)]
pub struct WorkItemPageView {
    /// 当前页任务。
    pub items: Vec<WorkItemView>,
    /// 授权范围内总数。
    pub total: i64,
    /// 当前页码。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 服务端形成的稳定队列上下文。
    pub queue_context_id: String,
}

/// 待办统计查询参数。
///
/// 统计与正式列表复用同一责任范围、任务族、类型、时限和工作时区语义；
/// 不接受分页、自由检索或客户端责任人条件。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct WorkItemStatsParams {
    /// 指标预警所依据的当前责任范围。
    pub scope: WorkItemScope,
    /// 可选任务族。
    pub family: Option<WorkItemFamily>,
    /// 可选固定任务类型。
    pub work_item_type: Option<WorkItemType>,
    /// 可选时限筛选。
    pub due: Option<WorkItemDueFilter>,
    /// IANA 时区；当前版本固定支持 `Asia/Shanghai`。
    pub timezone: Option<String>,
}

impl WorkItemStatsParams {
    /// 复用正式队列规范化逻辑形成服务端统计查询。
    ///
    /// # 错误
    /// 任务族与类型冲突或时区不受支持时返回验证错误。
    pub(crate) fn normalized(&self) -> Result<WorkItemListQuery> {
        WorkItemListParams {
            scope: self.scope,
            family: self.family,
            work_item_type: self.work_item_type,
            status: None,
            due: self.due,
            priorities: None,
            q: None,
            sort: Some(WorkItemSort::CreatedDesc),
            queue_context_id: None,
            current_work_item_id: None,
            timezone: self.timezone.clone(),
            page: Some(1),
            page_size: Some(100),
        }
        .normalized()
    }
}

/// 服务端权限过滤后的任务族数量。
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct WorkItemFamilyCountsView {
    /// 审批与确认任务数。
    pub approval: u64,
    /// 供给与采购任务数。
    pub procurement: u64,
    /// 履约与库存任务数。
    pub fulfillment: u64,
    /// 票款与结算任务数。
    pub finance: u64,
    /// 数据治理与异常任务数。
    pub exception: u64,
}

/// 服务端权限过滤后的待办统计。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemStatsView {
    /// 已分配给当前用户的开放任务总数。
    pub assigned: u64,
    /// 当前选中责任范围内、工作时区今天到期的任务数。
    pub due_today: u64,
    /// 当前选中责任范围内、截止时间早于统计时点的开放任务数。
    pub overdue: u64,
    /// 当前选中责任范围内的结果未知与业务异常任务数。
    pub exception: u64,
    /// 当前责任范围内按任务族划分的可处理任务数。
    pub family_counts: WorkItemFamilyCountsView,
    /// 服务端统计时点。
    pub as_of: erp_core::common::time::Instant,
}

fn normalize_work_item_types(
    family: Option<WorkItemFamily>,
    work_item_type: Option<WorkItemType>,
) -> Result<Vec<WorkItemType>> {
    if work_item_type.is_some_and(|value| !WORK_ITEM_TYPES.contains(&value)) {
        return Err(Error::ValidationError("WORK_ITEM_TYPE_RETIRED".to_string()));
    }
    let Some(family) = family else {
        return Ok(work_item_type.map_or_else(|| WORK_ITEM_TYPES.to_vec(), |value| vec![value]));
    };
    if let Some(work_item_type) = work_item_type {
        if family_of(work_item_type) != family {
            return Err(Error::ValidationError("任务类型不属于所选任务族".to_string()));
        }
        return Ok(vec![work_item_type]);
    }
    Ok(family.work_item_types())
}

fn normalize_statuses(scope: WorkItemScope, status: Option<WorkItemStatus>) -> Result<Vec<WorkItemStatus>> {
    match scope {
        WorkItemScope::History => match status {
            None => Ok(vec![WorkItemStatus::Completed, WorkItemStatus::Closed]),
            Some(WorkItemStatus::Completed | WorkItemStatus::Closed) => Ok(vec![status.unwrap()]),
            Some(WorkItemStatus::Open) => Err(Error::ValidationError(
                "处理历史只能查询已完成或已关闭任务".to_string(),
            )),
        },
        _ => match status {
            None | Some(WorkItemStatus::Open) => Ok(vec![WorkItemStatus::Open]),
            Some(_) => Err(Error::ValidationError("开放队列只能查询待处理任务".to_string())),
        },
    }
}

pub(super) fn parse_priorities(value: Option<&str>) -> Result<Vec<WorkItemPriority>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(Vec::new());
    };
    value
        .split(',')
        .map(|part| match part.trim() {
            "1" => Ok(WorkItemPriority::Urgent),
            "2" => Ok(WorkItemPriority::High),
            "3" => Ok(WorkItemPriority::Normal),
            "4" => Ok(WorkItemPriority::Low),
            _ => Err(Error::ValidationError("优先级必须是1至4".to_string())),
        })
        .collect()
}

fn ensure_supported_query(params: &WorkItemListParams) -> Result<()> {
    let timezone = params.timezone.as_deref().unwrap_or(DEFAULT_TIMEZONE).trim();
    if timezone != DEFAULT_TIMEZONE {
        return Err(Error::ValidationError(
            "当前任务队列只支持 Asia/Shanghai 时区".to_string(),
        ));
    }
    Ok(())
}

fn normalize_sort(sort: Option<WorkItemSort>) -> (&'static str, bool) {
    match sort.unwrap_or(WorkItemSort::PriorityDue) {
        WorkItemSort::PriorityDue | WorkItemSort::DueAsc => ("due_at", true),
        WorkItemSort::CreatedDesc => ("created_at", false),
    }
}
