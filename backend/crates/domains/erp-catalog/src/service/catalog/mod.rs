//! 商品字典、SPU/SKU、卡券与来源同步的本域用例。
//!
//! 公开写入口开启一个快照事务；授权、决定写入的事实、版本检查、写入和审计
//! 必须使用原执行器。跨域命令通过草稿写入步骤加入所属 Process 的事务。
//! 草稿准备只规范化输入及临时附件引用，不读取数据库或签发授权结果。
//! 文件与审计事实由本域窄 Port 提供；实体维护状态及修订不变式。
use std::sync::Arc;

use mongodb::Database;

use crate::ports::{CatalogAuditPort, FileAssetFactsPort};

mod access;
mod attribute;
mod brand;
mod brand_import;
mod category;
mod handover;
mod listing;
mod product_create;
mod product_detail;
mod product_disable;
mod product_query;
mod product_shared;
mod product_update;
mod sellable;
mod sku_edit;
mod source_sku;
mod source_sync;
mod support;
mod unit;
mod voucher;
mod voucher_defaults;

pub use self::sellable::{
    SellableSkuListParams, SellableSkuSpecificationAttributeView, SellableSkuView, sellable_sku_invalid_error,
};
pub use crate::dto::{
    CreateProductBrandRequest, CreateProductCategoryRequest, CreateProductRequest, CreateSkuAttributeRequest,
    CreateSkuAttributeValueRequest, CreateUnitOfMeasureRequest, CreateVoucherCategoryRequest,
    DisableProductRequest, MoveProductCategoryRequest, NewVoucherCategoryInput, PageView,
    ProductBrandListParams, ProductBrandView, ProductCategoryListParams, ProductCategoryParentChange,
    ProductCategoryView, ProductListParams, ProductListingView, ProductMediaInput, ProductRevisionListParams,
    ProductRevisionMediaView, ProductRevisionView, ProductSkuInput, ProductView, SkuAttributeListParams,
    SkuAttributeValueListParams, SkuAttributeValueView, SkuAttributeView, SkuListParams,
    SkuRevisionListParams, SkuRevisionView, SkuView, SpecEntryInput, UnitOfMeasureListParams,
    UnitOfMeasureView, UpdateProductBrandRequest, UpdateProductCategoryRequest, UpdateProductListingRequest,
    UpdateProductRequest, UpdateSkuAttributeRequest, UpdateSkuAttributeValueRequest, UpdateSkuListingRequest,
    UpdateUnitOfMeasureRequest, UpdateVoucherCategoryRequest, VoucherCategoryProfileListParams,
    VoucherCategoryProfileView, VoucherSkuInput,
};

/// 商品域服务。
///
/// 提供商品字典、SPU/SKU 与卡券类目的创建、查询、更新编排。
/// 商品修订规则留在本域；跨域附件原子编排的根事务由所属 Process 持有，本服务
/// 只通过 [`crate::ports::PendingAttachmentBatch`] 与 [`FileAssetFactsPort`] 消费附件事实。
#[derive(Clone)]
pub struct CatalogService {
    db: Database,
    audit: Arc<dyn CatalogAuditPort>,
    file_assets: Arc<dyn FileAssetFactsPort>,
    data_scope: CatalogAccess,
}

impl CatalogService {
    /// 创建商品域服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `audit` - 审计写入端口
    /// * `file_assets` - 媒体/Logo 文件存在性端口
    /// * `data_scope` - 已装配的商品授权能力
    ///
    /// # 返回
    /// 返回已装配授权能力的服务实例。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(
        db: Database,
        audit: Arc<dyn CatalogAuditPort>,
        file_assets: Arc<dyn FileAssetFactsPort>,
        data_scope: CatalogAccess,
    ) -> Self {
        Self { data_scope, db, audit, file_assets }
    }

    /// 构造本域范围访问器。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回绑定当前数据库与授权能力 的访问器。
    ///
    /// # 错误
    /// 无。
    pub(crate) fn access(&self) -> CatalogAccess {
        self.data_scope.clone()
    }
}

pub use access::CatalogAccess;
pub use product_create::ProductDraft;
pub use product_query::{prepare_product_list, product_page_view};
pub use product_update::SpecEditPlan;
pub use sellable::{prepare_sellable_sku_list, sellable_sku_page_view};

mod source_unit;

#[cfg(test)]
pub(super) mod testing {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;

    use super::*;
    use crate::ports::{EmptyFileAssetFacts, FailClosedAuditPort};
    use crate::{Error, Result};

    pub(super) async fn service() -> CatalogService {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("catalog_authorization_unit");
        CatalogService::new(
            db.clone(),
            Arc::new(FailClosedAuditPort),
            Arc::new(EmptyFileAssetFacts),
            CatalogAccess::new(db, unavailable_authorization()),
        )
    }

    pub(super) fn actor() -> AuditActor {
        AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin)
    }

    pub(super) fn denied<T>(result: Result<T>) {
        assert!(matches!(result, Err(Error::Internal(message)) if message == "测试授权设施不可用"));
    }
}
