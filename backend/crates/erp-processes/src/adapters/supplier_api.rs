//! 供应商 API 与引用注册表的默认失败关闭生产实现。
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::ports::supplier_api_gateway::{ClassifiedError, SupplierApiGateway};
use erp_supply::ports::supplier_reference_registry::{
    ResolvedSupplierReference, SupplierReferenceKind, SupplierReferenceRegistry,
};
/// 未注入引用注册表时的默认失败关闭实现。
pub struct UnavailableSupplierReferenceRegistry;

impl SupplierReferenceRegistry for UnavailableSupplierReferenceRegistry {
    fn is_available(&self) -> bool {
        false
    }

    fn resolve<'a>(
        &'a self,
        _kind: SupplierReferenceKind,
        _payload_reference: &'a str,
        _environment: erp_supply::entity::supplier_api::ConnectionEnvironment,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = std::result::Result<ResolvedSupplierReference, ClassifiedError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async {
            Err(ClassifiedError {
                class: SupplierFailureClass::AuthSignature,
                code: "REFERENCE_REGISTRY_UNAVAILABLE".to_string(),
                summary: "未注入权威引用注册表，引用绑定已失败关闭".to_string(),
            })
        })
    }
}

/// 默认网关：端点引用不可解析时失败关闭（可观测降级）。
pub struct UnavailableSupplierApiGateway;

impl SupplierApiGateway for UnavailableSupplierApiGateway {
    /// 执行一次连接健康检查（默认实现恒失败关闭）。
    ///
    /// # 参数
    /// * `connection` - 目标连接
    ///
    /// # 返回
    /// 恒返回 `TransientFailure` 分类错误（端点引用为配置引用，未注册可调用地址）。
    fn health_check<'a>(
        &'a self,
        connection: &'a SupplierApiConnection,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = std::result::Result<(), ClassifiedError>> + Send + 'a>,
    > {
        Box::pin(async move {
            Err(ClassifiedError {
                class: SupplierFailureClass::TransientFailure,
                code: "ENDPOINT_UNRESOLVED".to_string(),
                summary: format!(
                    "连接 {} 的端点引用未解析为可调用地址，健康检查失败关闭",
                    connection.connection_code
                ),
            })
        })
    }
}

#[cfg(test)]
mod tests {

    use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};
    use erp_supply::entity::failure::SupplierFailureClass;
    use erp_supply::entity::supplier_api::{
        ConnectionEnvironment, SupplierApiConnection, SupplierApiConnectionData, SupplierApiConnectionStatus,
    };

    use super::{ClassifiedError, SupplierApiGateway, UnavailableSupplierApiGateway};

    fn sample_connection() -> SupplierApiConnection {
        SupplierApiConnection::new(
            SupplierApiConnectionId::new("conn-1"),
            SupplierApiConnectionData {
                supplier_id: SupplierAccountId::new("sup-1"),
                connection_code: "CN-1".to_string(),
                environment: ConnectionEnvironment::Production,
                endpoint_reference: "config://supplier/001".to_string(),
                credential_reference: Some("kms://prod/sup-001".to_string()),
                rate_limit_policy: None,
                status: SupplierApiConnectionStatus::Active,
            },
            "admin-1",
        )
        .unwrap()
    }

    #[tokio::test]
    async fn default_gateway_fails_closed_with_classified_error() {
        let gateway = UnavailableSupplierApiGateway;
        let error: ClassifiedError =
            gateway.health_check(&sample_connection()).await.expect_err("默认网关必须失败关闭");
        assert_eq!(error.class, SupplierFailureClass::TransientFailure);
        assert_eq!(error.code, "ENDPOINT_UNRESOLVED");
    }
}
