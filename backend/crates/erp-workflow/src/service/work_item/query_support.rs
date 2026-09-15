//! Command-path projection helpers shared with mutation outcomes.

use std::collections::HashSet;

use persistence_core::NoTransaction;

use super::access::{ActorAccess, authorized_item_fields};
use super::order_access::filter_order_facts;
use super::{WorkItemService, dto};
use crate::entity::work_item::WorkItem;
use crate::error::Result;
use crate::ports::ObjectFactMap;

impl<A: crate::ports::WorkflowAuthorizationPort + Send + Sync + 'static> WorkItemService<A> {
    /// Reload object facts and keep authorized projections.
    pub async fn authorized_fields_for_items(
        &self,
        items: Vec<WorkItem>,
        access: &ActorAccess,
    ) -> Result<Vec<dto::WorkItemFields>> {
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
        Ok(items.into_iter().filter_map(|item| authorized_item_fields(item, access, &facts)).collect())
    }
}

#[cfg(test)]
pub(super) const AUTHORIZED_SCAN_BATCH_SIZE: std::num::NonZeroU32 =
    std::num::NonZeroU32::new(100).expect("批次大小必须非零");

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

    pub(super) fn extend(&mut self, authorized: impl IntoIterator<Item = T>) {
        for item in authorized {
            let position = self.total;
            self.total = self.total.saturating_add(1);
            if position >= self.start && position < self.end {
                self.items.push(item);
            }
        }
    }

    pub(super) fn finish(self) -> AuthorizedPage<T> {
        AuthorizedPage { items: self.items, total: i64::try_from(self.total).unwrap_or(i64::MAX) }
    }
}

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
