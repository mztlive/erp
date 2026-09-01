//! 采购责任规则仓储访问器。

use entities::ids::SkuId;
use entities::procurement_responsibility::{ProcurementCatalogBundle, ProcurementResponsibilityRule};
use mongodb::Database;

use super::super::procurement_responsibility::ProcurementResponsibilityRuleFilter;
use crate::executor::Executor;
use crate::Repository;
use crate::Result;

/// 采购责任规则仓储访问入口。
#[allow(async_fn_in_trait)]
pub trait ProcurementResponsibilityExt {
    /// 采购责任规则集合名。
    const PROCUREMENT_RESPONSIBILITY_RULES: &'static str = "procurement_responsibility_rules";

    /// 规则列表筛选条件类型。
    type ProcurementResponsibilityRuleFilter;

    /// 获取采购责任规则 Repository。
    ///
    /// # 返回
    /// 返回绑定 `procurement_responsibility_rules` 集合的 Repository。
    fn procurement_responsibility_rules(&self) -> Repository<'_, ProcurementResponsibilityRule>;

    /// 批量加载采购责任目录所需的最小持久化事实。
    ///
    /// # 参数
    /// * `sku_ids` - 待解析的 SKU 集合，已去重并保持调用方顺序
    /// * `executor` - 数据访问执行器，由 Service 决定事务边界；事务内重验必须复用调用方 executor
    ///
    /// # 返回
    /// 返回包含 SKU、商品、当前修订及全部父分类的最小事实集合；缺失由 Entity 层校验。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误；不负责缺失校验，软删除已通过 Repository 查询过滤。
    ///
    /// # 约束
    /// 查询次数与输入规模无关：SKU、商品、修订各一次批量读取，分类按深度分层批量读取；不得出现逐 SKU N+1。
    async fn load_procurement_catalog_bundle(
        &self,
        sku_ids: &[SkuId],
        executor: &mut dyn Executor,
    ) -> Result<ProcurementCatalogBundle>;
}

impl ProcurementResponsibilityExt for Database {
    type ProcurementResponsibilityRuleFilter = ProcurementResponsibilityRuleFilter;

    /// 获取采购责任规则 Repository。
    ///
    /// # 返回
    /// 返回绑定 `procurement_responsibility_rules` 集合的 Repository。
    fn procurement_responsibility_rules(&self) -> Repository<'_, ProcurementResponsibilityRule> {
        Repository::new(self, Self::PROCUREMENT_RESPONSIBILITY_RULES)
    }

    /// 批量加载采购责任目录所需的最小持久化事实。
    ///
    /// # 参数
    /// * `sku_ids` - 待解析的 SKU 集合，已去重并保持调用方顺序
    /// * `executor` - 数据访问执行器，由 Service 决定事务边界；事务内重验必须复用调用方 executor
    ///
    /// # 返回
    /// 返回包含 SKU、商品、当前修订及全部父分类的最小事实集合；缺失由 Entity 层校验。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误；不负责缺失校验，软删除已通过 Repository 查询过滤。
    ///
    /// # 约束
    /// 查询次数与输入规模无关：SKU、商品、修订各一次批量读取，分类按深度分层批量读取；不得出现逐 SKU N+1。
    async fn load_procurement_catalog_bundle(
        &self,
        sku_ids: &[SkuId],
        executor: &mut dyn Executor,
    ) -> Result<ProcurementCatalogBundle> {
        super::super::procurement_responsibility::load_procurement_catalog_bundle(self, sku_ids, executor)
            .await
    }
}
