//! 权威不透明引用注册表；内部引用禁止进入响应和日志。
use std::future::Future;
use std::pin::Pin;

use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};

use super::supplier_api_gateway::ClassifiedError;
use crate::entity::supplier_api::{ConnectionEnvironment, SupplierApiConnection};

/// 本次绑定命令预检的目标身份；禁止以票据携带的身份替代目标连接。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierReferenceTarget {
    pub connection_id: SupplierApiConnectionId,
    pub supplier_id: SupplierAccountId,
    pub environment: ConnectionEnvironment,
}

impl From<&SupplierApiConnection> for SupplierReferenceTarget {
    fn from(connection: &SupplierApiConnection) -> Self {
        Self {
            connection_id: SupplierApiConnectionId::new(connection.base.id.clone()),
            supplier_id: connection.supplier_id.clone(),
            environment: connection.environment,
        }
    }
}

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
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 已注入权威注册表时为 `true`，否则为 `false`。
    ///
    /// # 错误
    /// 不返回错误。
    fn is_available(&self) -> bool;

    /// 解析服务端签发的短时引用；实现必须校验目标身份、种类、用途和有效期。
    ///
    /// # 参数
    /// * `kind` - 引用种类。
    /// * `payload_reference` - 服务端签发的短时引用。
    /// * `target` - 本次命令预检的目标连接、供应商及环境。
    ///
    /// # 返回
    /// 校验通过时返回仅供后端配置写入的 `ResolvedSupplierReference`。
    ///
    /// # 错误
    /// 目标身份、种类、用途或有效期不通过时返回 `ClassifiedError`。
    fn resolve<'a>(
        &'a self,
        kind: SupplierReferenceKind,
        payload_reference: &'a str,
        target: &'a SupplierReferenceTarget,
    ) -> Pin<Box<dyn Future<Output = Result<ResolvedSupplierReference, ClassifiedError>> + Send + 'a>>;
}
