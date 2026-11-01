//! 完整冻结提交行的检索资料。所有调用方必须先证明事项读取资格。
use std::collections::HashMap;

use erp_catalog::repository::CatalogExt;
use erp_catalog::repository::prelude::*;
use erp_core::ids::{SalesOrderSubmissionId, SkuId};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_workflow::WorkflowAuthorizationPort;
use erp_workflow::entity::work_item::WorkItemType;
use persistence_core::Executor;

use super::super::WorkbenchReadService;
use super::super::dto::WorkItemFields;
use super::SearchDocument;
use crate::Result;

pub(super) type SubjectKey = (String, String, String);
impl<A: WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 补齐完整销售与采购提交商品行，不改变展示摘要长度。
    /// # 参数
    /// `fields` 为已授权候选；`executor` 为列表快照。
    /// # 返回
    /// 就地追加匹配资料。
    /// # 错误
    /// 版本或明细读取失败时中止查询。
    pub(in crate::workbench) async fn search_order_lines(
        &self,
        fields: &mut [WorkItemFields],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let keys = fields
            .iter()
            .filter(|f| f.work_item_type != WorkItemType::ProcurementOrderCreation)
            .map(|f| {
                (f.business_object_type.clone(), f.business_object_id.clone(), f.subject_version.clone())
            })
            .collect::<Vec<_>>();
        let sources = self.submission_search(&keys, executor).await?;
        for field in fields {
            let key = (
                field.business_object_type.clone(),
                field.business_object_id.clone(),
                field.subject_version.clone(),
            );
            if let Some(source) = sources.get(&key) {
                field.search.merge(source);
            }
            let kind = match field.business_object_type.as_str() {
                "sales_order" | "voucher_sales_order" => "sales_orders",
                "purchase_order" => "purchase_orders",
                _ => continue,
            };
            field.search.facet(kind, &key.1, &field.business_object_label.clone());
        }
        Ok(())
    }

    /// 批量读取精确提交版本的全部商品与 SKU 编号。
    /// # 参数
    /// `keys` 为已授权单据类型、身份及提交号；`executor` 沿用调用方快照。
    /// # 返回
    /// 返回按提交归属的检索文档，缺失版本不回退。
    /// # 错误
    /// 仓储读取失败时返回错误。
    pub(in crate::workbench) async fn submission_search(
        &self,
        keys: &[SubjectKey],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<SubjectKey, SearchDocument>> {
        let mut documents = HashMap::new();
        let normalized = keys.iter().map(canonical_key).collect::<Vec<_>>();
        self.sales_search_sources(&normalized, &mut documents, executor).await?;
        for key in keys {
            if key.0 == "voucher_sales_order"
                && let Some(document) = documents.get(&canonical_key(key)).cloned()
            {
                documents.insert(key.clone(), document);
            }
        }
        self.purchase_search_sources(keys, &mut documents, executor).await?;
        self.related_sales_search(keys, &mut documents, executor).await?;
        Ok(keys
            .iter()
            .filter_map(|key| {
                documents
                    .get(key)
                    .or_else(|| documents.get(&canonical_key(key)))
                    .cloned()
                    .map(|document| (key.clone(), document))
            })
            .collect())
    }

    async fn sales_search_sources(
        &self,
        keys: &[SubjectKey],
        documents: &mut HashMap<SubjectKey, SearchDocument>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = keys.iter().filter(|k| k.0 == "sales_order").map(|k| k.1.clone()).collect::<Vec<_>>();
        if ids.is_empty() {
            return Ok(());
        }
        let submissions = self
            .db
            .sales_order_submissions()
            .list_work_item_brief_submissions_by_orders(&ids, executor)
            .await?;
        let selected = submissions
            .iter()
            .filter(|s| {
                keys.contains(&(
                    "sales_order".into(),
                    s.sales_order_id.to_string(),
                    s.submission_no.to_string(),
                ))
            })
            .collect::<Vec<_>>();
        let ids = selected.iter().map(|s| SalesOrderSubmissionId::new(&s.base.id)).collect::<Vec<_>>();
        if ids.is_empty() {
            return Ok(());
        }
        let lines = self.db.sales_order_submission_lines().list_lines_by_submissions(&ids, executor).await?;
        let sku_ids = lines.iter().filter_map(|l| l.sku_id.clone()).collect::<Vec<_>>();
        let codes = self.search_sku_codes(&sku_ids, executor).await?;
        let by_id = selected.iter().map(|s| (s.base.id.as_str(), *s)).collect::<HashMap<_, _>>();
        for line in lines {
            let Some(s) = by_id.get(line.submission_id.as_ref()) else {
                continue;
            };
            let doc = documents
                .entry(("sales_order".into(), s.sales_order_id.to_string(), s.submission_no.to_string()))
                .or_default();
            doc.text("商品", &line.item_name_snapshot);
            if let Some(spec) = &line.spec_snapshot {
                doc.text("商品规格", spec);
            }
            if let Some(id) = &line.sku_id
                && let Some(code) = codes.get(id.as_ref())
            {
                doc.text("商品编码", code);
            }
        }
        Ok(())
    }

    async fn purchase_search_sources(
        &self,
        keys: &[SubjectKey],
        documents: &mut HashMap<SubjectKey, SearchDocument>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = keys.iter().filter(|k| k.0 == "purchase_order").map(|k| k.1.clone()).collect::<Vec<_>>();
        if ids.is_empty() {
            return Ok(());
        }
        let submissions = self
            .db
            .purchase_order_submissions()
            .list_work_item_brief_submissions_by_orders(&ids, executor)
            .await?;
        let selected = submissions
            .into_iter()
            .filter(|s| {
                keys.contains(&(
                    "purchase_order".into(),
                    s.purchase_order_id.to_string(),
                    s.submission_no.clone(),
                ))
            })
            .collect::<Vec<_>>();
        let lines = self.facts_reader().purchase_submission_lines(&selected, executor).await?;
        let sku_ids = lines.iter().filter_map(|l| l.sku_id.clone()).collect::<Vec<_>>();
        let codes = self.search_sku_codes(&sku_ids, executor).await?;
        let by_id = selected.iter().map(|s| (s.base.id.as_str(), s)).collect::<HashMap<_, _>>();
        for line in lines {
            let Some(s) = by_id.get(line.purchase_order_submission_id.as_ref()) else {
                continue;
            };
            let doc = documents
                .entry(("purchase_order".into(), s.purchase_order_id.to_string(), s.submission_no.clone()))
                .or_default();
            if let Some(name) = &line.product_name_snapshot {
                doc.text("商品", name);
            }
            if let Some(spec) = &line.specification_snapshot {
                doc.text("商品规格", spec);
            }
            if let Some(id) = &line.sku_id
                && let Some(code) = codes.get(id.as_ref())
            {
                doc.text("商品编码", code);
            }
        }
        Ok(())
    }
    async fn search_sku_codes(
        &self,
        ids: &[SkuId],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        Ok(self
            .db
            .skus()
            .find_by_ids(ids, executor)
            .await?
            .into_iter()
            .map(|sku| (sku.base.id, sku.sku_no))
            .collect())
    }
}

fn canonical_key(key: &SubjectKey) -> SubjectKey {
    let kind = if key.0 == "voucher_sales_order" { "sales_order" } else { &key.0 };
    (kind.to_owned(), key.1.clone(), key.2.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn voucher_submission_alias_preserves_exact_order_and_version() {
        let key = ("voucher_sales_order".into(), "order-1".into(), "2".into());
        assert_eq!(canonical_key(&key), ("sales_order".into(), "order-1".into(), "2".into()));
        assert_ne!(canonical_key(&key), canonical_key(&(key.0.clone(), key.1.clone(), "3".into())));
        assert_ne!(canonical_key(&key), canonical_key(&(key.0.clone(), "order-2".into(), key.2.clone())));
    }
}
