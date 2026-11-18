//! 受阻筛选复用正式任务投影，保持与办理入口一致的资格判断。
use persistence_core::Executor;

use super::super::access::QueueAccess;
use super::super::dto::WorkItemFields;
use super::super::{ProcessingState, WorkItemReadService, WorkItemScope};
use crate::Result;
use crate::dto::work_item::WorkItemView;

impl WorkItemReadService {
    // 将正式办理资格回填为查询事实。
    // 原地写入受阻标志，保持原有排序与显示资料。
    //
    // 审批上下文或负责人资格读取失败时中止查询。
    //
    // `fields` 为已授权事项，`scope`、`actor`、`access` 为查询范围及身份，`executor` 为同一事务。
    pub(in crate::service::work_item::read) async fn search_blockers(
        &self,
        fields: &mut [WorkItemFields],
        _scope: WorkItemScope,
        _access: &QueueAccess,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut projected = fields
            .iter()
            .cloned()
            .map(|field| WorkItemView::from_fields(field, String::new()))
            .collect::<Result<Vec<_>>>()?;
        self.apply_approval_contexts(&mut projected, executor).await?;

        for (field, view) in fields.iter_mut().zip(projected) {
            field.search.blocked = matches!(
                view.processing_state,
                ProcessingState::ApprovalBlocked | ProcessingState::ExecutionBlocked
            );
        }
        Ok(())
    }
}
