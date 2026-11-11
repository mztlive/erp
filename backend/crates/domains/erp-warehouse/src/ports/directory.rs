//! Warehouse 目录的独立授权端口；组合层调用公共 DataScope。
use application_core::AuditActor;
use async_trait::async_trait;
use persistence_core::Executor;

use crate::Result;
use crate::authorization::WarehouseQuery;

/// 目录只消费 identity 的完整查询凭证，不预载账号全部授权身份。
#[async_trait]
pub trait WarehouseDirectoryAccess: Send + Sync {
    /// 解析目录读取范围。
    /// # 参数
    /// `actor` 为已认证操作人，`executor` 为用例事务。
    /// # 返回
    /// 当前读取阶段的完整查询凭证。
    /// # 错误
    /// 无权限、未接线或解析失败时拒绝。
    async fn resolve(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<WarehouseQuery>;
}
