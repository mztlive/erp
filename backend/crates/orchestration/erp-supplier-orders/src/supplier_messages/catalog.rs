//! 消息补查所需的目录应用合同；应用组合根绑定目录编排实现。

use application_core::AuditActor;
use async_trait::async_trait;
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::entity::supplier_catalog::SupplierCatalogItem;

use crate::Result;

/// 目录资格在补查循环之前读取，每项保存仍原子校验原连接快照并应用观察。
#[async_trait]
pub trait SupplierMessageCatalog: Send + Sync {
    /// 读取当前来源连接的目录同步操作人。
    ///
    /// `connection` 为服务端当前补查连接身份。
    ///
    /// 原目录授权合同核实的操作人。
    ///
    /// 资格或范围不足、账号失效及读取失败时拒绝后续补查。
    async fn actor(&self, connection: &str) -> Result<AuditActor>;

    /// 在目录编排根事务中校验原连接快照并保存当前观察。
    ///
    /// `connection` 为本次消息补查使用的原连接快照；`item` 为定向查询事实；`actor` 为循环前核实的操作人。
    ///
    /// 该项观察和原目录副作用完成原子保存。
    ///
    /// 连接技术版本、供应商、权限或目录应用失败时停止；保留实际错误载荷。
    async fn save(
        &self,
        connection: &SupplierApiConnection,
        item: SupplierCatalogItem,
        actor: &AuditActor,
    ) -> Result<()>;
}
