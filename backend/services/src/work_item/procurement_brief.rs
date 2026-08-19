//! 采购二次确认任务已删除。本模块只保留空装载入口，避免旧队列路径回退。

use database::Executor;

use super::{ObjectFactMap, WorkItemService};
use crate::errors::Result;

impl WorkItemService {
    /// 旧采购确认对象事实已删除，恒返回空结果。
    ///
    /// # 参数
    /// * `_keys` - 本批任务引用的对象键
    /// * `_facts` - 输出的对象事实表
    /// * `_executor` - 数据访问执行器
    ///
    /// # 返回
    /// 不写入任何事实。
    pub(super) async fn load_procurement_confirmation_facts(
        &self,
        _keys: &std::collections::HashSet<(super::ObjectKind, String)>,
        _facts: &mut ObjectFactMap,
        _executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(())
    }
}
