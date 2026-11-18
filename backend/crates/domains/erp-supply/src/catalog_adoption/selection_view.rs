//! 商品库选择只读取身份和展示名称，不下载全批草稿和定价。

use std::collections::HashSet;

use mongodb::Database;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, Result, mongo_ops};
use serde::{Deserialize, Serialize};

use super::CatalogAdoptionBatch;

/// 选择条与排除清单所需的来源身份。
#[derive(Debug, Deserialize, Serialize)]
pub struct SelectionSource {
    pub id: String,
    pub product_id: String,
    pub name: String,
    pub specification: Option<String>,
    pub spec_id: String,
}

/// 选择成员不携带报价、匹配目标、执行载荷或人工编辑。
#[derive(Debug, Deserialize, Serialize)]
pub struct SelectionRow {
    pub source: SelectionSource,
}

/// 商品库选择集合摘要，完整草稿仅在进入配置后读取。
#[derive(Debug, Serialize)]
pub struct SelectionView {
    pub batch: CatalogAdoptionBatch,
    pub rows: Vec<SelectionRow>,
    pub product_count: usize,
}

/// 从已授权且准备完成的批次读取轻量选择集合。
/// # 参数
/// `batch` 为调用方已验证权限及完成状态的批次，执行器沿用读取事务。
/// # 返回
/// 固定成员身份及排除项展示资料，不返回完整草稿。
/// # 错误
/// 数据库读取失败。
pub async fn selection_view(
    db: &Database,
    batch: CatalogAdoptionBatch,
    executor: &mut dyn Executor,
) -> Result<SelectionView> {
    let rows = mongo_ops::find_many(&db.collection::<SelectionRow>("supplier_catalog_adoption_batch_items"),
        doc! {"batch_id":&batch.base.id,"deleted_at":0_i64},
        FindOptions::builder().projection(doc! {"_id":0,"source.id":1,"source.product_id":1,"source.name":1,"source.specification":1,"source.spec_id":1}).sort(doc! {"id":1}).build(), executor).await?;

    let excluded: HashSet<_> = batch.excluded_ids.iter().collect();
    let product_count = rows
        .iter()
        .filter(|row| !excluded.contains(&row.source.id))
        .map(|row| &row.source.product_id)
        .collect::<HashSet<_>>()
        .len();
    Ok(SelectionView { batch, rows, product_count })
}
