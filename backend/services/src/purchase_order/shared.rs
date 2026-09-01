//! 跨采购单编排模块共享的最小 helper。

use std::str::FromStr;

use database::{NoTransaction, PartyExt, SupplierExt};
use entities::ids::SupplierAccountId;
use entities::money::{Amount, Rate};
use entities::purchase_order::{PaymentTermSnapshot, PurchaseChangeOrder, PurchaseOrder};
use entities::supplier::SupplierPaymentTerm;

use super::PurchaseOrderService;
use crate::errors::{Error, Result};

/// 零金额。
pub(super) fn zero_amount() -> Amount {
    Amount::from_str("0").expect("零金额合法")
}

/// 零税率。
pub(super) fn zero_rate() -> Rate {
    Rate::from_str("0").expect("零税率合法")
}

impl PurchaseOrderService {
    /// 校验乐观锁版本一致。
    pub(super) fn ensure_version(&self, entity: &impl Versioned, expected: u64) -> Result<()> {
        if entity.version() != expected {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        Ok(())
    }

    /// 解析供应商名称（D09 供应商角色 → D07 主体 → 当前主体修订法定名称）。
    pub(super) async fn resolve_supplier_name(
        &self,
        supplier_id: &SupplierAccountId,
    ) -> Result<Option<String>> {
        let supplier = self
            .db
            .supplier_accounts()
            .find_by_id(supplier_id, &mut NoTransaction)
            .await?;
        let Some(supplier) = supplier else { return Ok(None) };
        let party = self
            .db
            .parties()
            .find_by_id(&supplier.party_id, &mut NoTransaction)
            .await?;
        let Some(party) = party else { return Ok(None) };
        let Some(revision_id) = party.stable.current_revision_id else {
            return Ok(None);
        };
        let revision = self
            .db
            .party_revisions()
            .find_by_id(&revision_id, &mut NoTransaction)
            .await?;
        Ok(revision.map(|revision| revision.legal_name))
    }

    /// 解析付款条件并生成门禁快照（金额/比例门槛暂空）。
    pub(super) async fn payment_term_snapshot(&self, payment_term_code: &str) -> Result<PaymentTermSnapshot> {
        let payment_term = SupplierPaymentTerm::parse(payment_term_code)?;
        PaymentTermSnapshot::new(
            payment_term.code().to_string(),
            payment_term.prepay_gate(),
            None,
            None,
        )
        .map_err(Into::into)
    }
}

/// 版本化访问（乐观锁校验统一入口）。
pub(super) trait Versioned {
    /// 返回实体乐观锁版本。
    fn version(&self) -> u64;
}

impl Versioned for PurchaseOrder {
    fn version(&self) -> u64 {
        self.base.version
    }
}

impl Versioned for PurchaseChangeOrder {
    fn version(&self) -> u64 {
        self.base.version
    }
}
