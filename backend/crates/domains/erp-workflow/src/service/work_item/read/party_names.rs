//! 已授权任务的处理人显示名按 ID 批量读取。
use erp_identity::service::directory::AccountDirectoryService;
use persistence_core::Executor;

use super::WorkItemReadService;
use crate::Result;
use crate::dto::work_item::WorkItemView;
use crate::dto::work_item::presentation::resolve_owner_display_name;
impl WorkItemReadService {
    /// 补齐已授权任务的处理人姓名。
    /// # 参数
    /// `executor` 沿用任务读取快照，`items` 已通过任务授权。
    /// # 返回
    /// 原地更新显示名；不改变筛选、排序或计数。
    /// # 错误
    /// 姓名读取失败时传播 identity 错误。
    pub(super) async fn apply_party_names_with(
        &self,
        executor: &mut dyn Executor,
        items: &mut [WorkItemView],
    ) -> Result<()> {
        let mut ids = items.iter().filter_map(|item| item.owner_user_id.clone()).collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        let names = AccountDirectoryService::new(self.db.clone()).names(&ids, executor).await?;
        for item in items {
            if let Some(owner) = &mut item.owner_user {
                owner.display_name = resolve_owner_display_name(&owner.id, &names);
            }
        }
        Ok(())
    }
}
