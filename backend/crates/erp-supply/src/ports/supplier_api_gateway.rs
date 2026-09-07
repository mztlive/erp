//! 供应商技术调用消费方合同；外部调用不持有事务。
use crate::entity::failure::SupplierFailureClass;
use crate::entity::supplier_api::SupplierApiConnection;
/// 外部调用错误分类（错误分类：临时故障/限流可自动重试，其余转人工，§7.7）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedError {
    /// 错误分类。
    pub class: SupplierFailureClass,
    /// 稳定错误码。
    pub code: String,
    /// 脱敏错误摘要。
    pub summary: String,
}

/// 供应商 API 网关（外部 HTTP 调用统一入口）。
///
/// 实现要求（P3 §7、AGENTS.md 外部依赖容错）：统一设置超时（5 秒）、重试上限
/// （2 次）与错误分类；依赖失败降级为可观测错误。默认实现
/// `UnavailableSupplierApiGateway` 在端点引用无法解析为可调用地址时以分类错误
/// 失败关闭（当前无地址配置注册表，`config://` 引用不可解析），测试注入 mock 验证
/// 成功与失败两条路径。
pub trait SupplierApiGateway: Send + Sync {
    /// 执行一次连接健康检查（生产检查不创建业务订单，phase-2 §14.1）。
    ///
    /// # 参数
    /// * `connection` - 目标连接（提供端点引用与限流策略上下文）
    ///
    /// # 返回
    /// 检查成功返回 `Ok(())`；失败返回分类错误（可自动重试或转人工）。
    fn health_check<'a>(
        &'a self,
        connection: &'a SupplierApiConnection,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = std::result::Result<(), ClassifiedError>> + Send + 'a>,
    >;

    /// 执行一次目录同步；实现必须保持来源幂等，并且只能写入 W21 正式供给链路。
    fn catalog_sync<'a>(
        &'a self,
        connection: &'a SupplierApiConnection,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = std::result::Result<(), ClassifiedError>> + Send + 'a>,
    > {
        Box::pin(async move {
            Err(ClassifiedError {
                class: SupplierFailureClass::TransientFailure,
                code: "CATALOG_SYNC_ADAPTER_UNAVAILABLE".to_string(),
                summary: format!("连接 {} 未注入目录同步适配器", connection.connection_code),
            })
        })
    }
}
