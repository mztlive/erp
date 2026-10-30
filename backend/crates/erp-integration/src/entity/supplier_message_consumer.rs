//! 一个供应商账号只绑定一个消息消费连接；版本与租期阻断旧消费者继续写入。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::ids::SupplierApiConnectionId;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct SupplierMessageConsumer {
    #[serde(flatten)]
    pub base: BaseModel,
    pub account_scope: String,
    pub connection_id: SupplierApiConnectionId,
    pub lease_owner: Option<String>,
    pub lease_epoch: i64,
    pub lease_until: Instant,
}

impl SupplierMessageConsumer {
    /// 创建账号的唯一消费连接绑定；账号更换必须使用新命名空间。
    /// # 参数
    /// `id` 为稳定本地号，`account_scope` 为配置账号范围，`connection_id` 为服务端绑定连接。
    /// # 返回
    /// 尚未持有租期的账号绑定。
    /// # 错误
    /// 空身份、过长范围或无效连接时拒绝。
    pub fn bind(id: String, account_scope: String, connection_id: SupplierApiConnectionId) -> Result<Self> {
        if id.is_empty()
            || id.len() > 128
            || account_scope.trim().is_empty()
            || account_scope.len() > 256
            || connection_id.is_empty()
        {
            return Err(Error::ValidationError("供应商消息账号绑定无效".into()));
        }
        Ok(Self {
            base: BaseModel::new(id),
            account_scope,
            connection_id,
            lease_owner: None,
            lease_epoch: 0,
            lease_until: Instant::from_unix_secs(0),
        })
    }

    /// 核验本地处理仍持有当前账号的消费资格。
    /// # 参数
    /// `owner` 和 `epoch` 来自原子领取结果，`now` 为服务端当前时间。
    /// # 返回
    /// 当前连接的消费者仍可继续。
    /// # 错误
    /// 租期失效、被替代或归属不符时失败关闭。
    pub fn require_lease(&self, owner: &str, epoch: i64, now: Instant) -> Result<()> {
        if self.lease_owner.as_deref() != Some(owner) || self.lease_epoch != epoch || self.lease_until <= now
        {
            return Err(Error::ConflictError("供应商消息消费资格已失效".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_epoch_and_expired_consumer_cannot_continue() {
        let mut consumer = SupplierMessageConsumer::bind(
            "consumer".into(),
            "account".into(),
            SupplierApiConnectionId::new("connection"),
        )
        .unwrap();
        consumer.lease_owner = Some("worker".into());
        consumer.lease_epoch = 2;
        consumer.lease_until = Instant::from_unix_secs(20);
        assert!(consumer.require_lease("worker", 2, Instant::from_unix_secs(19)).is_ok());
        assert!(consumer.require_lease("worker", 1, Instant::from_unix_secs(19)).is_err());
        assert!(consumer.require_lease("other", 2, Instant::from_unix_secs(19)).is_err());
        assert!(consumer.require_lease("worker", 2, Instant::from_unix_secs(20)).is_err());
    }
}
