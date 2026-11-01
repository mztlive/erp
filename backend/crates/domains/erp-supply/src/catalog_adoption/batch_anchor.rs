//! 同一来源商品内已存在 SKU 的服务器匹配证据；不接受客户端选择目标。

use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};

use super::{AdoptionMapping, CatalogAdoptionPreview};
use crate::entity::supplier_catalog::SupplierCatalogItem;
use crate::{Error, Result};

/// 已由自动身份匹配及商品权限确认的公司目标快照。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchAnchorTarget {
    pub product_id: String,
    pub sku_id: String,
    pub sku_version: u64,
    pub product_version: u64,
    pub product_revision_id: String,
}

/// 未提交的来源规格也可证明同源新规格的公司商品归属，执行时仍必须重验。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchMatchAnchor {
    pub source: SupplierCatalogItem,
    pub mapping: AdoptionMapping,
    pub target: BatchAnchorTarget,
}

impl BatchMatchAnchor {
    /// 核验传播依据仍来自当前来源商品及当前供应商，允许单独建立供给关联。
    /// # 参数
    /// `candidate` 为待新增规格，`current` 为事务内重读的依据来源。
    /// # 返回
    /// 同源商品且来源内容没有变化时成功。
    /// # 错误
    /// 跨连接、供应商、来源商品、规格或来源资料变化时拒绝。
    pub fn ensure_source(
        &self,
        candidate: &SupplierCatalogItem,
        current: &SupplierCatalogItem,
    ) -> Result<()> {
        if self.source.connection_id != candidate.connection_id
            || self.source.supplier_id != candidate.supplier_id
            || self.source.product_id != candidate.product_id
            || self.source.base.id != current.base.id
            || self.source.connection_id != current.connection_id
            || self.source.supplier_id != current.supplier_id
            || self.source.product_id != current.product_id
        {
            return Err(Error::ConflictError("同源规格的公司商品依据不一致，请重新核对".into()));
        }
        let mut source = self.source.clone();
        // 依据规格可先完成新增供给；实际公司 SKU 归属由 ensure_target 再次核验。
        source.offering_id = current.offering_id.clone();
        CatalogAdoptionPreview {
            base: source.base.clone(),
            connection_id: source.connection_id.clone(),
            actor_id: String::new(),
            // 此对象仅复用来源内容核对；依据有效性由批次与目标版本控制。
            expires_at: Instant::now(),
            items: vec![source],
        }
        .ensure_current_item(current)
    }

    /// 核验实际匹配目标；本操作追加规格可改变父商品版本，不能改变资料修订。
    /// # 参数
    /// `current` 为当前自动匹配的目标，`same_operation_product` 证明本操作已写入该商品回执。
    /// # 返回
    /// 目标身份、SKU 版本及父商品资料仍一致时成功。
    /// # 错误
    /// 换 SKU、换商品、资料修订变化或无本操作证据的版本变化时拒绝。
    pub fn ensure_target(&self, current: &BatchAnchorTarget, same_operation_product: bool) -> Result<()> {
        let stable = self.target.product_id == current.product_id
            && self.target.sku_id == current.sku_id
            && self.target.sku_version == current.sku_version
            && self.target.product_revision_id == current.product_revision_id;
        if !stable || (self.target.product_version != current.product_version && !same_operation_product) {
            return Err(Error::ConflictError("同源规格的公司商品或依据 SKU 已变化，请重新核对".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::connector::common::{Snapshot, SourceRevision, SourceStamp, SupplierSku};
    use crate::ports::connector::offer::{Offer, OfferDetails};

    fn anchor() -> BatchMatchAnchor {
        let source = SupplierCatalogItem::observed(
            "connection",
            "supplier",
            "sync",
            Snapshot {
                stamp: SourceStamp {
                    revision: SourceRevision::Unversioned,
                    changed_at: None,
                    observed_at: Instant::from_unix_secs(1),
                },
                value: Offer {
                    sku: SupplierSku { product_id: Some("rice".into()), spec_id: "5kg".into() },
                    name: "大米".into(),
                    specification: "5kg".into(),
                    unit: Some("袋".into()),
                    quote: None,
                    details: OfferDetails::default(),
                },
            },
        )
        .unwrap();
        BatchMatchAnchor {
            source,
            mapping: AdoptionMapping {
                category_id: "food".into(),
                unit_id: "bag".into(),
                product_kind: "PHYSICAL".into(),
                company_name: None,
                company_description: None,
            },
            target: BatchAnchorTarget {
                product_id: "company".into(),
                sku_id: "sku".into(),
                sku_version: 1,
                product_version: 2,
                product_revision_id: "revision".into(),
            },
        }
    }

    #[test]
    fn same_source_group_is_required_even_when_anchor_is_not_submitted() {
        let anchor = anchor();
        let mut candidate = anchor.source.clone();
        candidate.spec_id = "10kg".into();
        candidate.base.id = "another-source-sku".into();
        anchor.ensure_source(&candidate, &anchor.source).unwrap();
        for field in ["connection", "supplier", "product"] {
            let mut foreign = candidate.clone();
            match field {
                "connection" => foreign.connection_id = "other".into(),
                "supplier" => foreign.supplier_id = "other".into(),
                _ => foreign.product_id = "other".into(),
            }
            assert!(anchor.ensure_source(&foreign, &anchor.source).is_err());
        }
        let mut current = anchor.source.clone();
        current.purchase_price = Some("99".into());
        assert!(anchor.ensure_source(&candidate, &current).is_err());
    }

    #[test]
    fn prior_supply_link_is_allowed_but_target_identity_and_revisions_cannot_change() {
        let anchor = anchor();
        let mut current_source = anchor.source.clone();
        current_source.offering_id = Some("new-offering".into());
        current_source.sku_id = Some("sku".into());
        anchor.ensure_source(&anchor.source, &current_source).unwrap();
        anchor.ensure_target(&anchor.target, false).unwrap();
        let mut current = anchor.target.clone();
        current.product_version += 1;
        assert!(anchor.ensure_target(&current, false).is_err());
        anchor.ensure_target(&current, true).unwrap();
        current.product_revision_id = "changed".into();
        assert!(anchor.ensure_target(&current, true).is_err());
        current = anchor.target.clone();
        current.sku_id = "other".into();
        assert!(anchor.ensure_target(&current, true).is_err());
        current = anchor.target.clone();
        current.sku_version += 1;
        assert!(anchor.ensure_target(&current, true).is_err());
    }
}
