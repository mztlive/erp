use std::collections::HashMap;

use application_core::AuditActor;
use erp_identity::repository::prelude::*;
use erp_identity::{AccessControlExt, SharedRbacService};
use erp_import::repository::legacy_import::LegacyImportConfirmationRow;
use erp_import::repository::prelude::*;
use erp_import::{
    ConfirmationStatus, LegacyImportConfirmation, LegacyImportConfirmationFilter,
    LegacyImportConfirmationListParams, LegacyImportExt, PageView, SortDir,
};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus};
use erp_workflow::repository::prelude::*;
use erp_workflow::service::work_item::{ProcessingState, WorkItemAllowedAction};
use persistence_core::NoTransaction;
use validator::Validate;

use super::dto::{ImportBusinessConfirmationWorkItemView, LegacyImportConfirmationView};
use super::{IMPORT_CONFIRMATION_HANDLER, IMPORT_CONFIRMATION_WORKSPACE, ImportApplyService};
use crate::adapters::workflow::work_item_service;
use crate::{Error, Result};

impl ImportApplyService {
    /// 分页查询导入确认事实列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`batch_id` 为主要筛选）。
    /// * `actor` - 当前查询人，用于任务授权投影。
    /// * `rbac` - 工作项授权使用的权限服务。
    ///
    /// # 返回
    /// 返回契约形状的分页视图。无权或不存在的任务降级为只读投影，不因此失败。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单。
    ///
    /// 确认或任务查询失败、责任目的地解析失败，以及除 `Forbidden` 与 `NotFound` 以外的任务授权失败，返回对应错误。
    pub async fn confirmation_list(
        &self,
        params: &LegacyImportConfirmationListParams,
        actor: &AuditActor,
        rbac: SharedRbacService,
    ) -> Result<PageView<LegacyImportConfirmationView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = LegacyImportConfirmationFilter {
            batch_id: query.batch_id,
            confirmation_scope: query.confirmation_scope,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .legacy_import_confirmations()
            .search_legacy_import_confirmations(&filter, &mut NoTransaction)
            .await?;
        let work_item_ids = page.items.iter().map(|row| row.work_item_id.clone()).collect::<Vec<_>>();
        let work_items = self
            .db
            .work_items()
            .list_legacy_import_confirmations_by_ids(&work_item_ids, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|item| (item.base.id.clone(), item))
            .collect::<HashMap<_, _>>();
        let work_item_service = work_item_service(self.db.clone(), rbac);
        let mut items = Vec::with_capacity(page.items.len());
        for row in page.items {
            let work_item_id = row.work_item_id.to_string();
            let work_item = match work_item_service
                .authorize_work_item(&work_item_id, actor)
                .await
                .map_err(Error::from)
            {
                Ok(view) => Some(authorized_work_item_view(view, row.status)?),
                Err(Error::Forbidden(_) | Error::NotFound(_)) => {
                    work_items.get(&work_item_id).map(read_only_work_item_view)
                },
                Err(error) => return Err(error),
            };
            items.push(confirmation_row_view(row, work_item));
        }

        let ids = items.iter().filter_map(|item| item.decided_by.clone()).collect::<Vec<_>>();
        let names = self.db.accounts().names_by_ids(&ids, &mut NoTransaction).await?;
        for item in &mut items {
            item.decided_by_name = item.decided_by.as_ref().and_then(|id| names.get(id).cloned());
        }
        Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
    }
}

fn confirmation_row_view(
    row: LegacyImportConfirmationRow,
    work_item: Option<ImportBusinessConfirmationWorkItemView>,
) -> LegacyImportConfirmationView {
    LegacyImportConfirmationView {
        id: row.id,
        batch_id: row.batch_id.to_string(),
        confirmation_scope: row.confirmation_scope,
        owner_role: row.owner_role,
        batch_version: row.batch_version,
        trial_version: row.trial_version,
        status: row.status,
        decision: row.decision,
        reason_code: row.reason_code,
        comment: None,
        work_item,
        work_item_id: row.work_item_id.to_string(),
        decided_by: row.decided_by,
        decided_by_name: None,
        decided_at: row.decided_at.map(|at| at as i64),
        version: row.version,
        created_at: row.created_at,
    }
}

/// 把任务实体映射为 W18 真实任务投影。
///
/// 处理状态固定为 `READY`，不附带领域动作。
///
/// # 参数
/// * `item` - 导入确认任务。
///
/// # 返回
/// 返回保留真实任务类型、状态与责任字段的投影。
///
/// # 错误
/// 不返回错误。
pub(super) fn work_item_view(item: &WorkItem) -> ImportBusinessConfirmationWorkItemView {
    ImportBusinessConfirmationWorkItemView {
        work_item_id: item.base.id.clone(),
        work_item_type: item.work_item_type,
        task_version: item.base.version.to_string(),
        subject_version: item.subject_version.clone(),
        status: item.status,
        owner_role: item.owner_role.clone(),
        owner_organization_id: item.owner_organization_id.clone(),
        owner_user_id: item.owner_user_id.clone(),
        processing_state: "READY".to_string(),
        allowed_actions: Vec::new(),
        action_blockers: Vec::new(),
        handler_key: IMPORT_CONFIRMATION_HANDLER.to_string(),
        destination_workspace_id: IMPORT_CONFIRMATION_WORKSPACE.to_string(),
    }
}

/// 为不在当前责任范围的查询人返回最小只读任务投影。
///
/// # 参数
/// * `item` - 导入确认任务。
///
/// # 返回
/// 返回隐藏 `owner_user_id` 的投影；开放任务追加仅可查看的阻断说明。
///
/// # 错误
/// 不返回错误。
pub(super) fn read_only_work_item_view(item: &WorkItem) -> ImportBusinessConfirmationWorkItemView {
    let mut view = work_item_view(item);
    view.owner_user_id = None;
    if item.status == WorkItemStatus::Open {
        view.action_blockers.push("当前账号不在该责任范围，任务仅可查看。".to_string());
    }
    view
}

/// 把统一待办的 actor 安全投影合并为 W18 责任与领域动作。
fn authorized_work_item_view(
    item: erp_workflow::service::work_item::AuthorizedWorkItem,
    confirmation_status: ConfirmationStatus,
) -> Result<ImportBusinessConfirmationWorkItemView> {
    let (handler_key, destination_workspace_id) = erp_read_models::workbench::work_item_destination(
        item.item.work_item_type,
        &item.item.business_object_type,
        &item.item.owner_role,
    )?;
    let mut allowed_actions = item
        .allowed_actions
        .iter()
        .copied()
        .map(work_item_action_code)
        .map(str::to_string)
        .collect::<Vec<_>>();
    append_confirmation_actions(&mut allowed_actions, confirmation_status, &item.allowed_actions);
    let mut action_blockers = item.action_blockers;
    if let Some(blocker) = item.processing_blocker {
        action_blockers.push(blocker.message);
    }
    Ok(ImportBusinessConfirmationWorkItemView {
        work_item_id: item.item.base.id.clone(),
        work_item_type: item.item.work_item_type,
        task_version: item.item.base.version.to_string(),
        subject_version: item.item.subject_version.clone(),
        status: item.item.status,
        owner_role: item.item.owner_role.clone(),
        owner_organization_id: item.item.owner_organization_id.clone(),
        owner_user_id: item.item.owner_user_id.clone(),
        processing_state: processing_state_code(item.processing_state).to_string(),
        allowed_actions,
        action_blockers,
        handler_key: handler_key.to_string(),
        destination_workspace_id: destination_workspace_id.to_string(),
    })
}

/// 只有当前责任人且确认事实仍待处理时，才追加 W18 正式领域动作。
///
/// # 参数
/// * `actions` - 待追加的动作码列表。
/// * `confirmation_status` - 确认事实状态。
/// * `responsibility_actions` - 当前责任人被允许的统一待办动作。
///
/// # 返回
/// 条件满足时向 `actions` 追加 `CONFIRM_SCOPE` 与 `RETURN_FOR_FIX`；否则不修改列表。
///
/// # 错误
/// 不返回错误。
pub(super) fn append_confirmation_actions(
    actions: &mut Vec<String>,
    confirmation_status: ConfirmationStatus,
    responsibility_actions: &[WorkItemAllowedAction],
) {
    if confirmation_status != ConfirmationStatus::Pending
        || !responsibility_actions.contains(&WorkItemAllowedAction::Process)
    {
        return;
    }
    actions.push("CONFIRM_SCOPE".to_string());
    actions.push("RETURN_FOR_FIX".to_string());
}

/// 返回统一责任动作的稳定 wire code。
fn work_item_action_code(action: WorkItemAllowedAction) -> &'static str {
    match action {
        WorkItemAllowedAction::View => "VIEW",
        WorkItemAllowedAction::Process => "PROCESS",
        WorkItemAllowedAction::Approve => "APPROVE",
        WorkItemAllowedAction::Reject => "REJECT",
        WorkItemAllowedAction::Reassign => "REASSIGN",
        WorkItemAllowedAction::Close => "CLOSE",
    }
}

/// 返回统一处理状态的稳定 wire code。
fn processing_state_code(state: ProcessingState) -> &'static str {
    match state {
        ProcessingState::Ready => "READY",
        ProcessingState::ApprovalBlocked => "APPROVAL_BLOCKED",
        ProcessingState::ExecutionBlocked => "EXECUTION_BLOCKED",
    }
}

/// 合并确认事实与对应任务投影。
///
/// # 参数
/// * `confirmation` - 确认事实。
/// * `work_item` - 对应任务。
///
/// # 返回
/// 返回带任务投影的确认视图。
///
/// # 错误
/// 不返回错误。
pub(super) fn confirmation_view(
    confirmation: LegacyImportConfirmation,
    work_item: &WorkItem,
) -> LegacyImportConfirmationView {
    let mut view: LegacyImportConfirmationView = confirmation.into();
    view.work_item = Some(work_item_view(work_item));
    view
}

#[cfg(test)]
mod tests;
