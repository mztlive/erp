//! 域 D01 `source_registry` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as SourceRegistryExt>::SOURCE_SYSTEMS` 等值。

use mongodb::Database;

use super::super::source_registry::{
    ExternalIdentityMapFilter, SourceRegistryRepository, SourceSystemFilter,
};
use crate::repository::owned::{
    ExternalIdentityMapRepository, ExternalIdentityTargetRepository, SourceSystemRepository,
};

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

    /// 取得 `source_system` 集合的仓储，供来源系统登记和维护；由调用方提供执行器。
    fn source_systems(&self) -> SourceSystemRepository<'_>;

    /// 取得 `external_identity_map` 集合的仓储，供外部标识与内部身份关联的读写；由调用方提供执行器。
    fn external_identity_maps(&self) -> ExternalIdentityMapRepository<'_>;

    /// 取得 `external_identity_target` 集合的仓储，供外部映射目标及有效期的读写；由调用方提供执行器。
    fn external_identity_targets(&self) -> ExternalIdentityTargetRepository<'_>;

    /// 取得来源与外部身份映射的跨集合操作入口；原子写入沿用用例事务，不在仓储内开启事务。
    fn source_registry(&self) -> SourceRegistryRepository<'_>;
}

impl SourceRegistryExt for Database {
    type SourceSystemFilter = SourceSystemFilter;
    type ExternalIdentityMapFilter = ExternalIdentityMapFilter;

    fn source_systems(&self) -> SourceSystemRepository<'_> {
        SourceSystemRepository::new(self, Self::SOURCE_SYSTEMS)
    }

    fn external_identity_maps(&self) -> ExternalIdentityMapRepository<'_> {
        ExternalIdentityMapRepository::new(self, Self::EXTERNAL_IDENTITY_MAPS)
    }

    fn external_identity_targets(&self) -> ExternalIdentityTargetRepository<'_> {
        ExternalIdentityTargetRepository::new(self, Self::EXTERNAL_IDENTITY_TARGETS)
    }

    fn source_registry(&self) -> SourceRegistryRepository<'_> {
        SourceRegistryRepository::new(self)
    }
}
