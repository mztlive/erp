use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// 配送补充操作的永久去重回执，与执行资料及成功审计同事务保存。
#[derive(Clone, Serialize, Deserialize, Entity)]
pub struct ApiOrderSetupReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    pub execution_id: String,
    pub payload_hash: String,
}

impl ApiOrderSetupReceipt {
    /// 核对同一命令身份的原载荷，不重放已成功的资料修改。
    /// # 参数
    /// `execution` 为当前订单，`payload_hash` 为规范化业务载荷指纹。
    /// # 返回
    /// 身份和载荷相同时成功。
    /// # 错误
    /// 相同操作号跨订单或改变载荷时返回冲突。
    pub fn ensure_replay(&self, execution: &str, payload_hash: &str) -> Result<()> {
        if self.execution_id != execution || self.payload_hash != payload_hash {
            return Err(Error::ConflictError("同一配送操作号不得改变订单或内容".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn setup_receipt_replays_only_exact_order_and_payload() {
        let receipt = ApiOrderSetupReceipt {
            base: BaseModel::fake(),
            execution_id: "order-1".into(),
            payload_hash: "hash-1".into(),
        };
        assert!(receipt.ensure_replay("order-1", "hash-1").is_ok());
        assert!(receipt.ensure_replay("order-2", "hash-1").is_err());
        assert!(receipt.ensure_replay("order-1", "hash-2").is_err());
    }
}
