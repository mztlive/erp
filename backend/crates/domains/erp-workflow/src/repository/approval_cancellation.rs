//! ERP 取消事实仓储，与纯 BPM 模型集合保持分离。

use mongodb::Database;
use persistence_core::Repository;

use crate::entity::approval_cancellation::ApprovalCancellationFact;

/// 每审批实例唯一的 ERP 取消结果集合。
pub trait ApprovalCancellationExt {
    const APPROVAL_CANCELLATION_FACTS: &'static str = "approval_cancellation_facts";
    /// 取得 ERP 取消事实仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回本域不可变取消事实仓储。
    /// # 错误
    /// 无。
    fn approval_cancellation_facts(&self) -> Repository<'_, ApprovalCancellationFact>;
}

impl ApprovalCancellationExt for Database {
    fn approval_cancellation_facts(&self) -> Repository<'_, ApprovalCancellationFact> {
        Repository::new(self, Self::APPROVAL_CANCELLATION_FACTS)
    }
}
