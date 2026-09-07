//! 权威不透明引用注册表；内部引用禁止进入响应和日志。
use super::supplier_api_gateway::ClassifiedError;
/// 不透明引用种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupplierReferenceKind {
    BusinessProfile,
    Endpoint,
    Credential,
}

/// 权威引用注册表解析结果。
///
/// `internal_reference` 只能写入后端配置实体，不得进入列表、详情、审计消息或日志。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSupplierReference {
    pub internal_reference: String,
}

/// 服务端不透明引用注册表端口。
pub trait SupplierReferenceRegistry: Send + Sync {
    /// 判断当前进程是否已注入权威注册表。
    fn is_available(&self) -> bool;

    /// 解析服务端签发的短时引用；实现必须校验种类、环境、用途和有效期。
    fn resolve<'a>(
        &'a self,
        kind: SupplierReferenceKind,
        payload_reference: &'a str,
        environment: crate::entity::supplier_api::ConnectionEnvironment,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = std::result::Result<ResolvedSupplierReference, ClassifiedError>>
                + Send
                + 'a,
        >,
    >;
}
