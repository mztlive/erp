//! 投影投递链关系与 ACK 单调性领域守卫（SALES-E15）。
//!
//! 封装 W23 投递链的三段关系校验及商城确认指针单调性，作为投影/修订/投递
//! 实体的共享领域规则。Service 仅负责加载当前修订、外部 connector 与事务，
//! 所有纯身份与序号规则由本模块保障。

use crate::errors::{Error, Result};
use crate::projection::{SalesOrderProjection, SalesOrderProjectionDelivery, SalesOrderProjectionRevision};

/// 校验投影、修订与投递的三段链关系。
///
/// 用于受控发送与投递成功结算前的链路一致性校验，确保投影稳定身份、修订
/// 归属与投递目标商城三者一致。
///
/// # 参数
/// * `projection` - 投影稳定身份实体
/// * `revision` - 投影修订实体
/// * `delivery` - 投递记录实体
///
/// # 返回
/// 关系一致时返回 `Ok(())`。
///
/// # 错误
/// 任一链路不一致（修订不属于投影、投递不属于修订或商城不一致）时返回错误，
/// 调用方应映射为 409 Conflict。
///
/// # 关键业务约束
/// 该方法不触及持久化或外部状态；仅比较已加载三实体的身份字段。
pub fn ensure_delivery_relation(
    projection: &SalesOrderProjection,
    revision: &SalesOrderProjectionRevision,
    delivery: &SalesOrderProjectionDelivery,
) -> Result<()> {
    if revision.projection_id.to_string() != projection.base.id
        || delivery.projection_revision_id.to_string() != revision.base.id
        || delivery.target_mall_id != projection.target_mall_id
    {
        return Err(Error::from("投影、修订与固定投递身份不一致"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ensure_delivery_relation;
    use crate::common::time::Instant;
    use crate::ids::{
        SalesOrderId, SalesOrderProjectionId, SalesOrderProjectionRevisionId, SalesOrderRevisionId,
        SourceSystemId,
    };
    use crate::money::Amount;
    use crate::projection::{
        CardForm, ProjectionSource, SalesOrderProjection, SalesOrderProjectionData,
        SalesOrderProjectionDelivery, SalesOrderProjectionDeliveryData, SalesOrderProjectionRevision,
        SalesOrderProjectionRevisionData,
    };
    use std::str::FromStr;

    fn projection_with_mall(mall: &str) -> SalesOrderProjection {
        SalesOrderProjection::new(
            SalesOrderProjectionId::new("proj-1"),
            SalesOrderProjectionData {
                sales_order_id: SalesOrderId::new("so-1"),
                target_mall_id: SourceSystemId::new(mall),
            },
        )
        .unwrap()
    }

    fn revision_with_projection(
        projection_id: &str,
        rev_id: &str,
        rev_no: u32,
    ) -> SalesOrderProjectionRevision {
        SalesOrderProjectionRevision::new(
            SalesOrderProjectionRevisionId::new(rev_id),
            rev_no,
            SalesOrderProjectionRevisionData {
                projection_id: SalesOrderProjectionId::new(projection_id),
                projection_source: ProjectionSource::ErpRevision,
                sales_order_revision_id: SalesOrderRevisionId::new("so-rev-1"),
                customer_external_identity: "cust-1".to_string(),
                voucher_category_external_identity: "cat-1".to_string(),
                voucher_expiry_at: Instant::from_unix_secs(1_800_000_000),
                face_value: Amount::from_str("100.00").unwrap(),
                card_count: 10,
                card_form: CardForm::Electronic,
                effective_at: Instant::from_unix_secs(1_700_000_000),
                content_hash: "hash-1".to_string(),
            },
        )
        .unwrap()
    }

    fn delivery_with_revision_and_mall(rev_id: &str, mall: &str) -> SalesOrderProjectionDelivery {
        SalesOrderProjectionDelivery::new(
            crate::ids::SalesOrderProjectionDeliveryId::new("del-1"),
            SalesOrderProjectionDeliveryData {
                projection_revision_id: SalesOrderProjectionRevisionId::new(rev_id),
                target_mall_id: SourceSystemId::new(mall),
                status: crate::projection::ProjectionDeliveryStatus::PendingSend,
                attempt_count: 0,
                next_attempt_at: None,
                mall_ack_at: None,
                mall_execution_baseline: None,
                error_code: None,
                error_summary: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn relation_passes_when_all_segments_match() {
        let projection = projection_with_mall("mall-1");
        let revision = revision_with_projection("proj-1", "proj-rev-1", 1);
        let delivery = delivery_with_revision_and_mall("proj-rev-1", "mall-1");
        assert!(ensure_delivery_relation(&projection, &revision, &delivery).is_ok());
    }

    #[test]
    fn relation_rejects_wrong_projection() {
        let projection = projection_with_mall("mall-1");
        // revision belongs to different projection
        let revision = revision_with_projection("proj-2", "proj-rev-1", 1);
        let delivery = delivery_with_revision_and_mall("proj-rev-1", "mall-1");
        assert!(ensure_delivery_relation(&projection, &revision, &delivery).is_err());
    }

    #[test]
    fn relation_rejects_wrong_revision() {
        let projection = projection_with_mall("mall-1");
        let revision = revision_with_projection("proj-1", "proj-rev-1", 1);
        // delivery points to different revision
        let delivery = delivery_with_revision_and_mall("proj-rev-99", "mall-1");
        assert!(ensure_delivery_relation(&projection, &revision, &delivery).is_err());
    }

    #[test]
    fn relation_rejects_wrong_mall() {
        let projection = projection_with_mall("mall-1");
        let revision = revision_with_projection("proj-1", "proj-rev-1", 1);
        let delivery = delivery_with_revision_and_mall("proj-rev-1", "mall-2");
        assert!(ensure_delivery_relation(&projection, &revision, &delivery).is_err());
    }

    #[test]
    fn ack_monotonicity_old_new_and_same_seq_different_identity() {
        // old ack should not advance
        assert!(!SalesOrderProjection::should_advance_acked_revision(7, 6));
        // same seq (duplicate version) should advance when identity differs (monotonic allows)
        assert!(SalesOrderProjection::should_advance_acked_revision(7, 7));
        // new ack should advance
        assert!(SalesOrderProjection::should_advance_acked_revision(7, 8));
    }

    #[test]
    fn same_revision_duplicate_ack_is_detected_via_identity() {
        let mut projection = projection_with_mall("mall-1");
        projection
            .update(crate::projection::SalesOrderProjectionUpdate {
                current_acked_revision_id: Some(SalesOrderProjectionRevisionId::new("proj-rev-1")),
            })
            .unwrap();
        assert!(projection.is_same_acked_revision(&SalesOrderProjectionRevisionId::new("proj-rev-1")));
        assert!(!projection.is_same_acked_revision(&SalesOrderProjectionRevisionId::new("proj-rev-2")));
    }

    #[test]
    fn stale_command_version_is_rejected_by_delivery() {
        let delivery = delivery_with_revision_and_mall("proj-rev-1", "mall-1");
        let expected_version = delivery.base.version;
        assert!(delivery
            .ensure_matches_command(
                expected_version,
                &SalesOrderProjectionRevisionId::new("proj-rev-1")
            )
            .is_ok());
        // stale version
        assert!(delivery
            .ensure_matches_command(
                expected_version + 99,
                &SalesOrderProjectionRevisionId::new("proj-rev-1")
            )
            .is_err());
        // wrong revision identity
        assert!(delivery
            .ensure_matches_command(
                expected_version,
                &SalesOrderProjectionRevisionId::new("proj-rev-99")
            )
            .is_err());
    }

    #[test]
    fn revision_belongs_to_projection_validation() {
        let revision = revision_with_projection("proj-1", "proj-rev-1", 1);
        assert!(revision
            .ensure_belongs_to_projection(&SalesOrderProjectionId::new("proj-1"))
            .is_ok());
        assert!(revision
            .ensure_belongs_to_projection(&SalesOrderProjectionId::new("proj-2"))
            .is_err());
    }

    #[test]
    fn command_identity_mismatch_is_rejected() {
        use crate::ids::SalesOrderProjectionDeliveryId;
        use crate::projection::SalesOrderProjectionDelivery;
        assert!(SalesOrderProjectionDelivery::ensure_command_identity(
            &SalesOrderProjectionDeliveryId::new("del-1"),
            &SalesOrderProjectionDeliveryId::new("del-1")
        )
        .is_ok());
        assert!(SalesOrderProjectionDelivery::ensure_command_identity(
            &SalesOrderProjectionDeliveryId::new("del-1"),
            &SalesOrderProjectionDeliveryId::new("del-2")
        )
        .is_err());
    }
}
