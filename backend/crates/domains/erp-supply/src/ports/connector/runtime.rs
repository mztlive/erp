//! 协议运行时的唯一装配合同；实现由组合层提供，目录分派复用实际窄接口。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::entity::failure::SupplierFailureClass;
use crate::entity::supplier_api::{SupplierApiConnection, SupplierConnectorType, SupplierHealthCheckType};
use crate::ports::catalog_sync::CatalogSyncPolicy;
use crate::ports::connector::SupplierConnector;
use crate::ports::connector::common::{ConnectorError, Scan};
use crate::ports::supplier_api_gateway::{
    CatalogPage, CatalogRead, CatalogScopeRead, ClassifiedError, SupplierApiGateway,
};
use crate::ports::supplier_reference_registry::SupplierReferenceRegistry;

/// 由服务端调用路径决定的绑定用途，不接受客户端提交。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorPurpose {
    /// 目录同步不受新增下单开关及订单资金绑定控制。
    Catalog,
    /// 回调及消息消费只校验技术身份，不借用订单资金准入。
    Notification,
    /// 创建与尚未发送的确认必须校验当前新增下单资格。
    Creation,
    /// 原订单调查、取消及实收通知保留原订单账号约束。
    Fulfillment,
    /// 既有售后及结算来源不使用新增下单或资金支付资格。
    AfterSales,
}

/// 一个协议运行时提供技术治理与用途绑定，所有业务能力来自绑定后的 Connector。
pub trait SupplierRuntime: SupplierReferenceRegistry {
    /// 返回本运行时唯一实现的协议类型。
    /// # 参数
    /// 无。
    /// # 返回
    /// 与部署配置一致的协议类型。
    /// # 错误
    /// 不返回错误。
    fn connector_type(&self) -> SupplierConnectorType;

    /// 按调用用途验证服务端连接并返回实际能力。
    /// # 参数
    /// `connection` 为服务端记录，`purpose` 为业务流程指定用途。
    /// # 返回
    /// 身份及用途资格已核对的连接器。
    /// # 错误
    /// 技术绑定失效、跨账号或对应业务资格不足时拒绝。
    fn bind(
        &self,
        connection: &SupplierApiConnection,
        purpose: ConnectorPurpose,
    ) -> Result<Arc<dyn SupplierConnector>, ClassifiedError>;

    /// 返回目录调度及映射版本，不要求已打开自动下单。
    /// # 参数
    /// `connection` 为服务端连接。
    /// # 返回
    /// 有目录实现时返回策略，未实现时返回 None。
    /// # 错误
    /// 连接身份或配置非法时返回分类错误。
    fn catalog_schedule(
        &self,
        connection: &SupplierApiConnection,
    ) -> Result<Option<CatalogSyncPolicy>, ClassifiedError>;

    /// 返回配置已核实的可供观察有效期。
    /// # 参数
    /// `connection` 为服务端连接。
    /// # 返回
    /// 秒数或未知，不推断永久有效。
    /// # 错误
    /// 不返回错误。
    fn availability_validity(&self, _connection: &SupplierApiConnection) -> Option<u64> {
        None
    }

    /// 执行独立只读健康检查，不创建业务订单。
    /// # 参数
    /// 服务端连接与冻结检查种类。
    /// # 返回
    /// 对应检查完成的结果。
    /// # 错误
    /// 绑定、认证或外部读取失败时返回分类错误。
    fn check_health<'a>(
        &'a self,
        connection: &'a SupplierApiConnection,
        check: SupplierHealthCheckType,
    ) -> Pin<Box<dyn Future<Output = Result<(), ClassifiedError>> + Send + 'a>>;
}

/// 在公共入口复验协议与返回连接身份，阻止错误适配器串用账号。
/// # 参数
/// 已注册运行时、服务端连接及本次用途。
/// # 返回
/// 与请求连接身份完全一致的连接器。
/// # 错误
/// 协议、身份或用途资格不匹配时返回分类错误。
pub fn bind_checked<R: SupplierRuntime + ?Sized>(
    runtime: &R,
    connection: &SupplierApiConnection,
    purpose: ConnectorPurpose,
) -> Result<Arc<dyn SupplierConnector>, ClassifiedError> {
    if connection.base.is_deleted() || connection.connector_type != Some(runtime.connector_type()) {
        return Err(binding_error());
    }
    let connector = runtime.bind(connection, purpose)?;
    if connector.connection_id().as_ref() != connection.base.id {
        return Err(binding_error());
    }
    Ok(connector)
}

fn binding_error() -> ClassifiedError {
    ClassifiedError {
        class: SupplierFailureClass::AuthSignature,
        code: "SUPPLIER_CONNECTOR_BINDING_MISMATCH".into(),
        summary: "供应商接口与当前连接身份不一致".into(),
    }
}

fn classified(error: ConnectorError) -> ClassifiedError {
    ClassifiedError { class: error.class, code: error.code, summary: error.summary }
}

fn missing_catalog() -> ClassifiedError {
    ClassifiedError {
        class: SupplierFailureClass::CapabilityGap,
        code: "CATALOG_SYNC_ADAPTER_UNAVAILABLE".into(),
        summary: "当前接口尚不支持商品同步".into(),
    }
}

impl<T: SupplierRuntime + ?Sized> SupplierApiGateway for T {
    fn catalog_connector_types(&self) -> Vec<SupplierConnectorType> {
        vec![self.connector_type()]
    }

    fn catalog_policy(
        &self,
        connection: &SupplierApiConnection,
    ) -> Result<Option<CatalogSyncPolicy>, ClassifiedError> {
        self.catalog_schedule(connection)
    }

    fn catalog_availability_validity(&self, connection: &SupplierApiConnection) -> Option<u64> {
        self.availability_validity(connection)
    }

    fn catalog_scope<'a>(&'a self, connection: &'a SupplierApiConnection) -> CatalogScopeRead<'a> {
        Box::pin(async move {
            let connector = bind_checked(self, connection, ConnectorPurpose::Catalog)?;
            connector.offers().ok_or_else(missing_catalog)?.scope().await.map_err(classified)
        })
    }

    fn catalog_page<'a>(&'a self, connection: &'a SupplierApiConnection, scan: &'a Scan) -> CatalogRead<'a> {
        Box::pin(async move {
            let connector = bind_checked(self, connection, ConnectorPurpose::Catalog)?;
            let page = connector
                .offers()
                .ok_or_else(missing_catalog)?
                .scan_scoped(scan)
                .await
                .map_err(classified)?;
            Ok(CatalogPage {
                items: page.page.items,
                next: page.page.next,
                skipped: page.skipped,
                membership: page.membership,
            })
        })
    }

    fn health_check<'a>(
        &'a self,
        connection: &'a SupplierApiConnection,
        check: SupplierHealthCheckType,
    ) -> Pin<Box<dyn Future<Output = Result<(), ClassifiedError>> + Send + 'a>> {
        self.check_health(connection, check)
    }
}
