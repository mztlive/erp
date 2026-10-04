//! 服务确认动作及安全事实投影；地点、交付对象、完成说明和密文不进入事件。

use erp_audit::{
    AuditAction, AuditCode, AuditFact, AuditField, AuditFieldChange, AuditFieldKind, AuditValue,
    BusinessEventContent, BusinessEventResult,
};
use erp_fulfillment::entity::fulfillment::{ServiceFulfillment, ServiceFulfillmentState};

/// 服务确认的唯一业务事件注册项。
pub(super) const SERVICE_CONFIRM_ACTION: AuditAction = AuditAction {
    code: "service_fulfillment.confirm",
    resource_type: "service_fulfillment",
    label: "确认服务履约",
    version: 1,
    allowed_fields: &[
        AuditField {
            code: "status",
            label: "确认状态",
            kind: AuditFieldKind::Code(&[
                AuditCode { code: "DRAFT", label: "草稿" },
                AuditCode { code: "CONFIRMED", label: "已确认" },
            ]),
        },
        AuditField {
            code: "service_result",
            label: "服务结果",
            kind: AuditFieldKind::Code(&[
                AuditCode { code: "SUCCESS", label: "成功" },
                AuditCode { code: "PARTIAL_SUCCESS", label: "部分成功" },
                AuditCode { code: "FAILURE", label: "失败" },
            ]),
        },
        AuditField { code: "quantity", label: "服务数量", kind: AuditFieldKind::Quantity },
    ],
};

/// 从已确认领域事实中选择唯一允许记录的业务投影。
///
/// # 参数
/// * `record` - 已完成草稿到确认迁移的服务履约事实。
///
/// # 返回
/// 返回业务编号、确认状态、服务结果和精确数量的安全投影。
///
/// # 错误
/// 不执行 I/O；字段和值由统一执行边界在提交前校验。
pub(super) fn confirmed_service_content(record: &ServiceFulfillment) -> BusinessEventContent {
    BusinessEventContent {
        target_id: record.base.id.clone(),
        target_number: Some(record.fulfillment_no.clone()),
        result: BusinessEventResult::Succeeded,
        field_changes: vec![AuditFieldChange {
            field: "status".to_string(),
            before: AuditValue::Code {
                code: ServiceFulfillmentState::Draft.as_str().to_string(),
                label: ServiceFulfillmentState::Draft.label().to_string(),
            },
            after: AuditValue::Code {
                code: record.status.as_str().to_string(),
                label: record.status.label().to_string(),
            },
        }],
        facts: vec![
            AuditFact {
                field: "service_result".to_string(),
                value: AuditValue::Code {
                    code: record.result.as_str().to_string(),
                    label: record.result.label().to_string(),
                },
            },
            AuditFact {
                field: "quantity".to_string(),
                value: AuditValue::Quantity { value: record.quantity },
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use application_core::AuditActor;
    use entity_core::BaseModel;
    use erp_audit::BusinessEventContext;
    use erp_core::AccountKind;
    use erp_core::common::fact::FactBase;
    use erp_core::common::source::SourceType;
    use erp_core::common::time::Instant;
    use erp_core::ids::{PurchaseLineSalesAllocationId, PurchaseOrderId, SalesOrderLineId};
    use erp_core::money::Quantity;
    use erp_fulfillment::entity::fulfillment::FulfillmentResult;

    use super::*;

    fn confirmed_record(result: FulfillmentResult) -> ServiceFulfillment {
        ServiceFulfillment {
            base: BaseModel::fake(),
            fact: FactBase {
                fact_no: "fact-1".to_string(),
                occurred_at: Instant::from_unix_secs(1_700_000_000),
                recorded_at: Instant::from_unix_secs(1_700_000_000),
                recorded_by: "creator".to_string(),
                source_type: SourceType::ManualImport,
                source_reference: None,
                reason_code: None,
                reason_text: None,
            },
            fulfillment_no: "SF-2026-001".to_string(),
            sales_order_line_id: SalesOrderLineId::new("so-line-1"),
            purchase_order_id: PurchaseOrderId::new("po-1"),
            purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId::new("allocation-1"),
            recipient_snapshot: "secret-recipient-ciphertext".to_string(),
            recipient_snapshot_fingerprint: "secret-recipient-fingerprint".to_string(),
            quantity: Quantity::from_str("1.2500").unwrap(),
            result,
            evidence_attachment_id: None,
            service_location_encrypted: "secret-service-location-ciphertext".to_string(),
            service_location_fingerprint: "secret-service-location-fingerprint".to_string(),
            service_started_at: None,
            service_ended_at: None,
            completion_note: Some("sensitive-completion-note".to_string()),
            status: ServiceFulfillmentState::Confirmed,
        }
    }

    #[test]
    fn failed_service_confirmation_is_a_successful_command_with_safe_chinese_facts() {
        let context = BusinessEventContext::new(
            AuditActor::new("actor-1".to_string(), "caigou".to_string(), AccountKind::Admin),
            SERVICE_CONFIRM_ACTION,
        )
        .unwrap();
        let mut record = confirmed_record(FulfillmentResult::Failure);
        let log = context.log(confirmed_service_content(&record)).unwrap();
        record.fulfillment_no = "SF-renamed-after-event".to_string();
        let event = log.structured_event.as_ref().unwrap();
        assert!(log.success);
        assert_eq!(event.result, BusinessEventResult::Succeeded);
        assert_eq!(event.resource_number_snapshot.as_deref(), Some("SF-2026-001"));
        assert_eq!(event.actor_name_snapshot, None);
        assert_eq!(event.field_changes[0].field_label, "确认状态");
        assert_eq!(event.facts[0].field_label, "服务结果");
        assert_eq!(
            event.facts[0].value,
            AuditValue::Code { code: "FAILURE".to_string(), label: "失败".to_string() }
        );
        assert_eq!(event.facts[1].value, AuditValue::Quantity { value: Quantity::from_str("1.25").unwrap() });
        let serialized = serde_json::to_string(&log).unwrap();
        for secret in ["secret-", "sensitive-completion-note", "SF-renamed-after-event"] {
            assert!(!serialized.contains(secret));
        }
        assert!(log.message.unwrap().contains("服务结果"));
    }
}
