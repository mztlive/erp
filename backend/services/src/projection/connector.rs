use std::pin::Pin;

use entities::ids::SourceSystemId;
use entities::integration_ops::ErrorClass;
use entities::projection::SalesOrderProjectionRevision;

/// 外部调用错误分类（错误分类：临时故障/限流可自动重试，其余转人工，§7.7）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedError {
    /// 错误分类。
    pub class: ErrorClass,
    /// 稳定错误码。
    pub code: String,
    /// 脱敏错误摘要。
    pub summary: String,
}

/// 商城下发确认（成功响应）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliverAck {
    /// 商城执行基线。
    pub mall_execution_baseline: String,
}

/// 查询原投递身份得到的权威结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryProjectionResult {
    /// 商城明确确认原投递。
    Confirmed(DeliverAck),
    /// 商城明确拒绝或明确未接收原投递。
    Failed(ClassifiedError),
    /// 当前仍不能判断原投递是否已生效。
    StillUnknown,
}

/// 商城连接器（外部下发调用统一入口）。
///
/// 实现要求（P3 §7、AGENTS.md 外部依赖容错）：统一设置超时（5 秒）、重试上限
/// （2 次）与错误分类；依赖失败降级为可观测错误。默认实现
/// [`UnavailableMallConnector`] 在目标商城端点不可解析时以分类错误失败关闭
/// （当前无端点注册表），测试注入 mock 验证成功与失败两条路径。
pub trait MallConnector: Send + Sync {
    /// 下发投影版本到目标商城。
    ///
    /// # 参数
    /// * `revision` - 待下发的投影版本
    /// * `target_mall_id` - 目标商城
    ///
    /// # 返回
    /// 下发成功返回商城确认（`mall_execution_baseline`）；失败返回分类错误。
    fn deliver_projection<'a>(
        &'a self,
        revision: &'a SalesOrderProjectionRevision,
        target_mall_id: &'a SourceSystemId,
    ) -> Pin<
        Box<dyn std::future::Future<Output = std::result::Result<DeliverAck, ClassifiedError>> + Send + 'a>,
    >;

    /// 按原稳定消息键查询投递最终结果。
    ///
    /// 实现不得把连接器不可用、超时或空响应映射为确认；无法取得权威结果时
    /// 必须返回 [`QueryProjectionResult::StillUnknown`]。
    fn query_projection<'a>(
        &'a self,
        revision: &'a SalesOrderProjectionRevision,
        target_mall_id: &'a SourceSystemId,
        message_key: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = QueryProjectionResult> + Send + 'a>>;
}

/// 默认商城连接器：端点不可解析时失败关闭（可观测降级）。
pub struct UnavailableMallConnector;

impl MallConnector for UnavailableMallConnector {
    /// 下发投影版本到目标商城（默认实现恒失败关闭）。
    ///
    /// # 参数
    /// * `revision` - 待下发的投影版本
    /// * `target_mall_id` - 目标商城
    ///
    /// # 返回
    /// 恒返回 `TransientFailure` 分类错误（商城端点未注册）。
    fn deliver_projection<'a>(
        &'a self,
        revision: &'a SalesOrderProjectionRevision,
        target_mall_id: &'a SourceSystemId,
    ) -> Pin<
        Box<dyn std::future::Future<Output = std::result::Result<DeliverAck, ClassifiedError>> + Send + 'a>,
    > {
        Box::pin(async move {
            Err(ClassifiedError {
                class: ErrorClass::TransientFailure,
                code: "MALL_ENDPOINT_UNRESOLVED".to_string(),
                summary: format!(
                    "投影修订 {} 下发目标商城 {} 失败关闭：端点未注册",
                    revision.base.id, target_mall_id
                ),
            })
        })
    }

    /// 默认端点不可解析时保留结果未知，不伪造商城确认。
    fn query_projection<'a>(
        &'a self,
        _revision: &'a SalesOrderProjectionRevision,
        _target_mall_id: &'a SourceSystemId,
        _message_key: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = QueryProjectionResult> + Send + 'a>> {
        Box::pin(async { QueryProjectionResult::StillUnknown })
    }
}

#[cfg(test)]
mod tests {
    use entities::integration_ops::ErrorClass;

    use crate::projection::{
        ClassifiedError, DeliverAck, MallConnector, QueryProjectionResult, UnavailableMallConnector,
    };

    fn sample_revision() -> entities::projection::SalesOrderProjectionRevision {
        entities::projection::SalesOrderProjectionRevision::new(
            entities::ids::SalesOrderProjectionRevisionId::new("proj-rev-1"),
            1,
            entities::projection::SalesOrderProjectionRevisionData {
                projection_id: entities::ids::SalesOrderProjectionId::new("proj-1"),
                projection_source: entities::projection::ProjectionSource::ErpRevision,
                sales_order_revision_id: entities::ids::SalesOrderRevisionId::new("so-rev-1"),
                customer_external_identity: "mall-customer-001".to_string(),
                voucher_category_external_identity: "mall-voucher-001".to_string(),
                voucher_expiry_at: entities::common::time::Instant::from_unix_secs(1_800_000_000),
                face_value: std::str::FromStr::from_str("100.00").unwrap(),
                card_count: 100,
                card_form: entities::projection::CardForm::Electronic,
                effective_at: entities::common::time::Instant::from_unix_secs(1_700_000_000),
                content_hash: "abc".to_string(),
            },
        )
        .unwrap()
    }

    #[tokio::test]
    async fn default_connector_fails_closed_with_classified_error() {
        let connector = UnavailableMallConnector;
        let revision = sample_revision();
        let error: ClassifiedError = connector
            .deliver_projection(&revision, &entities::ids::SourceSystemId::new("mall-1"))
            .await
            .expect_err("默认连接器必须失败关闭");
        assert_eq!(error.class, ErrorClass::TransientFailure);
        assert_eq!(error.code, "MALL_ENDPOINT_UNRESOLVED");
        assert_eq!(
            connector
                .query_projection(
                    &revision,
                    &entities::ids::SourceSystemId::new("mall-1"),
                    "projection_delivery:proj-rev-1:mall-1",
                )
                .await,
            QueryProjectionResult::StillUnknown
        );
    }

    #[tokio::test]
    async fn mock_connector_success_returns_ack() {
        struct MockConnector;
        impl MallConnector for MockConnector {
            fn deliver_projection<'a>(
                &'a self,
                revision: &'a entities::projection::SalesOrderProjectionRevision,
                _target_mall_id: &'a entities::ids::SourceSystemId,
            ) -> std::pin::Pin<
                Box<
                    dyn std::future::Future<Output = std::result::Result<DeliverAck, ClassifiedError>>
                        + Send
                        + 'a,
                >,
            > {
                Box::pin(async move {
                    Ok(DeliverAck {
                        mall_execution_baseline: format!("bl-{}", revision.base.id),
                    })
                })
            }

            fn query_projection<'a>(
                &'a self,
                revision: &'a entities::projection::SalesOrderProjectionRevision,
                _target_mall_id: &'a entities::ids::SourceSystemId,
                _message_key: &'a str,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = QueryProjectionResult> + Send + 'a>>
            {
                Box::pin(async move {
                    QueryProjectionResult::Confirmed(DeliverAck {
                        mall_execution_baseline: format!("bl-{}", revision.base.id),
                    })
                })
            }
        }
        let connector = MockConnector;
        let revision = sample_revision();
        let ack = connector
            .deliver_projection(&revision, &entities::ids::SourceSystemId::new("mall-1"))
            .await
            .expect("mock 连接器必须成功");
        assert!(ack.mall_execution_baseline.starts_with("bl-proj-rev-1"));
        assert!(matches!(
            connector
                .query_projection(
                    &revision,
                    &entities::ids::SourceSystemId::new("mall-1"),
                    "projection_delivery:proj-rev-1:mall-1",
                )
                .await,
            QueryProjectionResult::Confirmed(_)
        ));
    }
}
