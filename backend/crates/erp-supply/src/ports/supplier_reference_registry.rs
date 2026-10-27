//! 权威不透明引用注册表；内部引用禁止进入响应和日志。
use std::future::Future;
use std::pin::Pin;

use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};
use serde::{Deserialize, Serialize};

use super::supplier_api_gateway::ClassifiedError;
use crate::entity::failure::SupplierFailureClass;
use crate::entity::supplier_api::{SupplierApiCapabilityCode, SupplierApiConnection, SupplierConnectorType};

/// 本次绑定命令预检的目标身份；禁止以票据携带的身份替代目标连接。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierReferenceTarget {
    pub connection_id: SupplierApiConnectionId,
    pub connector_type: Option<SupplierConnectorType>,
    pub supplier_id: SupplierAccountId,
}

impl From<&SupplierApiConnection> for SupplierReferenceTarget {
    fn from(connection: &SupplierApiConnection) -> Self {
        Self {
            connection_id: SupplierApiConnectionId::new(connection.base.id.clone()),
            supplier_id: connection.supplier_id.clone(),
            connector_type: connection.connector_type,
        }
    }
}

/// 不透明引用种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplierReferenceKind {
    BusinessProfile,
    Endpoint,
    Credential,
}

/// 后台选择器可见的配置别名与短时票据；不包含地址、密钥或内部引用。
#[derive(Serialize)]
pub struct SupplierReferenceOption {
    pub reference_id: String,
    pub alias: String,
    pub version: String,
    pub expires_at: i64,
}

/// 连接专属配置选择器参数。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupplierReferenceOptionsQuery {
    pub kind: SupplierReferenceKind,
}

/// 权威引用注册表解析结果。
///
/// `internal_reference` 只能写入后端配置实体，不得进入列表、详情、审计消息或日志。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSupplierReference {
    pub internal_reference: String,
}

/// 当前部署为一个连接解析出的内部配置；不得序列化到响应或日志。
#[derive(Clone, PartialEq, Eq)]
pub struct SupplierDeploymentConfig {
    pub endpoint: String,
    pub credential: String,
}

/// 服务端不透明引用注册表端口。
pub trait SupplierReferenceRegistry: Send + Sync {
    /// 读取适配器实际提供的操作，不以历史数据库能力记录为依据。
    /// # 参数
    /// `target` 为服务端读取的连接身份。
    /// # 返回
    /// 适配器的只读能力清单。
    /// # 错误
    /// 未装配适配器时返回明确错误。
    fn capabilities(
        &self,
        _target: &SupplierReferenceTarget,
    ) -> Result<Vec<SupplierApiCapabilityCode>, ClassifiedError> {
        Err(ClassifiedError {
            class: SupplierFailureClass::CapabilityGap,
            code: "SUPPLIER_CONNECTOR_UNAVAILABLE".into(),
            summary: "供应商接口未配置，无法读取能力".into(),
        })
    }

    /// 按后台身份取得当前部署配置，无需用户选择地址或凭据。
    /// # 参数
    /// `target` 是服务器读取的连接身份。
    /// # 返回
    /// 仅供后端绑定及失效检测的配置引用。
    /// # 错误
    /// 配置未登记或类型不支持时明确拒绝。
    fn deployment_config(
        &self,
        _target: &SupplierReferenceTarget,
    ) -> Result<SupplierDeploymentConfig, ClassifiedError> {
        Err(ClassifiedError {
            class: SupplierFailureClass::CapabilityGap,
            code: "SUPPLIER_CONNECTOR_UNAVAILABLE".into(),
            summary: "服务端尚未配置该供应商接口，请联系管理员".into(),
        })
    }

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

    /// 列出适用于当前后台连接的可绑定配置。
    /// # 参数
    /// `kind` 为配置种类，`target` 来自本次读取的后台连接记录。
    /// # 返回
    /// 安全别名及短时票据；未装配目录时返回空列表。
    /// # 错误
    /// 配置目录不可用或票据签发失败时返回分类错误。
    fn options<'a>(
        &'a self,
        _kind: SupplierReferenceKind,
        _target: &'a SupplierReferenceTarget,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<SupplierReferenceOption>, ClassifiedError>> + Send + 'a>>
    {
        Box::pin(async { Ok(Vec::new()) })
    }

    /// 解析服务端签发的短时引用；实现必须校验目标身份、种类、用途和有效期。
    ///
    /// # 参数
    /// * `kind` - 引用种类。
    /// * `payload_reference` - 服务端签发的短时引用。
    /// * `target` - 本次命令预检的目标连接及供应商。
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
