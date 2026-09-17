//! 直通委托 owned 仓储的共享声明宏（erp-customer-006）。
//!
//! 三个 owned 仓储（客户账户、归属、资料命令）除实体类型外全部同构；
//! 本宏生成 `new` 与 `Deref<Target = Repository<T>>` 转发，特化查询仍保留
//! 在各文件的原 inherent 实现模块中。

/// 为 owned 仓储生成构造与泛型委托（erp-customer-006）。
///
/// 生成的 `Deref` 使 `create/find_by_id/update/soft_delete/restore` 等通用
/// 方法经 persistence-core 解析，不再逐文件手写直通委托；公开签名不变。
macro_rules! owned_repository {
    ($repository:ident, $entity:ty) => {
        /// Owned repository 包装。
        ///
        /// 组合 persistence-core 泛型仓储并经 `Deref` 转发通用方法。
        pub struct $repository<'a> {
            inner: persistence_core::Repository<'a, $entity>,
        }

        impl<'a> $repository<'a> {
            /// 创建绑定到集合名的 owned 仓储。
            ///
            /// # 参数
            /// * `db` - MongoDB 数据库句柄
            /// * `collection_name` - 实体集合名
            ///
            /// # 返回
            /// 返回委托泛型存储的 owned 仓储。
            pub fn new(db: &'a mongodb::Database, collection_name: &'a str) -> Self {
                Self { inner: persistence_core::Repository::new(db, collection_name) }
            }
        }

        impl<'a> Deref for $repository<'a> {
            type Target = persistence_core::Repository<'a, $entity>;

            /// 返回组合的泛型仓储。
            ///
            /// # 参数
            /// * `self` - owned 仓储
            ///
            /// # 返回
            /// 返回 persistence-core 泛型仓储引用，通用 CRUD 经它解析。
            fn deref(&self) -> &Self::Target {
                &self.inner
            }
        }
    };
}

pub(crate) use owned_repository;
