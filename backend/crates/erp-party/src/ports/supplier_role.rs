//! 主体子资源守卫所需的供应商角色事实端口。

use async_trait::async_trait;
use erp_core::ids::PartyId;

use crate::error::{Error, Result};

/// 主体用来读取某主体当前是否带有供应商角色的端口。
///
/// 主体不依赖 `erp-supplier` 的类型。组合根适配器查询供应商账号事实，只返回这个布尔值。
#[async_trait]
pub trait SupplierRolePort: Send + Sync {
    /// 返回 `party_id` 当前是否带有供应商角色。
    ///
    /// # 参数
    /// * `party_id` - 稳定主体 ID。
    ///
    /// # 返回
    /// 当前带有供应商角色时返回 `true`，否则返回 `false`。
    ///
    /// # 错误
    /// 适配器或存储读取失败时返回对应错误。
    async fn party_has_supplier_role(&self, party_id: &PartyId) -> Result<bool>;
}

/// 组合根未注入适配器时使用的失败关闭供应商角色端口。
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedSupplierRolePort;

#[async_trait]
impl SupplierRolePort for FailClosedSupplierRolePort {
    /// 组合根未注入适配器时拒绝判断。
    ///
    /// # 参数
    /// * `_party_id` - 主体 ID；本实现不读取。
    ///
    /// # 返回
    /// 不返回角色判断。
    ///
    /// # 错误
    /// 始终返回 `Internal`，提示供应商角色端口未接线。
    async fn party_has_supplier_role(&self, _party_id: &PartyId) -> Result<bool> {
        Err(Error::Internal("供应商角色端口未接线".to_string()))
    }
}
