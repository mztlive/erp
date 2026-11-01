use std::collections::BTreeMap;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_support::FileAssetExt;
use persistence_core::NoTransaction;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{BusinessTransfers, TransferKind};
use crate::{Error, Result};

impl BusinessTransfers {
    pub(super) async fn pool_source(&self, input: &Value, actor: &AuditActor) -> Result<Vec<Value>> {
        let mut rows = self.collect(TransferKind::ProductPoolExcel, &input["query"], actor).await?;
        if let Some(ids) = input["selected_ids"].as_array() {
            rows.retain(|row| ids.contains(&row["sku_id"]));
            if rows.len() != ids.len() {
                return Err(Error::ConflictError("部分勾选商品已失效，请重新选择".into()));
            }
        }
        if rows.is_empty() || rows.len() > 2000 {
            return Err(Error::ValidationError("请选择 1 至 2000 个商品".into()));
        }
        Ok(rows)
    }
    pub(super) async fn workbook_authorization(
        &self,
        kind: TransferKind,
        input: &Value,
        actor: &AuditActor,
    ) -> Result<Value> {
        if kind == TransferKind::ProductPoolExcel {
            let rows = self.pool_source(input, actor).await?;
            return self.workbook_evidence("rows", &json!(rows), &rows).await;
        }
        let source = self.quote_source(input, actor).await?;
        let products = source["products"].as_array().ok_or_else(|| Error::Internal("商品资料缺失".into()))?;
        self.workbook_evidence("source", &source, products).await
    }
    pub(super) async fn workbook_evidence(&self, key: &str, source: &Value, rows: &[Value]) -> Result<Value> {
        let mut images = BTreeMap::new();
        for id in rows.iter().filter_map(|r| r["main_image_asset_id"].as_str()) {
            if images.contains_key(id) {
                continue;
            }
            let asset = self
                .db
                .file_assets()
                .find_by_id(id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("商品图片不存在，请核对商品资料".into()))?;
            if asset.destroyed_at.is_some() || asset.expires_at.is_some_and(|at| at <= Instant::now()) {
                return Err(Error::ConflictError("商品图片已失效，请重新导出".into()));
            }
            images.insert(id.to_owned(), json!({"version":asset.base.version,"hash":asset.content_hmac}));
        }
        let signature = json!({key:source,"images":images});
        let hash = hex::encode(Sha256::digest(
            serde_json::to_vec(&signature).map_err(|_| Error::Internal("导出摘要生成失败".into()))?,
        ));
        Ok(json!({"content_hash":hash}))
    }
}
