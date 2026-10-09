//! 供应商技术调用消费方合同；外部调用不持有事务。
use crate::entity::failure::SupplierFailureClass;
use crate::entity::supplier_api::SupplierApiConnection;
/// 既有网关失败分类；是否可重试由编排层依据副作用及恢复证据判断。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedError {
    /// 错误分类。
    pub class: SupplierFailureClass,
    /// 稳定错误码。
    pub code: String,
    /// 脱敏错误摘要。
    pub summary: String,
}

/// 既有供应商连接检查与目录同步入口；新协议接入使用 connector 模块的窄 trait。
///
/// 单次调用必须设置超时并返回分类结果；调度层负责有限退避和恢复，协议适配器不得
/// 隐藏写重试。连接检查只读，不创建业务订单。默认实现
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
    ///
    /// # 错误
    /// 检查失败时返回 `ClassifiedError`，由 `class` 区分可自动重试或转人工。
    fn health_check<'a>(
        &'a self,
        connection: &'a SupplierApiConnection,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = std::result::Result<(), ClassifiedError>> + Send + 'a>,
    >;

    /// 保留既有目录同步调用入口；新实现由 OfferSource 读取，再由 Process 调用正式供给用例。
    ///
    /// 协议适配器不得直接写 ERP 仓储。切换前必须补齐分页、来源核验和断点恢复，
    /// 不能把读取 HTTP 成功直接解释为目录业务已经同步完成。
    ///
    /// # 参数
    /// * `connection` - 目标供应商 API 连接。
    ///
    /// # 返回
    /// 覆盖实现同步成功时返回 `Ok(())`。默认实现不成功。
    ///
    /// # 错误
    /// 默认实现返回 `ClassifiedError`，`class` 为 `SupplierFailureClass::TransientFailure`，
    /// `code` 为 `CATALOG_SYNC_ADAPTER_UNAVAILABLE`。覆盖实现失败时同样返回 `ClassifiedError`。
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
