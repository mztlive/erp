//! 供应商付款单查询、银行回单与过账编排。

use std::collections::{HashMap, HashSet};

use erp_core::ids::{FileAssetId, PartyBankAccountId, SupplierPaymentId};
use erp_core::money::Amount;
use erp_finance::entity::payable::{AllocationAction, PaymentAllocation, SupplierPayment};
use erp_finance::repository::PayableExt;
use erp_finance::repository::prelude::*;
use erp_party::repository::prelude::*;
use erp_party::{PartyBankAccount, PartyExt};
use erp_supplier::repository::prelude::*;
use erp_supplier::{SupplierAccount, SupplierExt};
use erp_support::FileAssetExt;
use erp_support::repository::prelude::*;
use persistence_core::Executor;

use super::dto::{
    PaymentAllocationView, PaymentRecipientView, SupplierPaymentBankReceiptView, SupplierPaymentView,
};
use super::mapping::{payment_recipient_view, zero_amount};
use super::{PayableReadService, display};
use crate::finance::command_snapshot;
use crate::{Error, Result};

impl PayableReadService {
    // -----------------------------------------------------------------------
    // 供应商付款单
    // -----------------------------------------------------------------------

    /// 在原权限校验事务内读取实际付款、核销、回单及冲正的完整详情。
    ///
    /// # 参数
    /// * `id` - 付款事实身份。
    /// * `executor` - 当前资格校验使用的原事务执行器。
    /// # 返回
    /// 返回与同一事务事实一致的供应商付款视图。
    /// # 错误
    /// 付款不存在、回单缺失或关联事实读取失败时返回错误。
    pub async fn supplier_payment_detail_with_executor(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierPaymentView> {
        command_snapshot(executor)?;
        let ids = [SupplierPaymentId::new(id)];
        let payments = self.db.supplier_payments().find_supplier_payments_by_ids(&ids, executor).await?;
        let ordered = ordered_supplier_payments(&ids, &payments)?;
        let allocations = self.db.payment_allocations().find_allocations_by_payments(&ids, executor).await?;
        let (totals, grouped) = payment_allocation_groups(&ordered, &allocations);
        let enriched =
            display::enrich_payment_allocation_views_batched_with_executor(&self.db, grouped, executor)
                .await?;
        let suppliers = self.supplier_displays_by_ids_with_executor(&ordered, executor).await?;
        let receipts = self.bank_receipt_views_by_ids_with_executor(&ordered, executor).await?;
        let recipients = self.recipient_views_by_ids_with_executor(&ordered, executor).await?;
        let mut views =
            map_supplier_payment_views(&ordered, totals, enriched, &suppliers, &receipts, &recipients);
        self.attach_supplier_payment_reversals_with_executor(&mut views, executor).await?;
        views.pop().ok_or_else(|| Error::Internal("供应商付款详情装配失败".into()))
    }

    async fn supplier_displays_by_ids_with_executor(
        &self,
        payments: &[&SupplierPayment],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, (Option<String>, Option<String>)>> {
        let mut seen = HashSet::new();
        let supplier_ids = payments
            .iter()
            .filter(|payment| seen.insert(payment.supplier_id.to_string()))
            .map(|payment| payment.supplier_id.clone())
            .collect::<Vec<_>>();
        let suppliers = self.db.supplier_accounts().find_accounts_by_ids(&supplier_ids, executor).await?;
        let mut seen_parties = HashSet::new();
        let mut party_ids = Vec::new();
        for supplier in &suppliers {
            if seen_parties.insert(supplier.party_id.to_string()) {
                party_ids.push(supplier.party_id.clone());
            }
        }
        let parties = self.db.parties().find_parties_by_ids(&party_ids, executor).await?;
        let revision_ids: Vec<String> = parties
            .iter()
            .filter_map(|party| party.stable.current_revision_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let revisions = self.db.party_revisions().find_revisions_by_ids(&revision_ids, executor).await?;
        let suppliers_by_id: HashMap<&str, &SupplierAccount> =
            suppliers.iter().map(|supplier| (supplier.base.id.as_str(), supplier)).collect();
        let parties_by_id: HashMap<String, Option<String>> = parties
            .iter()
            .map(|party| (party.base.id.clone(), party.stable.current_revision_id.clone()))
            .collect();
        let names_by_revision: HashMap<&str, &str> = revisions
            .iter()
            .map(|revision| (revision.base.id.as_str(), revision.legal_name.as_str()))
            .collect();
        let mut displays = HashMap::with_capacity(payments.len());
        for payment in payments {
            let display = suppliers_by_id
                .get(payment.supplier_id.as_ref())
                .map(|supplier| {
                    let supplier_no = Some(supplier.supplier_no.clone());
                    let supplier_name = parties_by_id
                        .get(supplier.party_id.as_ref())
                        .and_then(|revision| revision.as_deref())
                        .and_then(|revision_id| {
                            names_by_revision.get(revision_id).map(|name| name.to_string())
                        });
                    (supplier_no, supplier_name)
                })
                .unwrap_or((None, None));
            displays.insert(payment.base.id.clone(), display);
        }
        Ok(displays)
    }

    async fn bank_receipt_views_by_ids_with_executor(
        &self,
        payments: &[&SupplierPayment],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, SupplierPaymentBankReceiptView>> {
        let mut seen = HashSet::new();
        let mut asset_ids = Vec::new();
        for payment in payments {
            if let Some(asset_id) = payment.bank_receipt_asset_id.as_ref()
                && seen.insert(asset_id.to_string())
            {
                asset_ids.push(FileAssetId::new(asset_id.to_string()));
            }
        }
        let assets = self.db.file_assets().find_by_ids(&asset_ids, executor).await?;
        let assets_by_id: HashMap<&str, &erp_support::FileAsset> =
            assets.iter().map(|asset| (asset.base.id.as_str(), asset)).collect();
        let mut views = HashMap::with_capacity(payments.len());
        for payment in payments {
            if let Some(asset_id) = payment.bank_receipt_asset_id.as_ref() {
                let asset = assets_by_id
                    .get(asset_id.as_ref())
                    .ok_or_else(|| Error::NotFound("银行回单不存在".to_string()))?;
                views.insert(
                    payment.base.id.clone(),
                    SupplierPaymentBankReceiptView {
                        asset_id: asset.base.id.clone(),
                        file_name: asset.file_name.clone(),
                        content_type: asset.content_type.clone(),
                        byte_size: asset.byte_size,
                    },
                );
            }
        }
        Ok(views)
    }

    async fn recipient_views_by_ids_with_executor(
        &self,
        payments: &[&SupplierPayment],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, PaymentRecipientView>> {
        let mut seen = HashSet::new();
        let mut account_ids = Vec::new();
        for payment in payments {
            if let Some(account_id) = payment.payee_bank_account_id.as_ref()
                && seen.insert(account_id.to_string())
            {
                account_ids.push(PartyBankAccountId::new(account_id.to_string()));
            }
        }
        let accounts =
            self.db.party_bank_accounts().find_bank_accounts_by_ids(&account_ids, executor).await?;
        let accounts_by_id: HashMap<&str, &PartyBankAccount> =
            accounts.iter().map(|account| (account.base.id.as_str(), account)).collect();
        let mut views = HashMap::new();
        for payment in payments {
            if let Some(account_id) = payment.payee_bank_account_id.as_ref()
                && let Some(account) = accounts_by_id.get(account_id.as_ref())
            {
                views.insert(payment.base.id.clone(), payment_recipient_view(account));
            }
        }
        Ok(views)
    }
}

/// 按调用方 ID 次序读取已批量加载的付款；缺失身份依原次序报错。
fn ordered_supplier_payments<'a>(
    payment_ids: &[SupplierPaymentId],
    payments: &'a [SupplierPayment],
) -> Result<Vec<&'a SupplierPayment>> {
    let payments_by_id: HashMap<&str, &SupplierPayment> =
        payments.iter().map(|payment| (payment.base.id.as_str(), payment)).collect();
    payment_ids
        .iter()
        .map(|id| {
            payments_by_id
                .get(id.as_ref())
                .copied()
                .ok_or_else(|| Error::NotFound("供应商付款单不存在".to_string()))
        })
        .collect()
}

/// 以付款输入次序分组核销，并保持每组序号和身份的稳定排序。
fn payment_allocation_groups(
    ordered: &[&SupplierPayment],
    allocations: &[PaymentAllocation],
) -> (Vec<Amount>, Vec<Vec<PaymentAllocationView>>) {
    let mut allocations_by_payment: HashMap<String, Vec<&PaymentAllocation>> = HashMap::new();
    for allocation in allocations {
        let Some(payment_id) = allocation.source.supplier_payment_id() else { continue };
        allocations_by_payment.entry(payment_id.to_string()).or_default().push(allocation);
    }
    for group in allocations_by_payment.values_mut() {
        group.sort_by(|left, right| {
            left.allocation_seq.cmp(&right.allocation_seq).then_with(|| left.base.id.cmp(&right.base.id))
        });
    }
    let mut grouped_views = Vec::with_capacity(ordered.len());
    let mut allocated_totals = Vec::with_capacity(ordered.len());
    for payment in ordered {
        let group = allocations_by_payment.get(payment.base.id.as_str()).cloned().unwrap_or_default();
        let owned: Vec<PaymentAllocation> = group.into_iter().map(|item| (*item).clone()).collect();
        let (allocated_total, views) = payment_allocation_view(&owned);
        grouped_views.push(views);
        allocated_totals.push(allocated_total);
    }
    (allocated_totals, grouped_views)
}

/// 将已经完成错误仲裁的独立投影写入付款响应，保持字段和输入次序。
fn map_supplier_payment_views(
    ordered: &[&SupplierPayment],
    allocated_totals: Vec<Amount>,
    enriched_groups: Vec<Vec<PaymentAllocationView>>,
    supplier_displays: &HashMap<String, (Option<String>, Option<String>)>,
    receipt_views: &HashMap<String, SupplierPaymentBankReceiptView>,
    recipient_views: &HashMap<String, PaymentRecipientView>,
) -> Vec<SupplierPaymentView> {
    ordered
        .iter()
        .zip(allocated_totals)
        .zip(enriched_groups)
        .map(|((payment, allocated_total), enriched)| {
            let (supplier_no, supplier_name) =
                supplier_displays.get(payment.base.id.as_str()).cloned().unwrap_or((None, None));
            SupplierPaymentView {
                id: payment.base.id.clone(),
                payment_no: payment.payment_no.clone(),
                status: payment.status,
                supplier_id: payment.supplier_id.to_string(),
                supplier_no,
                supplier_name,
                payment_recipient: recipient_views.get(payment.base.id.as_str()).cloned(),
                paid_at: payment.paid_at,
                amount: payment.amount,
                reversed_amount: payment.reversed_amount,
                effective_amount: payment.effective_amount(),
                bank_reference: payment.bank_reference.clone(),
                bank_receipt: receipt_views.get(payment.base.id.as_str()).cloned(),
                version: payment.base.version,
                created_at: payment.base.created_at,
                unallocated_amount: payment.effective_amount().checked_sub(allocated_total),
                allocated_total,
                allocations: enriched,
                related_reversals: Vec::new(),
            }
        })
        .collect()
}

/// 汇总付款核销分配并装配视图。
///
/// # 参数
/// * `allocations` - 付款核销分配集合
///
/// # 返回
/// 返回 `(净已核销合计, 分配视图列表)`。
fn payment_allocation_view(allocations: &[PaymentAllocation]) -> (Amount, Vec<PaymentAllocationView>) {
    let mut net = zero_amount();
    let views = allocations
        .iter()
        .map(|allocation| {
            match allocation.allocation_action {
                AllocationAction::Apply => net = net.checked_add(allocation.allocated_amount),
                AllocationAction::Reverse => net = net.checked_sub(allocation.allocated_amount),
            }
            allocation.into()
        })
        .collect();
    (net, views)
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::{FileAssetId, PartyBankAccountId, SupplierAccountId, SupplierPaymentId};
    use erp_finance::entity::payable::{SupplierPayment, SupplierPaymentData};

    use super::map_supplier_payment_views;

    /// 冲正款不是可再次核销的未分配资金，部分和全额冲正均使用有效金额。
    #[test]
    fn unallocated_payment_balance_excludes_formal_reversals() {
        let mut payment = SupplierPayment::new(
            SupplierPaymentId::new("payment"),
            SupplierPaymentData {
                payment_no: "FK-1".into(),
                supplier_id: SupplierAccountId::new("supplier"),
                payee_bank_account_id: PartyBankAccountId::new("bank"),
                paid_at: Instant::now(),
                amount: "100".parse().unwrap(),
                bank_reference: None,
                bank_receipt_asset_id: FileAssetId::new("receipt"),
            },
            "cashier",
        )
        .unwrap();
        for (reversed, allocated, unallocated) in [("40", "60", "0"), ("40", "50", "10"), ("100", "0", "0")] {
            payment.reversed_amount = reversed.parse().unwrap();
            let views = map_supplier_payment_views(
                &[&payment],
                vec![allocated.parse().unwrap()],
                vec![Vec::new()],
                &Default::default(),
                &Default::default(),
                &Default::default(),
            );
            assert_eq!(views[0].reversed_amount, reversed.parse().unwrap());
            assert_eq!(views[0].unallocated_amount, unallocated.parse().unwrap());
        }
    }
}
