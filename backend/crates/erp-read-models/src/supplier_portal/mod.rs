//! 独立门户的跨域允许列表投影；正式规则及写入仍由所属领域执行。

mod applications;
mod catalog;
mod image_source;
mod impacts;
pub use image_source::{PortalSkuImageSource, portal_sku_image_source};
mod management;
pub use impacts::{
    PortalImpactLine, PortalImpactTask, PortalOfferingImpactReadService, PortalOfferingImpactView,
    PortalPurchaseImpactView,
};
mod repository;
mod review_context;
mod session;
mod views;
mod warnings;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
pub use catalog::PortalCatalogSku;
pub use management::PortalGrantView;
use mongodb::Database;
use persistence_core::Executor;
pub use review_context::{ExistingOfferingReviewCandidate, PortalNewProductReviewContext};
use serde::Deserialize;
pub use session::{PortalCooperationView, PortalSessionView};
pub use views::{PortalApplicationView, PortalDecisionView, PortalSubmissionView};
pub use warnings::{PurchaseSupplyWarningReader, PurchaseSupplyWarnings, SupplyInterruptionWarning};

use crate::Result;
pub use crate::supplier_center::offering::portal::{
    PortalOfferingParams, PortalOfferingReadService, PortalOfferingView, PortalRevisionView,
};

/// 外部统一分页筛选，只能收窄当前供应商的授权结果。
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortalListParams {
    pub q: Option<String>,
    pub status: Option<String>,
    pub page: Option<u64>,
    pub page_size: Option<u32>,
}

/// 内部申请查询明确指定供应商，不能发起全库申请扫描。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortalAdminListParams {
    pub supplier_id: String,
    pub q: Option<String>,
    pub status: Option<String>,
    pub page: Option<u64>,
    pub page_size: Option<u32>,
}

/// 内部请求读取资格，由组合层复用供应商及供给对象规则。
#[async_trait]
pub trait PortalReadAuthorizationPort: Send + Sync {
    /// 在调用方执行器上重验精确申请及其供应商、供给对象资格。
    ///
    /// # 参数
    /// `actor` 为内部身份；`request_id` 为申请；`executor` 为本次读取执行器。
    /// # 返回
    /// 可读返回 `true`，不存在或越权返回 `false`。
    /// # 错误
    /// 授权事实或持久化错误时拒绝。
    async fn request_readable(
        &self,
        actor: &AuditActor,
        request_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool>;

    /// 核对内部账号对目标供应商的正式详情资格。
    ///
    /// # 参数
    /// `actor` 为内部身份；`supplier_id` 是服务器解析目标；`executor` 为本次执行器。
    /// # 返回
    /// 可读返回 `true`，范围外返回 `false`。
    /// # 错误
    /// 事实查询失败时拒绝。
    async fn supplier_readable(
        &self,
        actor: &AuditActor,
        supplier_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool>;

    /// 逐个验证已有供给的正式详情范围，不能继承新品申请的供应商范围。
    ///
    /// # 参数
    /// `actor` 为内部身份；`offering_id` 为服务器解析的供给；`executor` 为本次执行器。
    /// # 返回
    /// 可读返回 `true`，不存在或越权返回 `false`。
    /// # 错误
    /// 授权事实或持久化错误时拒绝。
    async fn offering_readable(
        &self,
        actor: &AuditActor,
        offering_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool>;
}

/// 门户会话、目录与三类申请的跨域读取入口。
pub struct SupplierPortalReadService {
    db: Database,
    authorization: Option<Arc<dyn PortalReadAuthorizationPort>>,
}

impl SupplierPortalReadService {
    /// 构造外部读取入口，会话与供应商状态由请求中间件重验。
    ///
    /// # 参数
    /// `db` 为组合根提供的数据库。
    /// # 返回
    /// 默认拒绝内部全量读取的服务。
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db, authorization: None }
    }

    /// 注入内部对象及任务读取授权，不扩大门户账号范围。
    ///
    /// # 参数
    /// `authorization` 为组合根的真实授权适配器。
    /// # 返回
    /// 支持已授权内部读取的同一服务。
    /// # 错误
    /// 无。
    pub fn with_authorization(mut self, authorization: Arc<dyn PortalReadAuthorizationPort>) -> Self {
        self.authorization = Some(authorization);
        self
    }
}
