//! 采购责任筛选仅装载供给身份和 SKU 引用。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, QueryFilter, Result, mongo_ops};
use serde::Deserialize;

use super::{SupplierOfferingFilter, SupplierOfferingRepository, in_filter};

/// 供给采购规则解析所需的最小身份引用。
#[derive(Debug, Deserialize)]
pub struct ProcurementOfferingFact {
    /// 供给身份。
    pub id: String,
    /// 供给引用的 SKU 身份。
    pub sku_id: String,
}

/// 供给稳定身份仓储的采购候选与窄事实查询。
#[allow(async_fn_in_trait)]
pub trait ProcurementOfferingRepositoryExt {
    /// 按完整列表条件读取责任解析候选身份，忽略页面边界。
    ///
    /// # 参数
    /// * `filter` - 包含授权与已解析关键词、编号、状态的完整筛选
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回至多 10001 个候选身份；调用方必须整体拒绝超限。
    ///
    /// # 错误
    /// MongoDB 查询或身份反序列化失败时返回错误。
    async fn procurement_candidate_ids(
        &self,
        filter: &SupplierOfferingFilter,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;

    /// 批量读取候选供给的 SKU 引用。
    ///
    /// # 参数
    /// * `ids` - 已授权且满足基础查询的候选供给
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回未删除供给的身份引用；空输入不查询。
    ///
    /// # 错误
    /// MongoDB 查询或事实反序列化失败时返回错误。
    async fn procurement_facts(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProcurementOfferingFact>>;
}

impl ProcurementOfferingRepositoryExt for SupplierOfferingRepository<'_> {
    async fn procurement_candidate_ids(
        &self,
        filter: &SupplierOfferingFilter,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        #[derive(Deserialize)]
        struct CandidateId {
            id: String,
        }
        let rows = mongo_ops::find_many(
            &self.collection().clone_with_type::<CandidateId>(),
            filter.to_doc(),
            FindOptions::builder().projection(doc! { "id": 1 }).sort(doc! { "id": 1 }).limit(10001).build(),
            executor,
        )
        .await?;
        Ok(rows.into_iter().map(|row| row.id).collect())
    }

    async fn procurement_facts(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProcurementOfferingFact>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut filter = in_filter("id", ids.iter().cloned());
        filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
        mongo_ops::find_many(
            &self.collection().clone_with_type::<ProcurementOfferingFact>(),
            filter,
            FindOptions::builder().projection(doc! { "id": 1, "sku_id": 1 }).build(),
            executor,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::deserialize_from_document;

    use super::*;

    #[test]
    /// 供给采购责任事实不需要商业条款、订货编码和审计内容。
    fn offering_procurement_fact_keeps_only_identity_and_sku() {
        let fact: ProcurementOfferingFact =
            deserialize_from_document(doc! { "id": "offering", "sku_id": "sku" }).unwrap();
        assert_eq!((fact.id.as_str(), fact.sku_id.as_str()), ("offering", "sku"));
        assert!(deserialize_from_document::<ProcurementOfferingFact>(doc! { "id": "offering" }).is_err());
        assert!(deserialize_from_document::<ProcurementOfferingFact>(doc! { "sku_id": "sku" }).is_err());
    }
}
