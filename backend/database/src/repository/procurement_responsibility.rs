//! 采购责任规则仓储查询。

use entities::catalog::{EnableStatus, Product, ProductCategory, ProductRevision, Sku, SkuRevision};
use entities::ids::{ProductCategoryId, ProductId, ProductRevisionId, SkuId, SkuRevisionId};
use entities::procurement_responsibility::{
    ProcurementResponsibilityRule, ProcurementResponsibilityRuleType,
};
use entities::AccountCore;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;

use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// 采购责任规则列表筛选条件。
#[derive(Debug, Clone)]
pub struct ProcurementResponsibilityRuleFilter {
    /// 规则类型；`None` 表示不筛选。
    pub rule_type: Option<ProcurementResponsibilityRuleType>,
    /// 负责人账号 ID；`None` 表示不筛选。
    pub owner_user_id: Option<String>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 页码，从 1 开始。
    pub page: u64,
    /// 每页条数。
    pub page_size: u32,
}

impl QueryFilter for ProcurementResponsibilityRuleFilter {
    /// 构造包含软删除约束的 MongoDB 查询文档。
    ///
    /// # 返回
    /// 返回规则类型、负责人及状态筛选文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(rule_type) = self.rule_type {
            filter.insert("rule_type", rule_type.as_str());
        }
        if let Some(owner_user_id) = self.owner_user_id.as_deref() {
            filter.insert("owner_user_id", owner_user_id);
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for ProcurementResponsibilityRuleFilter {
    /// 返回页码与每页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)`。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, ProcurementResponsibilityRule> {
    /// 分页查询采购责任规则。
    ///
    /// # 参数
    /// * `filter` - 规则列表筛选与分页条件
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按优先级与创建时间稳定排序的当前页规则及总数。
    ///
    /// # 错误
    /// MongoDB 查询、计数或反序列化失败时返回错误。
    pub async fn search_procurement_responsibility_rules(
        &self,
        filter: &ProcurementResponsibilityRuleFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProcurementResponsibilityRule>> {
        let options = FindOptions::builder()
            .sort(doc! { "rule_type": 1, "created_at": 1, "id": 1 })
            .skip(filter.skip())
            .limit(filter.limit())
            .build();
        let items = mongo_ops::find_many(&self.collection(), filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;
        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 读取全部启用采购责任规则。
    ///
    /// # 参数
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回全部未删除且启用规则。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_active_procurement_responsibility_rules(
        &self,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProcurementResponsibilityRule>> {
        self.find_many_sorted(
            doc! { "status": EnableStatus::Active.as_str() },
            doc! { "rule_type": 1, "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }

    /// 按稳定 ID 读取采购责任规则。
    ///
    /// # 参数
    /// * `id` - 采购责任规则 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回未删除规则；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_procurement_responsibility_rule(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProcurementResponsibilityRule>> {
        self.find_by_id(id, executor).await
    }
}

impl<'a> Repository<'a, Sku> {
    /// 批量读取采购责任解析或规则展示引用的 SKU。
    ///
    /// # 参数
    /// * `sku_ids` - SKU 稳定 ID 集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回全部匹配且未删除的 SKU；输入为空时返回空集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_procurement_responsibility_skus(
        &self,
        sku_ids: &[SkuId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Sku>> {
        if sku_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(doc! { "id": { "$in": ids_to_strings(sku_ids) } }, executor)
            .await
    }

    /// 判断采购责任规则引用的 SKU 是否存在。
    ///
    /// # 参数
    /// * `sku_id` - SKU 稳定 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 存在且未删除时返回 `true`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn has_procurement_responsibility_sku(
        &self,
        sku_id: &SkuId,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        Ok(self.find_by_id(sku_id.as_ref(), executor).await?.is_some())
    }
}

impl<'a> Repository<'a, Product> {
    /// 批量读取采购责任目录解析需要的稳定商品。
    ///
    /// # 参数
    /// * `product_ids` - 商品稳定 ID 集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回全部匹配且未删除的商品；输入为空时返回空集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_procurement_responsibility_products(
        &self,
        product_ids: &[ProductId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Product>> {
        if product_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(doc! { "id": { "$in": ids_to_strings(product_ids) } }, executor)
            .await
    }
}

impl<'a> Repository<'a, ProductRevision> {
    /// 批量读取采购责任目录解析需要的商品当前修订。
    ///
    /// # 参数
    /// * `revision_ids` - 商品修订 ID 集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回全部匹配且未删除的商品修订；输入为空时返回空集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_procurement_responsibility_product_revisions(
        &self,
        revision_ids: &[ProductRevisionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProductRevision>> {
        if revision_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(doc! { "id": { "$in": ids_to_strings(revision_ids) } }, executor)
            .await
    }
}

impl<'a> Repository<'a, ProductCategory> {
    /// 批量读取采购责任解析或规则展示引用的商品分类。
    ///
    /// # 参数
    /// * `category_ids` - 商品分类 ID 集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回全部匹配且未删除的商品分类；输入为空时返回空集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_procurement_responsibility_categories(
        &self,
        category_ids: &[ProductCategoryId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProductCategory>> {
        if category_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(doc! { "id": { "$in": ids_to_strings(category_ids) } }, executor)
            .await
    }

    /// 判断采购责任规则引用的商品分类是否存在。
    ///
    /// # 参数
    /// * `category_id` - 商品分类稳定 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 存在且未删除时返回 `true`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn has_procurement_responsibility_category(
        &self,
        category_id: &ProductCategoryId,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        Ok(self.find_by_id(category_id.as_ref(), executor).await?.is_some())
    }
}

impl<'a> Repository<'a, SkuRevision> {
    /// 批量读取采购责任规则展示需要的 SKU 当前修订。
    ///
    /// # 参数
    /// * `revision_ids` - SKU 修订 ID 集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回全部匹配且未删除的 SKU 修订；输入为空时返回空集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_procurement_responsibility_sku_revisions(
        &self,
        revision_ids: &[SkuRevisionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SkuRevision>> {
        if revision_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(doc! { "id": { "$in": ids_to_strings(revision_ids) } }, executor)
            .await
    }
}

impl<'a> Repository<'a, AccountCore> {
    /// 批量读取采购责任规则与解析结果引用的负责人账号。
    ///
    /// # 参数
    /// * `owner_ids` - 负责人账号 ID 集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回全部匹配且未删除的统一账号；输入为空时返回空集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_procurement_responsibility_owners(
        &self,
        owner_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<AccountCore>> {
        if owner_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(doc! { "id": { "$in": owner_ids } }, executor)
            .await
    }

    /// 按稳定 ID 读取采购负责人账号事实。
    ///
    /// # 参数
    /// * `owner_id` - 负责人账号 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回未删除账号；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_procurement_responsibility_owner(
        &self,
        owner_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<AccountCore>> {
        self.find_by_id(owner_id, executor).await
    }
}

/// 将强类型目录 ID 转换为 MongoDB 查询使用的稳定字符串集合。
///
/// # 参数
/// * `ids` - 同类强类型 ID 切片
///
/// # 返回
/// 返回保持输入顺序的字符串 ID 集合。
///
/// # 错误
/// 无。
fn ids_to_strings<T: ToString>(ids: &[T]) -> Vec<String> {
    ids.iter().map(ToString::to_string).collect()
}
