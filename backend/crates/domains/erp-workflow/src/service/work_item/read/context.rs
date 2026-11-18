use application_core::{EmptyScopeVersion, ScopeVersion};

use super::dto;
use crate::entity::work_item::{QueueContextField, QueueContextIdentity, WorkItemDueFilter};
use crate::{Error, Result};
pub(crate) fn queue_context_id(
    actor_id: &str,
    query: &dto::WorkItemListQuery,
    authorization_version: &str,
) -> String {
    QueueContextIdentity::new(
        "work-items",
        [
            QueueContextField::scalar("actor", actor_id),
            QueueContextField::scalar("scope", query.scope.as_str()),
            QueueContextField::set(
                "types",
                query.work_item_types.iter().map(|value| value.as_str().to_string()),
            ),
            QueueContextField::set("statuses", query.statuses.iter().map(|value| value.as_str().to_string())),
            QueueContextField::optional("due", query.due.map(WorkItemDueFilter::as_str)),
            QueueContextField::set(
                "priorities",
                query.priorities.iter().map(|value| value.as_str().to_string()),
            ),
            QueueContextField::optional("query", query.query.as_deref()),
            QueueContextField::scalar("filters", serde_json::to_string(&query.filters).unwrap_or_default()),
            QueueContextField::set("handlers", query.handler_user_ids.iter().cloned()),
            QueueContextField::set("sales_orders", query.sales_order_ids.iter().cloned()),
            QueueContextField::set("purchase_orders", query.purchase_order_ids.iter().cloned()),
            QueueContextField::scalar("sort", query.sort_by),
            QueueContextField::scalar("ascending", query.sort_ascending.to_string()),
            QueueContextField::scalar("authorization", authorization_version),
        ],
    )
    .into_string()
}

/// 完整结果使用有界滚动指纹；页码不进入版本，跨页共享同一授权锚点。
/// 返回前缀为 `work-item-scope` 的上下文标识。
///
/// `identity`：写入指纹的 `identity` 标量；`query`：写入指纹的 `query` 标量。
/// `result`：写入指纹的 `result` 标量。
pub fn queue_scope_version(identity: &str, query: &str, result: &str) -> String {
    QueueContextIdentity::new(
        "work-item-scope",
        [
            QueueContextField::scalar("identity", identity),
            QueueContextField::scalar("query", query),
            QueueContextField::scalar("result", result),
        ],
    )
    .into_string()
}

/// 后续页无版本或版本漂移必须失败关闭，禁止拼接不同范围结果。
/// 页码不大于 1 且未回传版本，或回传值与重算值一致时无返回值。
///
/// `page` 大于 1 且未回传版本，或回传值与 `expected` 不同时，返回 `Error::ConflictError`，详情前缀为 `DATA_SCOPE_CHANGED：`。
///
/// `provided`：客户端回传的范围版本；`expected`：服务端重算的范围版本。
pub fn ensure_scope_version(page: u64, provided: Option<&str>, expected: &str) -> Result<()> {
    let version = ScopeVersion::new(provided, EmptyScopeVersion::Provided);
    version
        .require_for_page(page)
        .and_then(|()| version.verify(expected))
        .map_err(|_| Error::ConflictError("DATA_SCOPE_CHANGED：数据范围已变化，请从第一页刷新".into()))
}

/// 为单条任务详情生成队列上下文标识。
/// 返回前缀为 `work-item-single` 的上下文标识。
pub fn single_item_context_id(actor_id: &str, work_item_id: &str) -> String {
    QueueContextIdentity::new(
        "work-item-single",
        [QueueContextField::scalar("actor", actor_id), QueueContextField::scalar("work_item", work_item_id)],
    )
    .into_string()
}

/// 校验客户端回传的队列上下文；缺省视为当前查询。
/// 未回传或与重算值相同时无返回值。
///
/// 回传值与 `expected` 不同时返回 `Error::ConflictError`，详情为 `DATA_SCOPE_CHANGED：队列范围已变化，请从第一页刷新`。
///
/// `provided`：客户端回传的队列上下文；`expected`：服务端按当前查询重算的上下文。
pub fn ensure_queue_context(provided: &Option<String>, expected: &str) -> Result<()> {
    if provided.as_deref().is_none_or(|provided| provided == expected) {
        return Ok(());
    }
    Err(Error::ConflictError("DATA_SCOPE_CHANGED：队列范围已变化，请从第一页刷新".into()))
}

#[derive(Debug, PartialEq, Eq)]
pub struct AuthorizedPage<T> {
    pub items: Vec<T>,
    pub total: i64,
}

/// 有界累加授权行：统计完整授权总数，仅保留请求页。
pub struct AuthorizedPageCollector<T> {
    pub start: u64,
    pub end: u64,
    pub total: u64,
    pub items: Vec<T>,
}

impl<T> AuthorizedPageCollector<T> {
    /// 按页码和页大小计算本页半开区间。
    /// 返回起点为 `(page - 1) * page_size`、终点为起点加页大小的收集器。
    ///
    /// 起点或终点溢出时返回 `Error::ValidationError`。
    ///
    /// `page`：页码；小于 1 时按 1 处理。
    pub fn new(page: u64, page_size: u32) -> Result<Self> {
        let start = page
            .max(1)
            .checked_sub(1)
            .and_then(|page_index| page_index.checked_mul(u64::from(page_size)))
            .ok_or_else(|| Error::ValidationError("分页偏移超出支持范围".to_string()))?;
        let end = start
            .checked_add(u64::from(page_size))
            .ok_or_else(|| Error::ValidationError("分页偏移超出支持范围".to_string()))?;
        Ok(Self { start, end, total: 0, items: Vec::with_capacity(page_size as usize) })
    }

    /// 按出现顺序计入授权行，只保留落在本页区间内的行。
    pub fn extend(&mut self, authorized: impl IntoIterator<Item = T>) {
        for item in authorized {
            let position = self.total;
            self.total = self.total.saturating_add(1);
            if position >= self.start && position < self.end {
                self.items.push(item);
            }
        }
    }

    /// 结束收集并交出本页行与授权总数。
    /// 返回本页 `items`。总数能放入 `i64` 时原样返回，否则为 `i64::MAX`。
    pub fn finish(self) -> AuthorizedPage<T> {
        AuthorizedPage { items: self.items, total: i64::try_from(self.total).unwrap_or(i64::MAX) }
    }
}
