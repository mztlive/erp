//! 采购命令共享的纯金额和乐观锁合同。
use super::PurchaseOrderService;
use crate::entity::purchase_order::{PurchaseChangeOrder, PurchaseOrder};
use crate::{Error, Result};
use erp_core::money::{Amount, Rate};
use std::str::FromStr;
/// 零金额。
pub fn zero_amount() -> Amount {
    Amount::from_str("0").expect("零金额合法")
}
/// 零税率。
pub fn zero_rate() -> Rate {
    Rate::from_str("0").expect("零税率合法")
}
impl PurchaseOrderService {
    /// 校验乐观锁版本一致。
    pub fn ensure_version(&self, entity: &impl Versioned, expected: u64) -> Result<()> {
        if entity.version() != expected {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        Ok(())
    }
}
/// 版本化访问（乐观锁校验统一入口）。
pub trait Versioned {
    /// 返回实体乐观锁版本。
    fn version(&self) -> u64;
}

impl Versioned for PurchaseOrder {
    fn version(&self) -> u64 {
        self.base.version
    }
}

impl Versioned for PurchaseChangeOrder {
    fn version(&self) -> u64 {
        self.base.version
    }
}
