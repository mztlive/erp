//! 供应商仓储完整执行 identity 编译阶段，不解释权限规则。

use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, QueryFilter, Repository, aggregate};
use serde::Deserialize;

use super::supplier::SupplierAccountFilter;
use crate::entity::supplier::SupplierAccount;

/// 身份域编译查询凭证；业务不解释权限条款。
pub type SupplierReadScope = crate::authorization::CompiledScope;
/// 跨页校验使用的供应商身份和版本，不包含展示字段。
#[derive(Debug, Deserialize)]
pub struct SupplierVersion {
    /// 供应商稳定主键。
    pub id: String,
    /// 供应商乐观锁版本。
    pub version: u64,
}

/// 供应商账号集合上的授权范围查询。
#[allow(async_fn_in_trait)]
pub trait SupplierAccountRepositoryScopeExt {
    /// 装载查询的有界身份与版本集合，用于跨页及导出一致性校验。
    ///
    /// # 参数
    /// * `filter` - 已与授权条件求交的列表筛选
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 最多 10001 行；调用方必须整体拒绝超限。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    async fn query_versions(
        &self,
        filter: &SupplierAccountFilter,
        executor: &mut dyn Executor,
    ) -> crate::Result<Vec<SupplierVersion>>;
}

impl SupplierAccountRepositoryScopeExt for Repository<'_, SupplierAccount> {
    async fn query_versions(
        &self,
        filter: &SupplierAccountFilter,
        executor: &mut dyn Executor,
    ) -> crate::Result<Vec<SupplierVersion>> {
        filter.authorized_scope.ensure_current(executor)?;
        let result = aggregate::find(
            &self.collection().clone_with_type::<SupplierVersion>(),
            filter.to_doc(),
            FindOptions::builder()
                .projection(doc! { "id": 1, "version": 1 })
                .sort(doc! { "id": 1 })
                .limit(10001)
                .build(),
            filter.authorized_scope.stages().to_vec(),
            executor,
        )
        .await?;
        filter.authorized_scope.ensure_current(executor)?;
        Ok(result)
    }
}
