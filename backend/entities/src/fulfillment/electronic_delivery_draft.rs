//! 电子交付草稿工厂（FUL-E04）。
//!
//! 指纹由 Service/crypto port 预计算；本层只收强类型结果。

use serde::{Deserialize, Serialize};

use super::electronic_delivery::{ElectronicDelivery, ElectronicDeliveryData, FulfillmentResult};
use crate::common::source::SourceType;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    ElectronicDeliveryId, FileAssetId, PurchaseLineSalesAllocationId, PurchaseOrderId, SalesOrderLineId,
};
use crate::money::Quantity;
use crate::validation::normalize_required_text;

use super::fingerprint::FINGERPRINT_HEX_LEN;

/// 预计算的交付对象快照指纹（不含密钥与明文）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ElectronicRecipientFingerprint(String);

impl ElectronicRecipientFingerprint {
    /// 包装 Service/crypto port 预计算的指纹。
    ///
    /// # 参数
    /// * `hex` - 64 位十六进制 HMAC 结果
    ///
    /// # 返回
    /// 返回强类型指纹。
    ///
    /// # 错误
    /// 非 64 位十六进制时返回错误。
    ///
    /// # 约束
    /// 不接收密钥与明文，不计算 HMAC。
    pub fn from_precomputed(hex: impl Into<String>) -> Result<Self> {
        let hex = normalize_required_text(
            hex.into(),
            "交付对象查询指纹不能为空",
            FINGERPRINT_HEX_LEN,
            "交付对象查询指纹过长",
        )?;
        if hex.len() != FINGERPRINT_HEX_LEN || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::from("查询指纹必须是 64 位十六进制字符串"));
        }
        Ok(Self(hex))
    }

    /// 返回指纹字符串。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回内部十六进制串的借用。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 电子交付草稿领域输入（Service 注入 ID/事实号/actor/时钟事实）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElectronicDeliveryDraftData {
    /// 履约记录号。
    pub fulfillment_no: String,
    /// 销售责任明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 采购行到销售行分配。
    pub purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId,
    /// 交付对象加密快照（不透明值）。
    pub recipient_snapshot: String,
    /// 预计算的快照指纹。
    pub recipient_snapshot_fingerprint: ElectronicRecipientFingerprint,
    /// 交付数量。
    pub quantity: Quantity,
    /// 履约结果。
    pub result: FulfillmentResult,
    /// 业务凭证。
    pub evidence_attachment_id: Option<FileAssetId>,
    /// 事实号（Service 注入）。
    pub fact_no: String,
    /// 实际交付时间。
    pub occurred_at: Instant,
    /// ERP 记录时间（Service 注入时钟事实）。
    pub recorded_at: Instant,
    /// 记录人（Service 注入 actor）。
    pub recorded_by: String,
}

/// 电子交付草稿工厂。
pub struct ElectronicDeliveryDraft;

impl ElectronicDeliveryDraft {
    /// 由草稿输入构造实体（来源默认 `Erp`）。
    ///
    /// # 参数
    /// * `id` - 实体主键（Service 注入系统 ID）
    /// * `data` - 草稿领域输入（含预计算指纹与时钟事实）
    ///
    /// # 返回
    /// 返回新建草稿实体。
    ///
    /// # 错误
    /// 字段违规、`recorded_at < occurred_at`、非正数量时返回错误。
    ///
    /// # 约束
    /// 无 I/O、无时钟、无密钥；来源引用/原因默认空。
    pub fn build(id: ElectronicDeliveryId, data: ElectronicDeliveryDraftData) -> Result<ElectronicDelivery> {
        ElectronicDelivery::new(
            id,
            ElectronicDeliveryData {
                fulfillment_no: data.fulfillment_no,
                sales_order_line_id: data.sales_order_line_id,
                purchase_order_id: data.purchase_order_id,
                purchase_line_sales_allocation_id: data.purchase_line_sales_allocation_id,
                recipient_snapshot: data.recipient_snapshot,
                recipient_snapshot_fingerprint: data.recipient_snapshot_fingerprint.as_str().to_string(),
                quantity: data.quantity,
                result: data.result,
                evidence_attachment_id: data.evidence_attachment_id,
                fact_no: data.fact_no,
                occurred_at: data.occurred_at,
                recorded_at: data.recorded_at,
                recorded_by: data.recorded_by,
                source_type: SourceType::Erp,
                source_reference: None,
                reason_code: None,
                reason_text: None,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn fingerprint() -> ElectronicRecipientFingerprint {
        ElectronicRecipientFingerprint::from_precomputed("a".repeat(64)).unwrap()
    }

    fn data() -> ElectronicDeliveryDraftData {
        ElectronicDeliveryDraftData {
            fulfillment_no: "ED-1".to_string(),
            sales_order_line_id: SalesOrderLineId::new("so-line-1"),
            purchase_order_id: PurchaseOrderId::new("po-1"),
            purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId::new("pla-1"),
            recipient_snapshot: "ciphertext".to_string(),
            recipient_snapshot_fingerprint: fingerprint(),
            quantity: Quantity::from_str("2").unwrap(),
            result: FulfillmentResult::Success,
            evidence_attachment_id: None,
            fact_no: "F-1".to_string(),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            recorded_at: Instant::from_unix_secs(1_700_000_100),
            recorded_by: "op-1".to_string(),
        }
    }

    /// ERP 默认来源，字段组合正确。
    #[test]
    fn build_defaults_to_erp_source() {
        let entity = ElectronicDeliveryDraft::build(ElectronicDeliveryId::new("ed-1"), data()).unwrap();
        assert_eq!(entity.fact.source_type, SourceType::Erp);
        assert!(entity.fact.source_reference.is_none());
        assert_eq!(entity.quantity, Quantity::from_str("2").unwrap());
    }

    /// occurred_at 转换与 recorded_at < occurred_at 拒绝。
    #[test]
    fn occurred_conversion_and_inverted_time_fails() {
        let entity = ElectronicDeliveryDraft::build(ElectronicDeliveryId::new("ed-1"), data()).unwrap();
        assert_eq!(entity.fact.occurred_at.unix_secs(), 1_700_000_000);
        assert!(ElectronicDeliveryDraft::build(
            ElectronicDeliveryId::new("ed-2"),
            ElectronicDeliveryDraftData {
                recorded_at: Instant::from_unix_secs(1_699_999_999),
                ..data()
            },
        )
        .is_err());
    }

    /// 非正数量与非法指纹失败。
    #[test]
    fn non_positive_quantity_and_bad_fingerprint_fail() {
        assert!(ElectronicDeliveryDraft::build(
            ElectronicDeliveryId::new("ed-3"),
            ElectronicDeliveryDraftData {
                quantity: Quantity::from_str("0").unwrap(),
                ..data()
            },
        )
        .is_err());
        assert!(ElectronicRecipientFingerprint::from_precomputed("zz").is_err());
    }
}
