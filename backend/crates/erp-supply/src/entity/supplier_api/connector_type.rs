//! 已实现的供应商接口协议；不得根据供应商名称、连接编号或地址猜测类型。
use serde::{Deserialize, Serialize};

/// 连接采用的接口协议；新增供应商协议须同时实现并注册对应适配器。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SupplierConnectorType {
    Dangaoshushu,
}

impl SupplierConnectorType {
    /// 返回当前程序已实现的接口类型。
    /// # 参数
    /// 无。
    /// # 返回
    /// 可用于新建连接的类型清单；不表示部署已配置凭据。
    /// # 错误
    /// 不返回错误。
    pub const fn all() -> [Self; 1] {
        [Self::Dangaoshushu]
    }

    /// 返回接口类型的中文名称。
    /// # 参数
    /// 无。
    /// # 返回
    /// 可直接展示的名称。
    /// # 错误
    /// 不返回错误。
    pub fn label(self) -> &'static str {
        match self {
            Self::Dangaoshushu => "蛋糕叔叔 API",
        }
    }
}
