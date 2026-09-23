//! Warehouse 目录的独立授权端口；组合层调用公共 DataScope。
use application_core::AuditActor;
use application_core::directory::DirectoryScope;
use async_trait::async_trait;
use persistence_core::Executor;

use crate::Result;

/// 目录授权仅返回本域稳定对象身份，None 必须有显式公司范围证明。
#[async_trait]
pub trait WarehouseDirectoryAccess: Send + Sync {
    /// 解析目录读取范围。
    /// # 参数
    /// `actor` 为已认证操作人，`executor` 为用例事务。
    /// # 返回
    /// 允许的对象身份及授权版本。
    /// # 错误
    /// 无权限、未接线或解析失败时拒绝。
    async fn resolve(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<DirectoryScope>;
}
