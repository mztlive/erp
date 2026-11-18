//! 已实现的供应商接口协议；不得根据供应商名称、连接编号或地址猜测类型。
use serde::{Deserialize, Serialize};

/// 连接采用的接口协议；新增供应商协议须同时实现并注册对应适配器。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SupplierConnectorType {
    Dangaoshushu,
    JdVop,
}

impl SupplierConnectorType {
    /// 返回当前程序已实现的接口类型。
    /// 可用于新建连接的类型清单；不表示部署已配置凭据。
    pub const fn all() -> [Self; 2] {
        [Self::Dangaoshushu, Self::JdVop]
    }

    /// 为 `SupplierConnectorType` 提供中文展示名；界面展示时使用，不以文案作为稳定标识。
    /// 可直接展示的名称。
    pub fn label(self) -> &'static str {
        match self {
            Self::Dangaoshushu => "蛋糕叔叔 API",
            Self::JdVop => "京东 VOP",
        }
    }
}
