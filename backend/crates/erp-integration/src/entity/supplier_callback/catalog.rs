//! 商品回调领取与逐规格检查点；保存事实后才能推进。
use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};

use super::{SupplierCallbackReceipt, SupplierRefreshHint};
use crate::{Error, Result};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogCallbackProgress {
    pub hint_index: usize,
    pub after_id: Option<String>,
}

impl SupplierCallbackReceipt {
    /// 校验目录消费者可处理的路由与提示，禁止误收订单。
    /// # 参数
    /// 无。
    /// # 返回
    /// 全部提示与目录路由一致时成功。
    /// # 错误
    /// 混合、未知或损坏的提示拒绝处理。
    pub fn ensure_catalog(&self) -> Result<()> {
        let topic = match self.route_kind.as_str() {
            "status" => "availability",
            "product" => "product",
            "cities" => "region",
            "price" => "price",
            _ => return Err(Error::ValidationError("非商品回调不能进入目录消费者".into())),
        };
        if self.refresh_hints.is_empty()
            || self.catalog_progress.hint_index > self.refresh_hints.len()
            || self.refresh_hints.iter().any(|hint| {
                hint.topic != topic
                    || !match self.route_kind.as_str() {
                        "price" => hint.target_kind == "sku" && hint.product_id.is_some(),
                        "product" => hint.target_kind == "product",
                        _ => matches!(hint.target_kind.as_str(), "product" | "brand"),
                    }
            })
        {
            return Err(Error::ValidationError("商品回调路由、提示或检查点不一致".into()));
        }
        Ok(())
    }

    /// 领取到期记录；延期时间同时作为可恢复的短租约。
    /// # 参数
    /// `now` 为当前服务器时间。
    /// # 返回
    /// 到期且仍待处理返回 true，完成或租约内返回 false。
    /// # 错误
    /// 非商品回调或检查点异常时拒绝。
    pub fn claim_catalog(&mut self, now: Instant) -> Result<bool> {
        self.ensure_catalog()?;
        if self.status != "received" || self.next_attempt_at.is_some_and(|at| at > now) {
            return Ok(false);
        }
        self.next_attempt_at = Some(Instant::from_unix_secs(now.unix_secs().saturating_add(300)));
        Ok(true)
    }

    /// 取得当前尚未完成的目录提示。
    /// # 参数
    /// 无。
    /// # 返回
    /// 全部完成返回 None。
    /// # 错误
    /// 路由或检查点不一致时拒绝。
    pub fn catalog_hint(&self) -> Result<Option<&SupplierRefreshHint>> {
        self.ensure_catalog()?;
        Ok(self.refresh_hints.get(self.catalog_progress.hint_index))
    }

    /// 保存已应用规格游标；SKU 单项提示可同时完成。
    /// # 参数
    /// `id` 为已应用目录主键，`single` 表示单个 SKU 提示。
    /// # 返回
    /// 逐规格推进，不跳过尚未保存的对象。
    /// # 错误
    /// 已结束提示、空身份或游标倒退时拒绝。
    pub fn advance_catalog(&mut self, id: &str, single: bool) -> Result<()> {
        let hint = self.catalog_hint()?.ok_or_else(|| Error::ConflictError("商品提示已处理".into()))?;
        if id.is_empty()
            || single != (hint.target_kind == "sku")
            || self.catalog_progress.after_id.as_deref().is_some_and(|after| id <= after)
        {
            return Err(Error::ConflictError("商品回调检查点不能跳过或重复推进".into()));
        }
        self.catalog_progress.after_id = Some(id.into());
        if single {
            self.finish_catalog_hint()?;
        }
        Ok(())
    }

    /// 分组已无下一规格时结束当前提示，空基线不能记成功。
    /// # 参数
    /// 无，调用方已核实没有剩余目标。
    /// # 返回
    /// 进入下一个提示。
    /// # 错误
    /// 没有应用任何规格或提示已结束时拒绝。
    pub fn finish_catalog_hint(&mut self) -> Result<()> {
        if self.catalog_hint()?.is_none() || self.catalog_progress.after_id.is_none() {
            return Err(Error::ValidationError("回调目标尚无目录基线，请先同步目录".into()));
        }
        self.catalog_progress.hint_index += 1;
        self.catalog_progress.after_id = None;
        Ok(())
    }

    /// 全部规格完成后结算回调，或让下一有界批次继续。
    /// # 参数
    /// `now` 为本次应用时间。
    /// # 返回
    /// 全部完成为 true，同时清理历史错误。
    /// # 错误
    /// 路由和检查点异常时拒绝。
    pub fn settle_catalog(&mut self, now: Instant) -> Result<bool> {
        let complete = self.catalog_hint()?.is_none();
        self.processing_error = None;
        self.next_attempt_at = if complete { None } else { Some(now) };
        if complete {
            self.status = "applied".into();
            self.processed_at = Some(now);
        }
        Ok(complete)
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::SupplierApiConnectionId;

    use super::*;

    fn receipt(route: &str, topic: &str, kind: &str) -> SupplierCallbackReceipt {
        SupplierCallbackReceipt::received(
            "receipt".into(),
            SupplierApiConnectionId::new("connection"),
            Instant::from_unix_secs(1),
            route.into(),
            "cipher".into(),
            vec![SupplierRefreshHint {
                topic: topic.into(),
                target_kind: kind.into(),
                object_id: "target".into(),
                product_id: Some("product".into()),
                funds_transaction_id: None,
            }],
        )
        .unwrap()
    }

    #[test]
    fn catalog_callbacks_complete_only_after_all_persisted_targets_and_preserve_evidence() {
        for (route, topic, kind) in [
            ("status", "availability", "brand"),
            ("product", "product", "product"),
            ("cities", "region", "product"),
            ("price", "price", "sku"),
        ] {
            let mut value = receipt(route, topic, kind);
            assert!(value.claim_catalog(Instant::from_unix_secs(10)).unwrap());
            assert!(!value.claim_catalog(Instant::from_unix_secs(11)).unwrap());
            assert!(value.finish_catalog_hint().is_err());
            assert!(!value.settle_catalog(Instant::from_unix_secs(12)).unwrap());
            assert_eq!(value.status, "received");
            value.advance_catalog("catalog-1", kind == "sku").unwrap();
            if kind != "sku" {
                value.finish_catalog_hint().unwrap();
            }
            value.processing_error = Some("previous failure".into());
            assert!(value.settle_catalog(Instant::from_unix_secs(20)).unwrap());
            assert_eq!(value.status, "applied");
            assert_eq!(value.body_encrypted, "cipher");
            assert_eq!(value.processing_error, None);
            assert_eq!(value.next_attempt_at, None);
            assert!(!value.claim_catalog(Instant::from_unix_secs(999)).unwrap());
        }
    }

    #[test]
    fn large_brand_resumes_after_restart_and_rejects_duplicate_or_backward_progress() {
        let mut value = receipt("status", "availability", "brand");
        assert!(value.claim_catalog(Instant::from_unix_secs(10)).unwrap());
        for n in 0..120 {
            value.advance_catalog(&format!("catalog-{n:04}"), false).unwrap();
        }
        let wire = serde_json::to_value(&value).unwrap();
        let mut restored: SupplierCallbackReceipt = serde_json::from_value(wire).unwrap();
        assert_eq!(restored.catalog_progress.after_id.as_deref(), Some("catalog-0119"));
        assert!(restored.advance_catalog("catalog-0119", false).is_err());
        assert!(restored.advance_catalog("catalog-0001", false).is_err());
        assert!(!restored.claim_catalog(Instant::from_unix_secs(309)).unwrap());
        assert!(restored.claim_catalog(Instant::from_unix_secs(310)).unwrap());
        restored.advance_catalog("catalog-0120", false).unwrap();
        restored.finish_catalog_hint().unwrap();
        assert!(restored.settle_catalog(Instant::from_unix_secs(311)).unwrap());
    }

    #[test]
    fn old_records_start_at_first_hint_and_order_or_mixed_routes_are_rejected() {
        let mut wire = serde_json::to_value(receipt("price", "price", "sku")).unwrap();
        wire.as_object_mut().unwrap().remove("catalog_progress");
        let restored: SupplierCallbackReceipt = serde_json::from_value(wire).unwrap();
        assert_eq!(restored.catalog_progress, CatalogCallbackProgress::default());
        let order = receipt("order", "order", "merchant_order");
        assert!(order.ensure_catalog().is_err());
        let mut mixed = receipt("status", "availability", "brand");
        mixed.refresh_hints.push(order.refresh_hints[0].clone());
        assert!(mixed.ensure_catalog().is_err());
        let mut wrong = receipt("cities", "region", "product");
        wrong.catalog_progress.hint_index = 2;
        assert!(wrong.catalog_hint().is_err());
    }

    #[test]
    fn price_batch_does_not_finish_after_first_sku() {
        let mut value = receipt("price", "price", "sku");
        value.refresh_hints.push(value.refresh_hints[0].clone());
        value.advance_catalog("sku-1", true).unwrap();
        assert!(!value.settle_catalog(Instant::from_unix_secs(3)).unwrap());
        assert_eq!(value.catalog_progress.hint_index, 1);
        value.advance_catalog("sku-2", true).unwrap();
        assert!(value.settle_catalog(Instant::from_unix_secs(4)).unwrap());
    }
}
