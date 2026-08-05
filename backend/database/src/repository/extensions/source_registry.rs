//! 域 D01 `source_registry` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as SourceRegistryExt>::SOURCE_SYSTEMS` 等值。

use entities::source_registry::{ExternalIdentityMap, ExternalIdentityTarget, SourceSystem};
use mongodb::Database;

use super::super::source_registry::{
    ExternalIdentityMapFilter, SourceRegistryRepository, SourceSystemFilter,
};
use crate::Repository;

/// 域 D01 仓储访问器。
pub trait SourceRegistryExt {
    /// `source_system` 集合名。
    const SOURCE_SYSTEMS: &'static str = "source_systems";
    /// `external_identity_map` 集合名。
    const EXTERNAL_IDENTITY_MAPS: &'static str = "external_identity_maps";
    /// `external_identity_target` 集合名。
    const EXTERNAL_IDENTITY_TARGETS: &'static str = "external_identity_targets";

    /// 来源系统列表筛选条件类型（定义见 `repository::source_registry`）。
    type SourceSystemFilter;

    /// 外部身份映射列表筛选条件类型（定义见 `repository::source_registry`）。
    type ExternalIdentityMapFilter;

    /// 获取 `source_system` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::source_registry::SourceSystem>`。
    fn source_systems(&self) -> Repository<'_, SourceSystem>;

    /// 获取 `external_identity_map` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::source_registry::ExternalIdentityMap>`。
    fn external_identity_maps(&self) -> Repository<'_, ExternalIdentityMap>;

    /// 获取 `external_identity_target` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::source_registry::ExternalIdentityTarget>`。
    fn external_identity_targets(&self) -> Repository<'_, ExternalIdentityTarget>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `SourceRegistryRepository` 实例。
    fn source_registry(&self) -> SourceRegistryRepository<'_>;
}

impl SourceRegistryExt for Database {
    type SourceSystemFilter = SourceSystemFilter;
    type ExternalIdentityMapFilter = ExternalIdentityMapFilter;

    fn source_systems(&self) -> Repository<'_, SourceSystem> {
        Repository::new(self, Self::SOURCE_SYSTEMS)
    }

    fn external_identity_maps(&self) -> Repository<'_, ExternalIdentityMap> {
        Repository::new(self, Self::EXTERNAL_IDENTITY_MAPS)
    }

    fn external_identity_targets(&self) -> Repository<'_, ExternalIdentityTarget> {
        Repository::new(self, Self::EXTERNAL_IDENTITY_TARGETS)
    }

    fn source_registry(&self) -> SourceRegistryRepository<'_> {
        SourceRegistryRepository::new(self)
    }
}
