//! 供应商技术调用消费方合同；外部调用不持有事务。
use std::future::Future;
use std::pin::Pin;

use super::connector::common::{Page, Scan, Snapshot};
use super::connector::offer::{IncrementalSupport, OfferChange};
use crate::entity::failure::SupplierFailureClass;
use crate::entity::supplier_api::{SupplierApiConnection, SupplierHealthCheckType};
/// 一页标准商品来源记录。
pub type CatalogPage = Page<Snapshot<OfferChange>>;
/// 事务外目录读取 Future。
pub type CatalogRead<'a> = Pin<Box<dyn Future<Output = Result<CatalogPage, ClassifiedError>> + Send + 'a>>;

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
/// 未注册的供应商以明确能力缺口失败；已注册协议使用当前服务端配置，
/// 由 Process 保存分页目录，不允许协议客户端直接修改 ERP 数据。
pub trait SupplierApiGateway: Send + Sync {
    /// 声明当前连接实际可用的增量覆盖。
    /// # 参数
    /// `connection` 为服务端已核验连接。
    /// # 返回
    /// 未提供实现时明确返回不支持增量。
    /// # 错误
    /// 类型不明或部署不可用时可返回明确错误。
    fn catalog_incremental_support(
        &self,
        _connection: &SupplierApiConnection,
    ) -> Result<IncrementalSupport, ClassifiedError> {
        Ok(IncrementalSupport::Unsupported)
    }

    /// 返回部署已核实的可供观察有效期。
    /// # 参数
    /// 当前连接。
    /// # 返回
    /// 秒数；未配置保持未知，不能伪造持续有效。
    /// # 错误
    /// 不返回错误；技术绑定仍由实际读取核验。
    fn catalog_availability_validity(&self, _connection: &SupplierApiConnection) -> Option<u64> {
        None
    }

    /// 返回部署规定的完整目录校对周期。
    /// # 参数
    /// 当前连接。
    /// # 返回
    /// 秒数；默认每天重新完整读取，不据缺项推断下架。
    /// # 错误
    /// 不返回错误。
    fn catalog_full_sync_interval(&self, _connection: &SupplierApiConnection) -> u64 {
        86_400
    }
    /// 读取一页供应商商品；数据库写入由 Process 执行。
    /// # 参数
    /// `connection` 为当前连接，`scan` 为持久化游标和固定页大小。
    /// # 返回
    /// 标准化来源记录及下一页游标。
    /// # 错误
    /// 未实现或供应商调用失败时返回明确错误。
    fn catalog_page<'a>(
        &'a self,
        _connection: &'a SupplierApiConnection,
        _scan: &'a Scan,
    ) -> CatalogRead<'a> {
        Box::pin(async {
            Err(ClassifiedError {
                class: SupplierFailureClass::CapabilityGap,
                code: "CATALOG_SYNC_ADAPTER_UNAVAILABLE".into(),
                summary: "当前接口尚不支持商品同步".into(),
            })
        })
    }

    /// 执行一次连接健康检查（生产检查不创建业务订单，phase-2 §14.1）。
    ///
    /// # 参数
    /// * `connection` - 目标连接（提供端点引用与限流策略上下文）
    /// * `check_type` - 本次启动运行冻结的检查种类；不可用其他种类的成功替代。
    ///
    /// # 返回
    /// 检查成功返回 `Ok(())`；失败返回分类错误（可自动重试或转人工）。
    ///
    /// # 错误
    /// 检查失败时返回 `ClassifiedError`，由 `class` 区分可自动重试或转人工。
    fn health_check<'a>(
        &'a self,
        connection: &'a SupplierApiConnection,
        check_type: SupplierHealthCheckType,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = std::result::Result<(), ClassifiedError>> + Send + 'a>,
    >;
}
