//! 精确候选查询与按名称串行化的事务写入点；不按账号范围隐藏重复身份。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, mongo_ops};

use super::CatalogRepository;
use crate::entity::catalog::auto_match::match_text;
use crate::entity::catalog::{Product, ProductRevision, Sku};
use crate::repository::CatalogExt;
use crate::{Error, Result};

impl CatalogRepository<'_> {
    /// 在商品写入的同一事务中争用名称，防止并发查询无结果后重复建档。
    /// # 参数
    /// `name` 为完整商品名；`executor` 必须持有事务。
    /// # 返回
    /// 更新独立的并发控制文档，不授予或改变 SKU 归属。
    /// # 错误
    /// 缺少事务或 MongoDB 并发写冲突时失败。
    pub async fn lock_match_name(&self, name: &str, executor: &mut dyn Executor) -> Result<()> {
        let session = executor.session().ok_or_else(|| Error::Internal("商品匹配写入必须使用事务".into()))?;
        self.db
            .collection::<Document>("catalog_match_guards")
            .update_one(doc! {"_id": match_text(name)}, doc! {"$inc": {"version": 1_i64}})
            .upsert(true)
            .session(session)
            .await
            .map_err(persistence_core::Error::from)?;
        Ok(())
    }

    /// 锁定已匹配商品和 SKU，不为新增供给改变业务版本。
    /// # 参数
    /// `product`、`sku` 为同事务读取并校验的身份。
    /// # 返回
    /// 两个文档的并发计数已递增；同批其它规格仍可使用原业务版本。
    /// # 错误
    /// 无事务、版本变化或数据库错误时拒绝。
    pub async fn lock_match_target(
        &self,
        product: &Product,
        sku: &Sku,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.lock_match_document(
            <mongodb::Database as CatalogExt>::PRODUCTS,
            &product.base.id,
            product.base.version,
            executor,
        )
        .await?;
        self.lock_match_document(
            <mongodb::Database as CatalogExt>::SKUS,
            &sku.base.id,
            sku.base.version,
            executor,
        )
        .await
    }

    async fn lock_match_document(
        &self,
        collection: &str,
        id: &str,
        version: u64,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let version = i64::try_from(version).map_err(|_| Error::Internal("商品版本超出存储范围".into()))?;
        let session = executor.session().ok_or_else(|| Error::Internal("商品匹配锁定必须使用事务".into()))?;
        let result = self
            .db
            .collection::<Document>(collection)
            .update_one(
                doc! {"id": id, "version": version, "deleted_at": NOT_DELETED_TIMESTAMP_BSON},
                doc! {"$inc": {"match_guard": 1_i64}},
            )
            .session(session)
            .await
            .map_err(persistence_core::Error::from)?;
        if result.matched_count != 1 {
            return Err(Error::ConflictError("匹配商品已变化，请重新匹配".into()));
        }
        Ok(())
    }

    /// SKU 修订或追加前争用所属商品的当前名称。
    /// # 参数
    /// `product_id` 为已有商品身份，执行器沿用共同事务。
    /// # 返回
    /// 与自动匹配共享写入点。
    /// # 错误
    /// 商品或当前资料缺失、事务错误时拒绝。
    pub async fn lock_product_match(&self, product_id: &str, executor: &mut dyn Executor) -> Result<()> {
        let product = self
            .db
            .products()
            .find_by_id(product_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("公司商品不存在".into()))?;
        let revision = self
            .current_product_revision(&product, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("公司商品缺少当前资料".into()))?;
        self.lock_match_name(&revision.name, executor).await
    }

    /// 按同名索引查询当前商品候选，保留停用商品用于冲突判断。
    /// # 参数
    /// `name` 为完整来源名称，执行器沿用调用事务。
    /// # 返回
    /// 至多 200 个当前商品；不返回历史名称命中的商品。
    /// # 错误
    /// 历史资料未回填、候选超限、缺失当前修订或数据库错误时拒绝。
    pub async fn match_products(&self, name: &str, executor: &mut dyn Executor) -> Result<Vec<Product>> {
        if self.db.product_revisions().exists(doc! {"match_name": null}, executor).await? {
            return Err(Error::ConflictError("商品匹配索引尚未初始化，请先执行商品名称回填后再入库".into()));
        }
        let revisions: Vec<ProductRevision> = mongo_ops::find_many(
            &self.db.product_revisions().collection(),
            doc! {"match_name": match_text(name), "deleted_at": NOT_DELETED_TIMESTAMP_BSON},
            FindOptions::builder().limit(1001).build(),
            executor,
        )
        .await?;
        if revisions.len() > 1000 {
            return Err(Error::ConflictError("同名商品资料过多，请先整理公司商品".into()));
        }
        let ids: Vec<_> = revisions.iter().map(|r| r.product_id.to_string()).collect();
        let products: Vec<Product> = mongo_ops::find_many(
            &self.db.products().collection(),
            doc! {"id": {"$in": ids}, "deleted_at": NOT_DELETED_TIMESTAMP_BSON},
            FindOptions::builder().limit(201).build(),
            executor,
        )
        .await?;
        if products.len() > 200 {
            return Err(Error::ConflictError("同名公司商品超过 200 项，请先整理重复商品".into()));
        }
        let current = self.current_product_revisions(&products, executor).await?;
        let key = match_text(name);
        let mut matches = Vec::new();
        for product in products {
            let revision = current
                .get(&product.base.id)
                .ok_or_else(|| Error::ConflictError("公司商品缺少当前资料".into()))?;
            if match_text(&revision.name) == key {
                matches.push(product);
            }
        }
        Ok(matches)
    }

    /// 有界读取商品的全部未删除规格，包含停用 SKU。
    /// # 参数
    /// `product_id` 为已经确定的公司商品身份，执行器沿用调用事务。
    /// # 返回
    /// 至多 500 个 SKU。
    /// # 错误
    /// 超限或数据库错误时拒绝，不截断候选。
    pub async fn match_skus(&self, product_id: &str, executor: &mut dyn Executor) -> Result<Vec<Sku>> {
        let skus = mongo_ops::find_many(
            &self.db.skus().collection(),
            doc! {"product_id": product_id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON},
            FindOptions::builder().limit(501).build(),
            executor,
        )
        .await?;
        if skus.len() > 500 {
            return Err(Error::ConflictError("公司商品规格过多，请先整理后重新入库".into()));
        }
        Ok(skus)
    }

    /// 读取条码的历史占用与修订归属，包含停用、过期身份，避免绕过占用新建。
    /// # 参数
    /// `barcode` 为原始文本条码；执行器沿用事务。
    /// # 返回
    /// 去重的 SKU 身份，最多两个即可判定歧义。
    /// # 错误
    /// 数据库错误或占用记录损坏时拒绝。
    pub async fn match_barcode(&self, barcode: &str, executor: &mut dyn Executor) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        if let Some(claim) = self.db.sku_barcode_claims().find_by_id(barcode, executor).await? {
            ids.push(claim.sku_id);
        }
        let revisions = mongo_ops::find_many(
            &self.db.sku_revisions().collection(),
            doc! {"barcode": barcode},
            FindOptions::builder().limit(1001).build(),
            executor,
        )
        .await?;
        if revisions.len() > 1000 {
            return Err(Error::ConflictError("条码历史资料过多，请先核对归属".into()));
        }
        ids.extend(revisions.into_iter().map(|r| r.sku_id.to_string()));
        ids.sort();
        ids.dedup();
        Ok(ids)
    }
}
