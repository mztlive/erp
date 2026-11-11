//! 合同授权条件；由应用层完成 DataScope 解析后再交给仓储。

use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, QueryFilter};
use serde::Deserialize;

use super::contract::{ContractDomainRepository, ContractFilter};
use super::extensions::ContractExt;
use crate::entity::contract::Contract;

/// identity完整查询阶段。
pub type ContractReadScope = crate::authorization::CompiledScope;
/// 跨页校验使用的合同身份和版本，不包含展示字段。
#[derive(Debug, Deserialize, Hash)]
pub struct ContractVersion {
    /// 合同稳定主键。
    pub id: String,
    /// 合同乐观锁版本。
    pub version: u64,
}

#[allow(async_fn_in_trait)]
pub trait ContractRepositoryScopeExt {
    /// 按明确授权条件读取单个合同。
    ///
    /// # 参数
    /// * `id` - 合同稳定主键
    /// * `scope` - 已证明的授权条件
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 不存在或不在范围内均返回 `None`。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    ///
    /// # 关键业务约束
    /// ID 条件不能替换范围交集；空授权不得返回文档。
    async fn find_authorized(
        &self,
        id: &str,
        scope: &ContractReadScope,
        executor: &mut dyn Executor,
    ) -> crate::Result<Option<Contract>>;
}

impl ContractRepositoryScopeExt for persistence_core::Repository<'_, Contract> {
    async fn find_authorized(
        &self,
        id: &str,
        scope: &ContractReadScope,
        executor: &mut dyn Executor,
    ) -> crate::Result<Option<Contract>> {
        scope.validate(executor).await?;
        let result: persistence_core::Result<_> = async {
            let mut stages = scope.stages();
            stages.push(doc! {"$match":{"id":id}});
            stages.push(doc! {"$limit":1});
            Ok(persistence_core::aggregate::rows(&self.collection(), stages, executor)
                .await?
                .into_iter()
                .next())
        }
        .await;
        let result = result?;
        scope.validate(executor).await?;
        Ok(result)
    }
}

impl ContractDomainRepository<'_> {
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
    ///
    /// # 关键业务约束
    /// 不得截断版本集合后继续拼接跨页结果。
    pub async fn query_versions(
        &self,
        filter: &ContractFilter,
        executor: &mut dyn Executor,
    ) -> crate::Result<Vec<ContractVersion>> {
        if let Some(scope) = &filter.access {
            scope.validate(executor).await?;
        }
        let result: persistence_core::Result<_> = async {
            persistence_core::aggregate::find(
                &self.db.collection::<ContractVersion>(<mongodb::Database as ContractExt>::CONTRACTS),
                filter.to_doc(),
                FindOptions::builder()
                    .projection(doc! { "id": 1, "version": 1 })
                    .sort(doc! { "id": 1 })
                    .limit(10001)
                    .build(),
                filter.stages(),
                executor,
            )
            .await
        }
        .await;
        let result = result?;
        if let Some(scope) = &filter.access {
            scope.validate(executor).await?;
        }
        Ok(result)
    }
}
