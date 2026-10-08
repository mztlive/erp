//! 命令路径上的投影辅助，与变更结果共用。

use std::collections::HashSet;

use application_core::AuditActor;
use persistence_core::NoTransaction;

use super::access::{ActorAccess, authorized_item_fields};
use super::order_access::filter_order_facts;
use super::{WorkItemService, dto};
use crate::entity::work_item::WorkItem;
use crate::error::Result;
use crate::ports::ObjectFactMap;
use crate::service::approval::execution::runtime_service::approval_task_readable_with_executor;

impl<A: crate::ports::WorkflowAuthorizationPort + Send + Sync + 'static> WorkItemService<A> {
    /// 重读对象事实，只保留当前快照仍有权看到的投影。
    ///
    /// # 参数
    /// * `items` - 待投影的工作项
    /// * `access` - 当前账号访问快照
    ///
    /// # 返回
    /// 返回审批任务责任链匹配的投影，以及普通任务经对象事实过滤后的投影。
    ///
    /// # 错误
    /// 账号、审批责任链或对象事实读取失败时返回对应错误。
    pub async fn authorized_fields_for_items(
        &self,
        items: Vec<WorkItem>,
        access: &ActorAccess,
    ) -> Result<Vec<dto::WorkItemFields>> {
        let mut ordinary = Vec::new();
        let mut approval_fields = Vec::new();
        let actor = self
            .auth
            .load_account(&access.actor_id, &mut NoTransaction)
            .await?
            .map(|account| AuditActor::new(account.id, account.login_account, account.kind));
        for item in items {
            if item.work_item_type.is_document_approval() {
                if let Some(actor) = &actor
                    && approval_task_readable_with_executor(
                        &self.db,
                        &self.auth,
                        actor,
                        &item,
                        &mut NoTransaction,
                    )
                    .await?
                {
                    approval_fields.push(dto::WorkItemFields::from(item));
                }
            } else {
                ordinary.push(item);
            }
        }
        let items = ordinary;
        let keys = items
            .iter()
            .filter_map(|item| {
                item.work_item_type
                    .brief_relation(&item.business_object_type)
                    .map(|policy| (policy.object_kind, item.business_object_id.clone()))
            })
            .collect::<HashSet<_>>();
        let mut facts: ObjectFactMap = self.facts.load_object_facts(&keys, &mut NoTransaction).await?;
        filter_order_facts(&self.auth, &access.actor_id, &mut facts, &mut NoTransaction).await?;
        approval_fields
            .extend(items.into_iter().filter_map(|item| authorized_item_fields(item, access, &facts)));
        Ok(approval_fields)
    }
}

#[cfg(test)]
pub(super) const AUTHORIZED_SCAN_BATCH_SIZE: std::num::NonZeroU32 =
    std::num::NonZeroU32::new(100).expect("批次大小必须非零");

/// 去掉审批通过与驳回动作。
///
/// # 参数
/// * `actions` - 待过滤的允许动作
///
/// # 返回
/// 至少去掉一个决定动作时返回 `true`。
///
/// # 错误
/// 不返回错误。
#[cfg(test)]
pub(super) fn remove_approval_decision_actions(actions: &mut Vec<super::dto::WorkItemAllowedAction>) -> bool {
    let before = actions.len();
    actions.retain(|action| {
        !matches!(
            action,
            super::dto::WorkItemAllowedAction::Approve | super::dto::WorkItemAllowedAction::Reject
        )
    });
    actions.len() != before
}

#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
pub(super) struct AuthorizedPage<T> {
    pub(super) items: Vec<T>,
    pub(super) total: i64,
}

#[cfg(test)]
pub(super) struct AuthorizedPageCollector<T> {
    pub(super) start: u64,
    pub(super) end: u64,
    pub(super) total: u64,
    pub(super) items: Vec<T>,
}

#[cfg(test)]
impl<T> AuthorizedPageCollector<T> {
    /// 按页码和页大小建立授权结果收集器。
    ///
    /// # 参数
    /// * `page` - 从 1 开始的页码；小于 1 时按 1 处理
    /// * `page_size` - 页大小
    ///
    /// # 返回
    /// 返回覆盖该页区间的空收集器。
    ///
    /// # 错误
    /// 页偏移或页尾溢出时返回校验错误。
    pub(super) fn new(page: u64, page_size: u32) -> crate::error::Result<Self> {
        let start = page
            .max(1)
            .checked_sub(1)
            .and_then(|page_index| page_index.checked_mul(u64::from(page_size)))
            .ok_or_else(|| crate::error::Error::ValidationError("分页偏移超出支持范围".to_string()))?;
        let end = start
            .checked_add(u64::from(page_size))
            .ok_or_else(|| crate::error::Error::ValidationError("分页偏移超出支持范围".to_string()))?;
        Ok(Self { start, end, total: 0, items: Vec::with_capacity(page_size as usize) })
    }

    /// 按出现顺序收录已授权项，只保留当前页区间内的项。
    ///
    /// # 参数
    /// * `authorized` - 本批已授权项
    ///
    /// # 返回
    /// 无返回值。总数与页内项就地更新。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn extend(&mut self, authorized: impl IntoIterator<Item = T>) {
        for item in authorized {
            let position = self.total;
            self.total = self.total.saturating_add(1);
            if position >= self.start && position < self.end {
                self.items.push(item);
            }
        }
    }

    /// 结束收集并交出当前页。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回页内项与总数；总数超出 `i64` 时记为 `i64::MAX`。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn finish(self) -> AuthorizedPage<T> {
        AuthorizedPage { items: self.items, total: i64::try_from(self.total).unwrap_or(i64::MAX) }
    }
}

/// 判断该范围下的就绪视图是否计入可处理统计。
///
/// # 参数
/// * `scope` - 队列范围
/// * `access` - 查看结果
///
/// # 返回
/// 仅“我的”范围、处理状态就绪，且允许处理或审批时返回 `true`。
///
/// # 错误
/// 不返回错误。
#[cfg(test)]
pub(super) fn counts_as_processable_stat(
    scope: super::dto::WorkItemScope,
    access: &super::access::ViewAccess,
) -> bool {
    use super::dto::{ProcessingState, WorkItemAllowedAction, WorkItemScope};
    if access.processing_state != ProcessingState::Ready {
        return false;
    }
    match scope {
        WorkItemScope::Mine => access
            .allowed_actions
            .iter()
            .any(|action| matches!(action, WorkItemAllowedAction::Process | WorkItemAllowedAction::Approve)),
        WorkItemScope::Managed | WorkItemScope::History => false,
    }
}

/// 计算给定时刻所属业务日的今日窗口。
///
/// # 参数
/// * `now_unix_secs` - 当前 Unix 秒
///
/// # 返回
/// 返回今日窗口的下界与上界。
///
/// # 错误
/// 窗口计算失败时返回内部错误。
///
/// # Panics
/// 今日窗口缺少下界时 panic；`Today` 窗口约定必须有下界。
#[cfg(test)]
pub(super) fn business_day_bounds_at(
    now_unix_secs: i64,
) -> crate::error::Result<(erp_core::common::time::Instant, erp_core::common::time::Instant)> {
    use erp_core::common::time::Instant;

    use crate::entity::work_item::WorkItemDueFilter;
    let window = WorkItemDueFilter::Today
        .window_at(Instant::from_unix_secs(now_unix_secs))
        .map_err(|error| crate::error::Error::Internal(error.to_string()))?;
    Ok((window.from.expect("今日窗口必须有下界"), window.before))
}

#[cfg(test)]
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct WorkItemFamilyCountsView {
    pub approval: u64,
    pub procurement: u64,
    pub fulfillment: u64,
    pub finance: u64,
    pub exception: u64,
}

/// 按任务族累计给定类型出现次数。
///
/// # 参数
/// * `work_item_types` - 待计数的工作项类型
///
/// # 返回
/// 返回审批、采购、履约、财务与异常五族计数；溢出时饱和相加。
///
/// # 错误
/// 不返回错误。
#[cfg(test)]
pub(super) fn family_counts_for_types(
    work_item_types: impl IntoIterator<Item = crate::entity::work_item::WorkItemType>,
) -> WorkItemFamilyCountsView {
    use super::dto::WorkItemFamily;
    let mut counts = WorkItemFamilyCountsView::default();
    for work_item_type in work_item_types {
        match super::dto::family_of(work_item_type) {
            WorkItemFamily::Approval => counts.approval = counts.approval.saturating_add(1),
            WorkItemFamily::Procurement => {
                counts.procurement = counts.procurement.saturating_add(1);
            },
            WorkItemFamily::Fulfillment => {
                counts.fulfillment = counts.fulfillment.saturating_add(1);
            },
            WorkItemFamily::Finance => counts.finance = counts.finance.saturating_add(1),
            WorkItemFamily::Exception => counts.exception = counts.exception.saturating_add(1),
        }
    }
    counts
}
