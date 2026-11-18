//! 差异确认命令的实际执行顺序，Mongo与替身共享同一编排。
use async_trait::async_trait;

use super::*;
use crate::{Error, Result};

pub(crate) struct ResolutionFacts {
    pub product_version: u64,
    pub source: ProductSource,
    pub current: CompanySourceContent,
}

#[async_trait]
pub(crate) trait ResolutionStore: Send {
    async fn authorize(&mut self) -> Result<()>;
    async fn receipt(&mut self, key: &str) -> Result<Option<SourceResolutionReceipt>>;
    async fn facts(&mut self) -> Result<ResolutionFacts>;
    async fn save(
        &mut self,
        source: ProductSource,
        changes: Vec<(SourceField, Option<String>, SourceValue)>,
        req: ResolveSourceDiffRequest,
    ) -> Result<ProductSourceDiffView>;
}

// 确认来源差异时先授权，再按操作号检查回执；同商品同请求重放原结果，不按后来版本重新执行。
// 首次命令才比较商品、来源和涉及的 SKU 版本，再应用字段决定并交原存储事务一次保存。异载荷、旧版本或校验失败不继续写入。
pub(crate) async fn resolve<S: ResolutionStore>(
    store: &mut S,
    product_id: &str,
    req: ResolveSourceDiffRequest,
) -> Result<ProductSourceDiffView> {
    store.authorize().await?;

    if let Some(receipt) = store.receipt(&req.idempotency_key).await? {
        if receipt.product_id != product_id || receipt.request != req || receipt.base.deleted_at != 0 {
            return Err(Error::ConflictError("差异确认操作号已被不同请求使用".into()));
        }
        return Ok(receipt.result);
    }

    let ResolutionFacts { product_version, mut source, current } = store.facts().await?;
    if product_version != req.expected_product_version || source.base.version != req.expected_source_version {
        return Err(Error::ConflictError("商品或来源资料已变化，请重新比较".into()));
    }
    for decision in &req.decisions {
        if let Some(sku) = &decision.sku_id
            && (current.sku_versions.get(sku) != req.expected_sku_versions.get(sku)
                || !current.sku_versions.contains_key(sku))
        {
            return Err(Error::ConflictError("商品规格已变化，请重新比较".into()));
        }
    }

    source.record_company_edit(&current.values);
    let changes = source.resolve(&req.decisions)?;
    store.save(source, changes, req).await
}
