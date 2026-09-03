//! 线下服务履约草稿工厂（FUL-E05）。
//!
//! 双指纹均由 Service/crypto port 分别预计算；类型不可混用。

use serde::{Deserialize, Serialize};

use super::electronic_delivery::FulfillmentResult;
use super::service_fulfillment::{ServiceFulfillment, ServiceFulfillmentData};
use crate::common::source::SourceType;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    FileAssetId, PurchaseLineSalesAllocationId, PurchaseOrderId, SalesOrderLineId, ServiceFulfillmentId,
};
use crate::money::Quantity;
use crate::validation::normalize_required_text;

use super::fingerprint::FINGERPRINT_HEX_LEN;

fn typed_fingerprint(label: &str, hex: String) -> Result<String> {
    let value = normalize_required_text(hex, label, FINGERPRINT_HEX_LEN, label)?;
    if value.len() != FINGERPRINT_HEX_LEN || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::from("查询指纹必须是 64 位十六进制字符串"));
    }
    Ok(value)
}

/// 预计算的交付对象快照指纹（服务域专用，与地点指纹不可混用）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceRecipientFingerprint(String);

impl ServiceRecipientFingerprint {
    /// 包装 Service/crypto port 预计算的交付对象指纹。
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
        Ok(Self(typed_fingerprint("交付对象查询指纹不能为空", hex.into())?))
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

/// 预计算的服务地点指纹（与交付对象指纹不可混用）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceLocationFingerprint(String);

impl ServiceLocationFingerprint {
    /// 包装 Service/crypto port 预计算的服务地点指纹。
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
        Ok(Self(typed_fingerprint("服务地点查询指纹不能为空", hex.into())?))
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

/// 服务履约草稿领域输入（Service 注入 ID/事实号/actor/时钟事实）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceFulfillmentDraftData {
    /// 履约记录号。
    pub fulfillment_no: String,
    /// 销售责任明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 采购行到销售行分配。
    pub purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId,
    /// 交付对象加密快照。
    pub recipient_snapshot: String,
    /// 预计算的交付对象指纹。
    pub recipient_snapshot_fingerprint: ServiceRecipientFingerprint,
    /// 服务数量。
    pub quantity: Quantity,
    /// 履约结果。
    pub result: FulfillmentResult,
    /// 业务凭证。
    pub evidence_attachment_id: Option<FileAssetId>,
    /// 服务地点加密值。
    pub service_location_encrypted: String,
    /// 预计算的服务地点指纹。
    pub service_location_fingerprint: ServiceLocationFingerprint,
    /// 服务开始时间。
    pub service_started_at: Option<Instant>,
    /// 服务结束时间。
    pub service_ended_at: Option<Instant>,
    /// 完成说明。
    pub completion_note: Option<String>,
    /// 事实号（Service 注入）。
    pub fact_no: String,
    /// 实际服务时间。
    pub occurred_at: Instant,
    /// ERP 记录时间（Service 注入时钟事实）。
    pub recorded_at: Instant,
    /// 记录人（Service 注入 actor）。
    pub recorded_by: String,
}

/// 服务履约草稿工厂。
pub struct ServiceFulfillmentDraft;

impl ServiceFulfillmentDraft {
    /// 由草稿输入构造实体（来源默认 `Erp`）。
    ///
    /// # 参数
    /// * `id` - 实体主键（Service 注入系统 ID）
    /// * `data` - 草稿领域输入（含两项预计算指纹与时钟事实）
    ///
    /// # 返回
    /// 返回新建草稿实体。
    ///
    /// # 错误
    /// 字段违规、时间窗倒挂、数量非正时返回错误。
    ///
    /// # 约束
    /// 无 I/O、无时钟、无密钥；来源引用/原因默认空。
    pub fn build(id: ServiceFulfillmentId, data: ServiceFulfillmentDraftData) -> Result<ServiceFulfillment> {
        ServiceFulfillment::new(
            id,
            ServiceFulfillmentData {
                fulfillment_no: data.fulfillment_no,
                sales_order_line_id: data.sales_order_line_id,
                purchase_order_id: data.purchase_order_id,
                purchase_line_sales_allocation_id: data.purchase_line_sales_allocation_id,
                recipient_snapshot: data.recipient_snapshot,
                recipient_snapshot_fingerprint: data.recipient_snapshot_fingerprint.as_str().to_string(),
                quantity: data.quantity,
                result: data.result,
                evidence_attachment_id: data.evidence_attachment_id,
                service_location_encrypted: data.service_location_encrypted,
                service_location_fingerprint: data.service_location_fingerprint.as_str().to_string(),
                service_started_at: data.service_started_at,
                service_ended_at: data.service_ended_at,
                completion_note: data.completion_note,
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

    fn data() -> ServiceFulfillmentDraftData {
        ServiceFulfillmentDraftData {
            fulfillment_no: "SF-1".to_string(),
            sales_order_line_id: SalesOrderLineId::new("so-line-1"),
            purchase_order_id: PurchaseOrderId::new("po-1"),
            purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId::new("pla-1"),
            recipient_snapshot: "ciphertext-r".to_string(),
            recipient_snapshot_fingerprint: ServiceRecipientFingerprint::from_precomputed("a".repeat(64))
                .unwrap(),
            quantity: Quantity::from_str("1").unwrap(),
            result: FulfillmentResult::Success,
            evidence_attachment_id: None,
            service_location_encrypted: "ciphertext-l".to_string(),
            service_location_fingerprint: ServiceLocationFingerprint::from_precomputed("b".repeat(64))
                .unwrap(),
            service_started_at: Some(Instant::from_unix_secs(1_700_000_000)),
            service_ended_at: Some(Instant::from_unix_secs(1_700_003_600)),
            completion_note: None,
            fact_no: "F-1".to_string(),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            recorded_at: Instant::from_unix_secs(1_700_000_100),
            recorded_by: "op-1".to_string(),
        }
    }

    /// ERP 默认来源，空 reason/source reference 组合正确。
    #[test]
    fn build_defaults_to_erp_source() {
        let entity = ServiceFulfillmentDraft::build(ServiceFulfillmentId::new("sf-1"), data()).unwrap();
        assert_eq!(entity.fact.source_type, SourceType::Erp);
        assert!(entity.fact.source_reference.is_none());
    }

    /// 开始/结束时间、数量与完成说明不变量。
    #[test]
    fn window_quantity_and_note_invariants() {
        assert!(ServiceFulfillmentDraft::build(
            ServiceFulfillmentId::new("sf-2"),
            ServiceFulfillmentDraftData {
                service_started_at: Some(Instant::from_unix_secs(1_700_003_600)),
                service_ended_at: Some(Instant::from_unix_secs(1_700_000_000)),
                ..data()
            },
        )
        .is_err());
        assert!(ServiceFulfillmentDraft::build(
            ServiceFulfillmentId::new("sf-3"),
            ServiceFulfillmentDraftData {
                quantity: Quantity::from_str("0").unwrap(),
                ..data()
            },
        )
        .is_err());
    }

    /// 两项指纹类型不可混用：构造各自校验，非法格式失败。
    #[test]
    fn fingerprints_are_distinct_typed_values() {
        assert!(ServiceRecipientFingerprint::from_precomputed("zz").is_err());
        assert!(ServiceLocationFingerprint::from_precomputed("zz").is_err());
        let entity = ServiceFulfillmentDraft::build(ServiceFulfillmentId::new("sf-4"), data()).unwrap();
        assert_eq!(entity.recipient_snapshot_fingerprint, "a".repeat(64));
        assert_eq!(entity.service_location_fingerprint, "b".repeat(64));
    }
}
