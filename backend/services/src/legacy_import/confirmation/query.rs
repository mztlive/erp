use std::collections::HashMap;

use database::{LegacyImportExt, WorkItemExt};
use entities::legacy_import::{ConfirmationStatus, LegacyImportConfirmation};
use entities::work_item::{WorkItem, WorkItemStatus};
use persistence_core::NoTransaction;
use validator::Validate;

use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;
use crate::work_item::{ProcessingState, WorkItemAllowedAction, WorkItemService, WorkItemView};
use application_core::AuditActor;

use super::super::dto::{
    ImportBusinessConfirmationWorkItemView, LegacyImportConfirmationListParams, LegacyImportConfirmationView,
    PageView, SortDir,
};
use super::super::{
    LegacyImportConfirmationFilter, LegacyImportService, IMPORT_CONFIRMATION_HANDLER,
    IMPORT_CONFIRMATION_WORKSPACE,
};

impl LegacyImportService {
    /// 分页查询导入确认事实列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`batch_id` 为主要筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
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
        let work_item_ids = page
            .items
            .iter()
            .map(|row| row.work_item_id.clone())
            .collect::<Vec<_>>();
        let work_items = self
            .db
            .work_items()
            .list_legacy_import_confirmations_by_ids(&work_item_ids, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|item| (item.base.id.clone(), item))
            .collect::<HashMap<_, _>>();
        let work_item_service = WorkItemService::new(self.db.clone(), rbac);
        let mut items = Vec::with_capacity(page.items.len());
        for row in page.items {
            let work_item_id = row.work_item_id.to_string();
            let work_item = match work_item_service.work_item_detail(&work_item_id, actor).await {
                Ok(view) => Some(authorized_work_item_view(view, row.status)),
                Err(Error::Forbidden(_) | Error::NotFound(_)) => {
                    work_items.get(&work_item_id).map(read_only_work_item_view)
                }
                Err(error) => return Err(error),
            };
            items.push(LegacyImportConfirmationView {
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
                work_item_id,
                decided_by: row.decided_by,
                decided_at: row.decided_at.map(|at| at as i64),
                version: row.version,
                created_at: row.created_at,
            });
        }

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }
}

/// 把任务实体映射为 W18 真实任务投影。
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
pub(super) fn read_only_work_item_view(item: &WorkItem) -> ImportBusinessConfirmationWorkItemView {
    let mut view = work_item_view(item);
    view.owner_user_id = None;
    if item.status == WorkItemStatus::Open {
        view.action_blockers
            .push("当前账号不在该责任范围，任务仅可查看。".to_string());
    }
    view
}

/// 把统一待办的 actor 安全投影合并为 W18 责任与领域动作。
fn authorized_work_item_view(
    item: WorkItemView,
    confirmation_status: ConfirmationStatus,
) -> ImportBusinessConfirmationWorkItemView {
    let mut allowed_actions = item
        .allowed_actions
        .iter()
        .copied()
        .map(work_item_action_code)
        .map(str::to_string)
        .collect::<Vec<_>>();
    append_confirmation_actions(&mut allowed_actions, confirmation_status, &item.allowed_actions);
    let mut action_blockers = item
        .action_blockers
        .into_iter()
        .map(|blocker| blocker.message)
        .collect::<Vec<_>>();
    if let Some(blocker) = item.processing_blocker {
        action_blockers.push(blocker.message);
    }
    ImportBusinessConfirmationWorkItemView {
        work_item_id: item.id,
        work_item_type: item.work_item_type,
        task_version: item.task_version,
        subject_version: item.subject_version,
        status: item.status,
        owner_role: item.owner_role,
        owner_organization_id: item.owner_organization_id,
        owner_user_id: item.owner_user_id,
        processing_state: processing_state_code(item.processing_state).to_string(),
        allowed_actions,
        action_blockers,
        handler_key: item.handler_key,
        destination_workspace_id: item.destination_workspace_id,
    }
}

/// 只有当前责任人且确认事实仍待处理时，才追加 W18 正式领域动作。
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
    }
}

/// 合并确认事实与对应任务投影。
pub(super) fn confirmation_view(
    confirmation: LegacyImportConfirmation,
    work_item: &WorkItem,
) -> LegacyImportConfirmationView {
    let mut view: LegacyImportConfirmationView = confirmation.into();
    view.work_item = Some(work_item_view(work_item));
    view
}
