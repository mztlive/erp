//! 供应商来源采购价的只读约束与不可变修订。
use erp_core::ids::SupplierOfferingRevisionId;
use erp_core::money::{Rate, UnitPrice};

use super::{FromGrossPricesParams, SupplierOfferingRevision, SupplierOfferingRevisionData};
use crate::entity::supplier_catalog::SupplierCatalogItem;
use crate::{Error, Result};

impl SupplierCatalogItem {
    /// 将当前目录约束绑定到服务端构造的 API 供给首版。
    /// # 参数
    /// `revision` 为尚未持久化的首版条款，不接受客户端采购限制。
    /// # 返回
    /// 核验来源报价和已报告税率后，冻结真实来源条款。
    /// # 错误
    /// 报价、来源约束或已报告税率与待建条款不一致时拒绝。
    pub fn bind_purchase_terms(&self, revision: &mut SupplierOfferingRevision) -> Result<()> {
        if let Some(terms) = &self.source_purchase_terms {
            terms.validate()?;
            if terms
                .quote_context
                .as_ref()
                .and_then(|context| context.tax_rate)
                .is_some_and(|rate| rate != revision.input_tax_rate)
            {
                return Err(Error::ValidationError("采购税率必须与供应商已核实税率一致".into()));
            }
        }

        revision.source_purchase_terms = self.source_purchase_terms.clone();
        self.ensure_supply_prices(revision)
    }

    /// 读取供应商明确提供的采购报价，不允许用人工价填补缺失。
    /// # 参数
    /// 无；读取当前目录观察。
    /// # 返回
    /// 非负的四位精度采购单价，保留零价。
    /// # 错误
    /// 报价缺失、非法或为负数时拒绝。
    pub fn purchase_quote(&self) -> Result<UnitPrice> {
        let price: UnitPrice = self
            .purchase_price
            .as_deref()
            .ok_or_else(|| Error::ValidationError("供应商尚未提供采购报价，请同步报价后再入库".into()))?
            .parse()
            .map_err(|_| Error::ValidationError("供应商采购报价格式无效".into()))?;
        if price.to_decimal().is_sign_negative() {
            return Err(Error::ValidationError("供应商采购报价不能为负数".into()));
        }
        Ok(price)
    }

    /// 校验当前目录支持的代发采购报价，不接受来源未提供的集采条件。
    /// # 参数
    /// `revision` 为待创建的供给首版条款。
    /// # 返回
    /// 采购价一致且未编造集采条件时成功。
    /// # 错误
    /// 报价缺失、被改写或提交来源不存在的集采条款时拒绝。
    pub fn ensure_supply_prices(&self, revision: &SupplierOfferingRevision) -> Result<()> {
        self.ensure_purchase_price(revision.dropship_supply_price_gross)?;
        if self.source_purchase_terms != revision.source_purchase_terms {
            return Err(Error::ValidationError("供应商采购限制或报价上下文已变化，请刷新后再入库".into()));
        }
        if revision.bulk_supply_price_gross.is_some() || revision.bulk_minimum_order_quantity.is_some() {
            return Err(Error::ValidationError(
                "供应商尚未提供集采报价和起订量，不能手工补填，请等待供应商更新报价".into(),
            ));
        }
        Ok(())
    }

    /// 校验采购供给价等于当前供应商报价。
    /// # 参数
    /// `price` 为请求中的含税采购价。
    /// # 返回
    /// 数值相同则成功，允许不同的等价小数表示。
    /// # 错误
    /// 来源缺报价或请求改价时拒绝。
    pub fn ensure_purchase_price(&self, price: UnitPrice) -> Result<()> {
        if self.purchase_quote()? != price {
            return Err(Error::ValidationError("采购供给价由供应商同步，不可手工修改，请刷新报价".into()));
        }
        Ok(())
    }
}

impl SupplierOfferingRevision {
    /// 根据新的供应商采购价追加修订，保留公司补充条件及历史版本。
    /// # 参数
    /// `id`、`revision_no` 为新版本身份，`price` 为已核验的来源含税价。
    /// # 返回
    /// 报价未变时返回空，否则返回重算不含税价的新修订。
    /// # 错误
    /// 新版本号或商业条件无效时拒绝。
    pub fn with_source_price(
        &self,
        id: SupplierOfferingRevisionId,
        revision_no: u32,
        price: UnitPrice,
    ) -> Result<Option<Self>> {
        if self.dropship_supply_price_gross == price {
            return Ok(None);
        }
        let mut next = self.next_priced_revision(id, revision_no, price, self.input_tax_rate)?;
        next.source_purchase_terms = self.source_purchase_terms.clone();
        Ok(Some(next))
    }

    /// 按真实来源价格、税率和采购限制追加不可变修订。
    /// # 参数
    /// `id` 与 `revision_no` 为新版本身份，`item` 为已核验同订货身份的目录。
    /// # 返回
    /// 实际条件不变时返回空；来源条件变化时保留历史并返回新修订。
    /// # 错误
    /// 报价缺失、税率或来源约束无效时拒绝。
    pub fn with_source_observation(
        &self,
        id: SupplierOfferingRevisionId,
        revision_no: u32,
        item: &SupplierCatalogItem,
    ) -> Result<Option<Self>> {
        let price = item.purchase_quote()?;
        let tax_rate = item
            .source_purchase_terms
            .as_ref()
            .and_then(|terms| terms.quote_context.as_ref())
            .and_then(|context| context.tax_rate)
            .unwrap_or(self.input_tax_rate);

        let previous = self.source_purchase_terms.as_ref().map(|terms| terms.version_content());
        let current = item.source_purchase_terms.as_ref().map(|terms| terms.version_content());
        if price == self.dropship_supply_price_gross && tax_rate == self.input_tax_rate && previous == current
        {
            return Ok(None);
        }

        let mut next = self.next_priced_revision(id, revision_no, price, tax_rate)?;
        item.bind_purchase_terms(&mut next)?;
        Ok(Some(next))
    }

    fn next_priced_revision(
        &self,
        id: SupplierOfferingRevisionId,
        revision_no: u32,
        price: UnitPrice,
        input_tax_rate: Rate,
    ) -> Result<Self> {
        let data = SupplierOfferingRevisionData::from_gross_prices(FromGrossPricesParams {
            supplier_offering_id: self.supplier_offering_id.clone(),
            revision_no,
            dropship_supply_price_gross: price,
            bulk_supply_price_gross: self.bulk_supply_price_gross,
            input_tax_rate,
            bulk_minimum_order_quantity: self.bulk_minimum_order_quantity,
            product_capabilities: self.product_capabilities.clone(),
            valid_from: self.valid_from,
            valid_to: self.valid_to,
            prefill_source_refs: self.prefill_source_refs.clone(),
        });
        Self::new(id, data).map_err(Error::from)
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::SupplierOfferingId;

    use super::*;
    use crate::entity::source_purchase::{SourcePurchaseTerms, SourceQuoteContext};
    use crate::ports::connector::common::{Snapshot, SourceRevision, SourceStamp, SupplierSku};
    use crate::ports::connector::offer::{Offer, OfferDetails};

    fn catalog(rate: &str, minimum: &str) -> SupplierCatalogItem {
        let mut item = SupplierCatalogItem::observed(
            "c",
            "s",
            "job",
            Snapshot {
                stamp: SourceStamp {
                    revision: SourceRevision::Unversioned,
                    changed_at: None,
                    observed_at: Instant::from_unix_secs(100),
                },
                value: Offer {
                    sku: SupplierSku { product_id: Some("p".into()), spec_id: "s".into() },
                    name: "商品".into(),
                    specification: "规格".into(),
                    unit: Some("件".into()),
                    quote: None,
                    details: OfferDetails {
                        source_purchase_terms: Some(SourcePurchaseTerms {
                            minimum_quantity: Some(minimum.parse().unwrap()),
                            quote_context: Some(SourceQuoteContext {
                                quantity: "1".parse().unwrap(),
                                observed_at: Instant::from_unix_secs(100),
                                tax_rate: Some(rate.parse().unwrap()),
                                tax_rate_raw: Some(rate.into()),
                                address_fingerprint: None,
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                },
            },
        )
        .unwrap();
        item.purchase_price = Some("80".into());
        item
    }

    fn revision() -> SupplierOfferingRevision {
        SupplierOfferingRevision::new(
            SupplierOfferingRevisionId::new("old"),
            SupplierOfferingRevisionData::from_gross_prices(FromGrossPricesParams {
                supplier_offering_id: SupplierOfferingId::new("offering"),
                revision_no: 1,
                dropship_supply_price_gross: "80".parse().unwrap(),
                bulk_supply_price_gross: None,
                input_tax_rate: "0.13".parse().unwrap(),
                bulk_minimum_order_quantity: None,
                product_capabilities: vec!["REFUND".into()],
                valid_from: "2026-10-27".parse().unwrap(),
                valid_to: Some("2026-11-27".parse().unwrap()),
                prefill_source_refs: Default::default(),
            }),
        )
        .unwrap()
    }

    #[test]
    fn supplier_price_changes_append_history_and_recalculate_net_without_overwriting_internal_terms() {
        let old = revision();
        let next = old
            .with_source_price(SupplierOfferingRevisionId::new("next"), 2, "90".parse().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(old.dropship_supply_price_gross, "80".parse().unwrap());
        assert_eq!(next.dropship_supply_price_gross, "90".parse().unwrap());
        assert_eq!(next.dropship_supply_price_net, "78.30".parse().unwrap());
        assert_eq!(next.revision.revision_no, 2);
        assert_eq!(next.input_tax_rate, old.input_tax_rate);
        assert_eq!(next.valid_from, old.valid_from);
        assert_eq!(next.valid_to, old.valid_to);
        assert_eq!(next.product_capabilities, old.product_capabilities);
        assert_eq!(next.bulk_supply_price_gross, old.bulk_supply_price_gross);
        assert_eq!(next.bulk_minimum_order_quantity, old.bulk_minimum_order_quantity);
        assert!(next.ensure_api_prices_unchanged(&old).is_err());
    }

    #[test]
    fn unchanged_price_is_idempotent_and_zero_price_is_preserved() {
        let old = revision();
        assert!(
            old.with_source_price(SupplierOfferingRevisionId::new("same"), 2, "80.0000".parse().unwrap())
                .unwrap()
                .is_none()
        );

        let zero = old
            .with_source_price(SupplierOfferingRevisionId::new("zero"), 2, "0".parse().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(zero.dropship_supply_price_gross, "0".parse().unwrap());
        assert_eq!(zero.dropship_supply_price_net, "0".parse().unwrap());

        assert!(
            old.with_source_price(SupplierOfferingRevisionId::new("bad"), 2, "-1".parse().unwrap()).is_err()
        );
    }

    #[test]
    fn source_constraints_and_actual_tax_change_revision_even_with_unchanged_unit_price() {
        let mut old = revision();
        let mut original = catalog("0.13", "2");
        original.bind_purchase_terms(&mut old).unwrap();

        let mut same = original.clone();
        same.source_purchase_terms.as_mut().unwrap().quote_context.as_mut().unwrap().observed_at =
            Instant::from_unix_secs(200);
        assert!(
            old.with_source_observation(SupplierOfferingRevisionId::new("same"), 2, &same).unwrap().is_none()
        );

        let changed = catalog("0.09", "3");
        let next = old
            .with_source_observation(SupplierOfferingRevisionId::new("new"), 2, &changed)
            .unwrap()
            .unwrap();
        assert_eq!(next.dropship_supply_price_gross, old.dropship_supply_price_gross);
        assert_eq!(next.input_tax_rate, "0.09".parse().unwrap());
        assert_eq!(next.dropship_supply_price_net, "72.8".parse().unwrap());
        assert_eq!(next.source_purchase_terms, changed.source_purchase_terms);
        assert_eq!(old.source_purchase_terms, original.source_purchase_terms);
        assert!(next.source_purchase_terms.as_ref().unwrap().ensure_quantity("2".parse().unwrap()).is_err());

        let mut encoded = serde_json::to_value(next).unwrap();
        encoded.as_object_mut().unwrap().remove("source_purchase_terms");
        let restored: SupplierOfferingRevision = serde_json::from_value(encoded).unwrap();
        assert!(restored.source_purchase_terms.is_none());

        original.source_purchase_terms.as_mut().unwrap().quote_context.as_mut().unwrap().tax_rate =
            Some("0.09".parse().unwrap());
        assert!(original.bind_purchase_terms(&mut old).is_err());
    }
}
