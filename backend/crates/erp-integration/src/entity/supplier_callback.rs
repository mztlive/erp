//! 无可靠来源事件号的供应商推送接收证据；独立接收身份不得冒充业务事实。
use std::fmt;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::ids::SupplierApiConnectionId;
use erp_core::validation::normalize_required_text;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierRefreshHint {
    pub topic: String,
    pub target_kind: String,
    pub object_id: String,
    pub product_id: Option<String>,
}

/// 原始证据、验签范围和补查意图单文档原子落库；不直接形成正式订单/供给事实。
#[derive(Clone, Serialize, Deserialize, Entity)]
pub struct SupplierCallbackReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    pub connection_id: SupplierApiConnectionId,
    /// 来源未提供事件号时为 None；本地字节级去重号不得填入此字段。
    pub source_event_id: Option<String>,
    pub integrity: String,
    pub received_at: Instant,
    pub route_kind: String,
    pub body_encrypted: String,
    pub refresh_hints: Vec<SupplierRefreshHint>,
    /// received 表示等待补查与正式业务应用；不能称为业务已处理。
    pub status: String,
}

impl SupplierCallbackReceipt {
    /// 构造只认证信封的推送接收记录。
    ///
    /// # 参数
    /// `id` 是本地接收号；其余参数是绑定连接、接收时间、路由、密文和补查意图。
    /// # 返回
    /// 来源事件号缺省、状态为 received 的证据记录。
    /// # 错误
    /// 连接、路由、密文或补查意图不满足接收合同则拒绝。
    pub fn received(
        id: String,
        connection_id: SupplierApiConnectionId,
        received_at: Instant,
        route_kind: String,
        body_encrypted: String,
        refresh_hints: Vec<SupplierRefreshHint>,
    ) -> Result<Self> {
        let body_encrypted =
            normalize_required_text(body_encrypted, "推送证据密文不能为空", 1024 * 1024, "推送证据超限")?;
        if connection_id.is_empty()
            || refresh_hints.is_empty()
            || refresh_hints.len() > 1000
            || !matches!(route_kind.as_str(), "order" | "status" | "product" | "cities" | "price")
            || refresh_hints.iter().any(|hint| {
                hint.object_id.is_empty()
                    || hint.object_id.len() > 128
                    || !matches!(
                        hint.topic.as_str(),
                        "product" | "availability" | "price" | "region" | "order" | "refund"
                    )
                    || !matches!(
                        hint.target_kind.as_str(),
                        "brand" | "product" | "sku" | "merchant_order" | "external_order"
                    )
            })
        {
            return Err(Error::ValidationError("供应商推送接收意图无效".into()));
        }
        Ok(Self {
            base: BaseModel::new(id),
            connection_id,
            source_event_id: None,
            integrity: "envelope_only".into(),
            received_at,
            route_kind,
            body_encrypted,
            refresh_hints,
            status: "received".into(),
        })
    }
}

impl fmt::Debug for SupplierCallbackReceipt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SupplierCallbackReceipt")
            .field("id", &self.base.id)
            .field("connection_id", &self.connection_id)
            .field("status", &self.status)
            .field("body_encrypted", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_receive_identity_never_becomes_provider_identity() {
        let hint = SupplierRefreshHint {
            topic: "order".into(),
            target_kind: "merchant_order".into(),
            object_id: "order-1".into(),
            product_id: None,
        };
        let receipt = SupplierCallbackReceipt::received(
            "local-1".into(),
            SupplierApiConnectionId::new("connection-1"),
            Instant::from_unix_secs(1),
            "order".into(),
            "encrypted-sensitive-body".into(),
            vec![hint],
        )
        .unwrap();
        assert_eq!(receipt.source_event_id, None);
        assert_eq!(receipt.status, "received");
        assert!(!format!("{receipt:?}").contains("encrypted-sensitive-body"));
        assert!(
            SupplierCallbackReceipt::received(
                "local-2".into(),
                SupplierApiConnectionId::new("connection-1"),
                Instant::from_unix_secs(1),
                "order".into(),
                "cipher".into(),
                vec![]
            )
            .is_err()
        );
    }
}
