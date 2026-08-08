//! 供应商 SKU ↔ 公司商品池匹配：已映射状态与候选公司 SKU。
//!
//! 匹配只提供证据与排序，最终映射由采购确认；条码一致优先，名称/规格/单位为辅助信号。

use std::collections::{HashMap, HashSet};

use database::{CatalogExt, NoTransaction, SupplierCatalogExt};
use entities::catalog::EnableStatus;
use entities::ids::{SkuId, SupplierCatalogProductId, SupplierCatalogSkuId};
use entities::supplier_catalog::SupplierCatalogSkuRevision;
use mongodb::bson::{doc, Bson, Regex};

use super::dto::{
    CompanySkuMatchCandidateView, PoolMatchStatus, SupplierProductPoolMatchView, SupplierSkuPoolMatchView,
};
use super::SupplierCatalogService;
use crate::errors::{Error, Result};

/// 候选数量上限（每供应商 SKU）。
const MAX_CANDIDATES: usize = 8;
/// 名称/规格辅助检索结果上限。
const TEXT_SEARCH_LIMIT: usize = 40;

impl SupplierCatalogService {
    /// 查询供应商商品下各 SKU 的池内状态与公司 SKU 匹配候选。
    ///
    /// # 参数
    /// * `supplier_product_id` - 供应商 SPU 稳定身份
    ///
    /// # 返回
    /// 返回 SPU 级汇总与每行供应商 SKU 的 `MAPPED` / `HAS_CANDIDATES` / `UNMATCHED`。
    ///
    /// # 错误
    /// * `NotFound` - 供应商商品不存在
    pub async fn product_pool_match(
        &self,
        supplier_product_id: &str,
    ) -> Result<SupplierProductPoolMatchView> {
        let product_id = SupplierCatalogProductId::new(supplier_product_id.to_string());
        let product = self
            .db
            .supplier_catalog_products()
            .find_by_id(&product_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商商品不存在".to_string()))?;

        let source_revision_no = self.current_product_revision_no(&product_id).await?.unwrap_or(0);

        let skus = self
            .db
            .supplier_catalog_skus()
            .find_many(
                doc! {
                    "supplier_catalog_product_id": product.base.id.clone(),
                    "deleted_at": Bson::Int64(0),
                },
                &mut NoTransaction,
            )
            .await?;
        let sku_ids: Vec<SupplierCatalogSkuId> = skus.iter().map(|sku| sku.base.id.clone().into()).collect();
        let revisions = self.current_sku_revisions(&sku_ids).await?;

        let mut items = Vec::with_capacity(skus.len());
        for sku in skus {
            let rev = revisions.get(&sku.base.id).and_then(|v| v.as_ref());
            let row = self
                .match_one_supplier_sku(sku.base.id.clone().into(), &sku.supplier_sku_code, rev)
                .await?;
            items.push(row);
        }

        Ok(SupplierProductPoolMatchView {
            supplier_product_id: product.base.id,
            source_revision_no,
            items,
        })
    }

    /// 为单个供应商 SKU 计算池内状态与候选列表。
    ///
    /// # 参数
    /// * `supplier_catalog_sku_id` - 供应商 SKU
    /// * `supplier_sku_code` - 供应商 SKU 编码
    /// * `revision` - 当前来源修订（可空）
    ///
    /// # 返回
    /// 返回单行匹配视图。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn match_one_supplier_sku(
        &self,
        supplier_catalog_sku_id: SupplierCatalogSkuId,
        supplier_sku_code: &str,
        revision: Option<&SupplierCatalogSkuRevision>,
    ) -> Result<SupplierSkuPoolMatchView> {
        let specification = revision.map(|r| r.specification.clone());
        let barcode = revision.and_then(|r| r.barcode.clone());

        if let Some(mapping) = self
            .db
            .supplier_product_mappings()
            .find_active_by_supplier_sku(&supplier_catalog_sku_id, &mut NoTransaction)
            .await?
        {
            let company_sku = self
                .db
                .skus()
                .find_by_id(mapping.sku_id.as_ref(), &mut NoTransaction)
                .await?;
            return Ok(SupplierSkuPoolMatchView {
                supplier_catalog_sku_id: supplier_catalog_sku_id.to_string(),
                supplier_sku_code: supplier_sku_code.to_string(),
                specification,
                barcode,
                pool_status: PoolMatchStatus::Mapped,
                mapped_company_sku_id: Some(mapping.sku_id.to_string()),
                mapped_company_sku_no: company_sku.map(|sku| sku.sku_no),
                candidates: Vec::new(),
            });
        }

        let mut candidates = self.collect_candidates(revision).await?;
        candidates.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.sku_no.cmp(&b.sku_no)));
        candidates.truncate(MAX_CANDIDATES);

        let pool_status = if candidates.is_empty() {
            PoolMatchStatus::Unmatched
        } else {
            PoolMatchStatus::HasCandidates
        };

        Ok(SupplierSkuPoolMatchView {
            supplier_catalog_sku_id: supplier_catalog_sku_id.to_string(),
            supplier_sku_code: supplier_sku_code.to_string(),
            specification,
            barcode,
            pool_status,
            mapped_company_sku_id: None,
            mapped_company_sku_no: None,
            candidates,
        })
    }

    /// 按条码、名称、规格收集公司 SKU 候选并打分。
    ///
    /// # 参数
    /// * `revision` - 供应商 SKU 来源修订
    ///
    /// # 返回
    /// 返回去重后的候选列表（未截断）。
    ///
    /// # 错误
    /// 查询失败时返回错误。
    async fn collect_candidates(
        &self,
        revision: Option<&SupplierCatalogSkuRevision>,
    ) -> Result<Vec<CompanySkuMatchCandidateView>> {
        let Some(revision) = revision else {
            return Ok(Vec::new());
        };

        let mut by_sku: HashMap<String, CompanySkuMatchCandidateView> = HashMap::new();

        if let Some(barcode) = revision
            .barcode
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let revs = self
                .db
                .sku_revisions()
                .find_active_by_barcode(barcode, &mut NoTransaction)
                .await?;
            for rev in revs {
                self.upsert_candidate(
                    &mut by_sku,
                    &rev.sku_id,
                    &rev.name,
                    rev.specification.as_deref(),
                    rev.barcode.as_deref(),
                    rev.sales_visible_price_gross.map(|v| v.to_string()),
                    vec!["条码一致".to_string()],
                    100,
                )
                .await?;
            }
        }

        let name = revision.name.trim();
        if name.chars().count() >= 2 {
            let rows = self.search_sku_revisions_by_text(name).await?;
            for rev in rows {
                let mut signals = Vec::new();
                let mut score = 20u32;
                if names_related(&rev.name, name) {
                    signals.push("名称相近".to_string());
                    score += 25;
                }
                if let Some(spec) = rev.specification.as_deref() {
                    if specs_related(spec, &revision.specification) {
                        signals.push("规格相近".to_string());
                        score += 20;
                    }
                }
                if signals.is_empty() {
                    continue;
                }
                self.upsert_candidate(
                    &mut by_sku,
                    &rev.sku_id,
                    &rev.name,
                    rev.specification.as_deref(),
                    rev.barcode.as_deref(),
                    rev.sales_visible_price_gross.map(|v| v.to_string()),
                    signals,
                    score,
                )
                .await?;
            }
        }

        let spec = revision.specification.trim();
        if spec.chars().count() >= 2 && spec != name && spec.chars().count() <= 40 {
            let rows = self.search_sku_revisions_by_text(spec).await?;
            for rev in rows {
                let mut signals = vec!["规格线索".to_string()];
                let mut score = 30u32;
                if names_related(&rev.name, name) {
                    signals.push("名称相近".to_string());
                    score += 15;
                }
                self.upsert_candidate(
                    &mut by_sku,
                    &rev.sku_id,
                    &rev.name,
                    rev.specification.as_deref(),
                    rev.barcode.as_deref(),
                    rev.sales_visible_price_gross.map(|v| v.to_string()),
                    signals,
                    score,
                )
                .await?;
            }
        }

        if let Some(unit) = revision
            .source_base_unit
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            let keys: Vec<String> = by_sku.keys().cloned().collect();
            for key in keys {
                let Some(sku) = self.db.skus().find_by_id(&key, &mut NoTransaction).await? else {
                    continue;
                };
                if let Some(uom) = self
                    .db
                    .unit_of_measures()
                    .find_by_id(sku.base_unit_id.as_ref(), &mut NoTransaction)
                    .await?
                {
                    let unit_hit = uom.name == unit || uom.unit_code == unit || uom.symbol == unit;
                    if unit_hit {
                        if let Some(entry) = by_sku.get_mut(&key) {
                            if !entry.match_signals.iter().any(|s| s == "单位一致") {
                                entry.match_signals.push("单位一致".to_string());
                                entry.score += 10;
                            }
                        }
                    }
                }
            }
        }

        Ok(by_sku.into_values().collect())
    }

    /// 按文本对启用中的 SKU 修订做不区分大小写的包含匹配。
    ///
    /// # 参数
    /// * `text` - 检索关键字（已 trim）
    ///
    /// # 返回
    /// 返回命中的修订实体（最多 [`TEXT_SEARCH_LIMIT`] 条）。
    ///
    /// # 错误
    /// 查询失败时返回错误。
    async fn search_sku_revisions_by_text(&self, text: &str) -> Result<Vec<entities::catalog::SkuRevision>> {
        let pattern = regex_escape(text);
        let filter = doc! {
            "deleted_at": Bson::Int64(0),
            "status": EnableStatus::Active.as_str(),
            "$or": [
                { "name": Regex { pattern: pattern.clone(), options: "i".to_string() } },
                { "specification": Regex { pattern, options: "i".to_string() } },
            ],
        };
        let mut rows = self
            .db
            .sku_revisions()
            .find_many(filter, &mut NoTransaction)
            .await?;
        // 每个 SKU 只保留最大 revision_no
        rows.sort_by(|a, b| {
            a.sku_id
                .to_string()
                .cmp(&b.sku_id.to_string())
                .then(b.revision.revision_no.cmp(&a.revision.revision_no))
        });
        let mut seen = HashSet::new();
        let mut latest = Vec::new();
        for row in rows {
            let key = row.sku_id.to_string();
            if seen.insert(key) {
                latest.push(row);
            }
            if latest.len() >= TEXT_SEARCH_LIMIT {
                break;
            }
        }
        Ok(latest)
    }

    /// 将候选合并进 map：同一公司 SKU 保留更高分并合并信号。
    ///
    /// # 参数
    /// * `by_sku` - 候选累加器
    /// * `sku_id` - 公司 SKU
    /// * `name` / `specification` / `barcode` / `sales_price` - 展示字段
    /// * `signals` - 本次匹配证据
    /// * `score` - 本次得分
    ///
    /// # 返回
    /// 成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 加载 SKU/商品失败时返回错误。
    #[allow(clippy::too_many_arguments)]
    async fn upsert_candidate(
        &self,
        by_sku: &mut HashMap<String, CompanySkuMatchCandidateView>,
        sku_id: &SkuId,
        name: &str,
        specification: Option<&str>,
        barcode: Option<&str>,
        sales_price: Option<String>,
        signals: Vec<String>,
        score: u32,
    ) -> Result<()> {
        let key = sku_id.to_string();
        if let Some(existing) = by_sku.get_mut(&key) {
            let mut signal_set: HashSet<String> = existing.match_signals.iter().cloned().collect();
            for signal in signals {
                signal_set.insert(signal);
            }
            existing.match_signals = signal_set.into_iter().collect();
            existing.match_signals.sort();
            existing.score = existing.score.max(score);
            if existing.sales_visible_price_gross.is_none() {
                existing.sales_visible_price_gross = sales_price;
            }
            if existing.specification.is_none() {
                existing.specification = specification.map(str::to_string);
            }
            if existing.barcode.is_none() {
                existing.barcode = barcode.map(str::to_string);
            }
            return Ok(());
        }

        let sku = match self
            .db
            .skus()
            .find_by_id(sku_id.as_ref(), &mut NoTransaction)
            .await?
        {
            Some(sku) if sku.stable.status == EnableStatus::Active => sku,
            _ => return Ok(()),
        };
        let product = match self
            .db
            .products()
            .find_by_id(sku.product_id.as_ref(), &mut NoTransaction)
            .await?
        {
            Some(product) if product.stable.status == EnableStatus::Active => product,
            _ => return Ok(()),
        };

        let active_supplier_count = self.count_active_offerings_for_sku(sku_id).await?;

        by_sku.insert(
            key,
            CompanySkuMatchCandidateView {
                sku_id: sku.base.id.clone(),
                sku_no: sku.sku_no,
                product_id: product.base.id.clone(),
                product_no: product.product_no,
                name: name.to_string(),
                specification: specification.map(str::to_string),
                barcode: barcode.map(str::to_string),
                base_unit_id: sku.base_unit_id.to_string(),
                sales_visible_price_gross: sales_price,
                active_supplier_count,
                match_signals: {
                    let mut s = signals;
                    s.sort();
                    s.dedup();
                    s
                },
                score,
            },
        );
        Ok(())
    }

    /// 统计公司 SKU 上状态为启用的供给数量。
    ///
    /// # 参数
    /// * `sku_id` - 公司 SKU
    ///
    /// # 返回
    /// 返回有效供给条数。
    ///
    /// # 错误
    /// 查询失败时返回错误。
    async fn count_active_offerings_for_sku(&self, sku_id: &SkuId) -> Result<u32> {
        let rows = self
            .db
            .supplier_offerings()
            .find_many(
                doc! {
                    "sku_id": sku_id.to_string(),
                    "status": entities::supplier_catalog::OfferingStatus::Active.as_str(),
                    "deleted_at": Bson::Int64(0),
                },
                &mut NoTransaction,
            )
            .await?;
        Ok(rows.len() as u32)
    }

    /// 取供应商 SPU 当前来源修订号。
    ///
    /// # 参数
    /// * `product_id` - 供应商 SPU
    ///
    /// # 返回
    /// 最大修订号；无修订时 `None`。
    ///
    /// # 错误
    /// 查询失败时返回错误。
    pub(super) async fn current_product_revision_no(
        &self,
        product_id: &SupplierCatalogProductId,
    ) -> Result<Option<u32>> {
        let revisions = self
            .db
            .supplier_catalog_product_revisions()
            .find_many(
                doc! { "supplier_catalog_product_id": product_id.to_string() },
                &mut NoTransaction,
            )
            .await?;
        Ok(revisions
            .into_iter()
            .map(|revision| revision.revision.revision_no)
            .max())
    }
}

/// 名称是否构成弱相关（包含关系）。
///
/// # 参数
/// * `left` / `right` - 比较文本
///
/// # 返回
/// 相关返回 `true`。
fn names_related(left: &str, right: &str) -> bool {
    let a = left.trim();
    let b = right.trim();
    if a.is_empty() || b.is_empty() {
        return false;
    }
    a.contains(b) || b.contains(a)
}

/// 规格是否构成弱相关。
///
/// # 参数
/// * `left` / `right` - 规格文本
///
/// # 返回
/// 相关返回 `true`。
fn specs_related(left: &str, right: &str) -> bool {
    names_related(left, right)
}

/// 转义正则元字符，避免用户输入破坏查询。
///
/// # 参数
/// * `value` - 原文本
///
/// # 返回
/// 返回转义后的模式串。
fn regex_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 2);
    for ch in value.chars() {
        match ch {
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '^' | '$' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}
