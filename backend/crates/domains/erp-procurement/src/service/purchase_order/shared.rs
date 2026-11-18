//! 采购命令共享的纯金额、乐观锁与单据加载合同。
use erp_core::money::{Amount, Rate};

use super::PurchaseOrderService;
use crate::entity::purchase_order::{PurchaseChangeOrder, PurchaseOrder};
use crate::{Error, Result};
/// 零金额（委托领域规范零值，避免与行金额模块分叉）。返回领域唯一的零金额。
pub fn zero_amount() -> Amount {
    crate::entity::purchase_order::zero_amount()
}
/// 零税率（委托领域规范零值，避免与行金额模块分叉）。返回领域唯一的零税率。
pub fn zero_rate() -> Rate {
    crate::entity::purchase_order::zero_rate()
}
impl PurchaseOrderService {
    /// 校验乐观锁版本一致。
    ///
    /// 版本一致时返回 `Ok(())`。版本不一致时返回冲突错误。
    pub fn ensure_version(&self, entity: &impl Versioned, expected: u64) -> Result<()> {
        if entity.version() != expected {
            return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()));
        }
        Ok(())
    }
}
/// 版本化访问（乐观锁校验统一入口）。
pub trait Versioned {
    /// 读取实体当前版本，供采购写入前比较请求的期望版本。
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

// 按 ID 加载采购单（草稿保存与作废路径共用）。
//
// 返回存在的采购单。采购单不存在或仓储读取失败时返回错误。
//
// 沿用调用方的执行器，不另开事务。
pub(crate) async fn load_order_by_id(
    db: &mongodb::Database,
    purchase_order_id: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<PurchaseOrder> {
    use crate::repository::PurchaseOrderExt;

    db.purchase_orders()
        .find_by_id(purchase_order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))
}

#[cfg(test)]
mod tests {
    use super::{zero_amount, zero_rate};
    use crate::entity::purchase_order::{zero_amount as domain_zero_amount, zero_rate as domain_zero_rate};

    // 服务零值必须与领域规范零值一致，不再各自解析字符串。
    #[test]
    fn shared_zero_values_match_domain_canonical() {
        assert_eq!(zero_amount(), domain_zero_amount());
        assert_eq!(zero_rate(), domain_zero_rate());
    }
}
