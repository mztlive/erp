//! 域 D26 `publication` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as PublicationExt>::PRODUCT_PUBLICATIONS` 等值。

use entities::publication::{
    ProductPublication, ProductPublicationDelivery, ProductPublicationRevision,
    ProductPublicationRevisionMedia,
};
use mongodb::Database;

use super::super::publication::{
    ProductPublicationDeliveryFilter, ProductPublicationFilter, PublicationRepository,
};
use crate::Repository;

/// 域 D26 仓储访问器。
pub trait PublicationExt {
    /// `product_publication` 集合名。
    const PRODUCT_PUBLICATIONS: &'static str = "product_publications";
    /// `product_publication_revision` 集合名。
    const PRODUCT_PUBLICATION_REVISIONS: &'static str = "product_publication_revisions";
    /// `product_publication_revision_media` 集合名。
    const PRODUCT_PUBLICATION_REVISION_MEDIA: &'static str = "product_publication_revision_media";
    /// `product_publication_delivery` 集合名。
    const PRODUCT_PUBLICATION_DELIVERIES: &'static str = "product_publication_deliveries";

    /// 发布列表筛选条件类型（定义见 `repository::publication`）。
    type ProductPublicationFilter;

    /// 发布投递列表筛选条件类型（定义见 `repository::publication`）。
    type ProductPublicationDeliveryFilter;

    /// 获取 `product_publication` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::publication::ProductPublication>`。
    fn product_publications(&self) -> Repository<'_, ProductPublication>;

    /// 获取 `product_publication_revision` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::publication::ProductPublicationRevision>`。
    fn product_publication_revisions(&self) -> Repository<'_, ProductPublicationRevision>;

    /// 获取 `product_publication_revision_media` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::publication::ProductPublicationRevisionMedia>`。
    fn product_publication_revision_media(&self) -> Repository<'_, ProductPublicationRevisionMedia>;

    /// 获取 `product_publication_delivery` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::publication::ProductPublicationDelivery>`。
    fn product_publication_deliveries(&self) -> Repository<'_, ProductPublicationDelivery>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `PublicationRepository` 实例。
    fn publication(&self) -> PublicationRepository<'_>;
}

impl PublicationExt for Database {
    type ProductPublicationFilter = ProductPublicationFilter;
    type ProductPublicationDeliveryFilter = ProductPublicationDeliveryFilter;

    fn product_publications(&self) -> Repository<'_, ProductPublication> {
        Repository::new(self, Self::PRODUCT_PUBLICATIONS)
    }

    fn product_publication_revisions(&self) -> Repository<'_, ProductPublicationRevision> {
        Repository::new(self, Self::PRODUCT_PUBLICATION_REVISIONS)
    }

    fn product_publication_revision_media(&self) -> Repository<'_, ProductPublicationRevisionMedia> {
        Repository::new(self, Self::PRODUCT_PUBLICATION_REVISION_MEDIA)
    }

    fn product_publication_deliveries(&self) -> Repository<'_, ProductPublicationDelivery> {
        Repository::new(self, Self::PRODUCT_PUBLICATION_DELIVERIES)
    }

    fn publication(&self) -> PublicationRepository<'_> {
        PublicationRepository::new(self)
    }
}
