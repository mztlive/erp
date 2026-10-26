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
    #[serde(default)]
    pub processed_at: Option<Instant>,
    #[serde(default)]
    pub processing_error: Option<String>,
    #[serde(default)]
    pub next_attempt_at: Option<Instant>,
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
            processed_at: None,
            processing_error: None,
            next_attempt_at: None,
        })
    }

    /// 在全部订单刷新意图已经查询并完成领域应用后标记接收处理完成。
    /// # 参数
    /// `at` 为 ERP 完成应用的时刻，不代表来源业务发生时间。
    /// # 返回
    /// 将接收状态更新为 applied 并清理处理错误。
    /// # 错误
    /// 混合商品意图或非订单意图不允许由订单消费者标记全部完成。
    pub fn mark_orders_applied(&mut self, at: Instant) -> Result<()> {
        self.mark_orders_resolved(0, at)
    }

    /// 结束已应用或已被采购变更退役的订单刷新意图，保留原始接收证据。
    /// # 参数
    /// `retired_intents` 为服务器已证明退役的意图数量；其他意图须已经完成领域应用。
    /// `at` 为 ERP 结束处理的时间。
    /// # 返回
    /// 全部退役标记 ignored_retired，混合结果标记 applied_with_retired，全部应用标记 applied。
    /// # 错误
    /// 意图不是订单、数量越界或缺少意图时拒绝结束。
    pub fn mark_orders_resolved(&mut self, retired_intents: usize, at: Instant) -> Result<()> {
        if self.refresh_hints.is_empty() || retired_intents > self.refresh_hints.len() {
            return Err(Error::ValidationError("订单回调完成意图数量无效".into()));
        }
        if self.refresh_hints.iter().any(|hint| {
            !matches!(hint.topic.as_str(), "order" | "refund")
                || !matches!(hint.target_kind.as_str(), "merchant_order" | "external_order")
        }) {
            return Err(Error::ValidationError("推送仍包含未应用的非订单变化".into()));
        }
        self.status = if retired_intents == self.refresh_hints.len() {
            "ignored_retired"
        } else if retired_intents > 0 {
            "applied_with_retired"
        } else {
            "applied"
        }
        .into();
        self.processed_at = Some(at);
        self.processing_error = None;
        self.next_attempt_at = None;
        Ok(())
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

    #[test]
    fn order_completion_requires_order_only_intents() {
        let hint = SupplierRefreshHint {
            topic: "order".into(),
            target_kind: "external_order".into(),
            object_id: "external".into(),
            product_id: None,
        };
        let mut receipt = SupplierCallbackReceipt::received(
            "local".into(),
            SupplierApiConnectionId::new("connection"),
            Instant::from_unix_secs(10),
            "order".into(),
            "cipher".into(),
            vec![hint],
        )
        .unwrap();
        receipt.mark_orders_applied(Instant::from_unix_secs(20)).unwrap();
        assert_eq!(receipt.status, "applied");
        assert_eq!(receipt.processed_at, Some(Instant::from_unix_secs(20)));
        let mut mixed = receipt.clone();
        mixed.refresh_hints.push(SupplierRefreshHint {
            topic: "product".into(),
            target_kind: "product".into(),
            object_id: "product".into(),
            product_id: None,
        });
        mixed.status = "received".into();
        mixed.processed_at = None;
        assert!(mixed.mark_orders_applied(Instant::from_unix_secs(30)).is_err());
        assert_eq!(mixed.status, "received");
    }

    #[test]
    fn retired_order_callbacks_finish_without_claiming_business_application_or_losing_evidence() {
        let hint = SupplierRefreshHint {
            topic: "order".into(),
            target_kind: "external_order".into(),
            object_id: "old-external".into(),
            product_id: None,
        };
        let mut receipt = SupplierCallbackReceipt::received(
            "late".into(),
            SupplierApiConnectionId::new("connection"),
            Instant::from_unix_secs(10),
            "order".into(),
            "encrypted-original-evidence".into(),
            vec![hint.clone()],
        )
        .unwrap();
        receipt.next_attempt_at = Some(Instant::from_unix_secs(90));
        receipt.mark_orders_resolved(1, Instant::from_unix_secs(20)).unwrap();
        assert_eq!(receipt.status, "ignored_retired");
        assert_eq!(receipt.body_encrypted, "encrypted-original-evidence");
        assert_eq!(receipt.next_attempt_at, None);
        assert_eq!(receipt.refresh_hints, vec![hint.clone()]);
        let mut mixed = receipt.clone();
        mixed.refresh_hints.push(hint);
        mixed.mark_orders_resolved(1, Instant::from_unix_secs(21)).unwrap();
        assert_eq!(mixed.status, "applied_with_retired");
        assert!(mixed.mark_orders_resolved(3, Instant::from_unix_secs(22)).is_err());
    }
}
