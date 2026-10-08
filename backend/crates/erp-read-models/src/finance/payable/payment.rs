//! 供应商付款单查询、银行回单与过账编排。

use std::collections::{HashMap, HashSet};
use std::future::Future;

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
use persistence_core::NoTransaction;
use validator::Validate;

use super::dto::{
    PageView, PaymentAllocationView, PaymentRecipientView, SortDir, SupplierPaymentBankReceiptView,
    SupplierPaymentListParams, SupplierPaymentView,
};
use super::mapping::{payment_recipient_view, zero_amount};
use super::{PayableReadService, SupplierPaymentFilter, display};
use crate::{Error, Result};

impl PayableReadService {
    // -----------------------------------------------------------------------
    // 供应商付款单
    // -----------------------------------------------------------------------

    /// 分页查询供应商付款单列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`payment_no`/`supplier_id`/`status`）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// 参数校验失败，或关键词、付款单、视图装配与冲正读取失败时返回对应错误。
    pub async fn supplier_payment_list(
        &self,
        params: &SupplierPaymentListParams,
    ) -> Result<PageView<SupplierPaymentView>> {
        params.validate()?;
        let query = params.normalized()?;
        let keyword_ids = crate::finance::search::keyword_ids(
            &self.db,
            query.q.as_deref(),
            erp_finance::repository::keyword::FinanceSearchTarget::Payment,
        )
        .await?;
        let filter = SupplierPaymentFilter {
            keyword_ids,
            keyword: None,
            keyword_supplier_ids: Vec::new(),
            payment_no: query.payment_no,
            supplier_id: query.supplier_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self.db.supplier_payments().search_supplier_payments(&filter, &mut NoTransaction).await?;
        let payment_ids: Vec<SupplierPaymentId> =
            page.items.iter().map(|row| SupplierPaymentId::new(row.id.clone())).collect();
        let mut views = self.assemble_supplier_payment_views(&payment_ids, false).await?;
        self.attach_supplier_payment_reversals(&mut views).await?;
        Ok(PageView { items: views, total: page.total, page: filter.page, page_size: filter.page_size })
    }

    /// 查询供应商付款单详情（含核销分配行）。
    ///
    /// # 参数
    /// * `id` - 付款单 ID
    ///
    /// # 返回
    /// 返回付款单视图。
    ///
    /// # 错误
    /// 付款单不存在时返回 `NotFound`；冲正读取失败或详情装配失败时返回对应错误。
    pub async fn supplier_payment_detail(&self, id: &str) -> Result<SupplierPaymentView> {
        let mut views = vec![self.supplier_payment_view(id.to_string(), true).await?];
        self.attach_supplier_payment_reversals(&mut views).await?;
        views.pop().ok_or_else(|| Error::Internal("供应商付款详情装配失败".to_string()))
    }

    /// 装配供应商付款单视图（FIN-R02 单笔入口，经批量装载保持与列表一致）。
    ///
    /// # 参数
    /// * `id` - 付款单 ID
    /// * `include_payment_recipient` - 是否加载付款详情所需的冻结收款账户
    ///
    /// # 返回
    /// 返回付款单视图（含分配行与未分配余额）。
    ///
    /// # 错误
    /// * `NotFound` - 付款单不存在
    async fn supplier_payment_view(
        &self,
        id: String,
        include_payment_recipient: bool,
    ) -> Result<SupplierPaymentView> {
        let mut views = self
            .assemble_supplier_payment_views(
                std::slice::from_ref(&SupplierPaymentId::new(id)),
                include_payment_recipient,
            )
            .await?;
        views.pop().ok_or_else(|| Error::NotFound("供应商付款单不存在".to_string()))
    }

    /// 按付款 ID 集合批量装载并集中映射付款视图（FIN-R02）。
    ///
    /// 数据库往返固定：付款批量 1 次、核销分配 1 次、分配来源（分录／子账／
    /// 两类来源单号）共 4 次、供应商／主体／修订 3 次、银行回单 1 次、
    /// 冻结收款账户（仅详情）1 次，不随页长增长。响应映射、掩码与可见字段
    /// 选择集中在本函数；Repository 只返回实体事实，不返回 View、不执行
    /// 脱敏策略。缺失付款返回 `NotFound`；缺失分录／子账／主数据的行保持
    /// 对应展示字段为空；引用的银行回单缺失返回 `NotFound`，与原单笔语义一致。
    ///
    /// # 参数
    /// * `payment_ids` - 付款单 ID 集合（保持输入顺序装配）
    /// * `include_payment_recipient` - 是否加载冻结收款账户（仅详情）
    ///
    /// # 返回
    /// 返回与输入顺序对齐的付款单视图；空输入不访问数据库。
    ///
    /// # 错误
    /// 付款或其引用的银行回单缺失、仓储读取失败时返回错误。
    async fn assemble_supplier_payment_views(
        &self,
        payment_ids: &[SupplierPaymentId],
        include_payment_recipient: bool,
    ) -> Result<Vec<SupplierPaymentView>> {
        if payment_ids.is_empty() {
            return Ok(Vec::new());
        }
        let payments = self
            .db
            .supplier_payments()
            .find_supplier_payments_by_ids(payment_ids, &mut NoTransaction)
            .await?;
        let ordered = ordered_supplier_payments(payment_ids, &payments)?;
        let allocations = self
            .db
            .payment_allocations()
            .find_allocations_by_payments(payment_ids, &mut NoTransaction)
            .await?;
        let (allocated_totals, grouped_views) = payment_allocation_groups(&ordered, &allocations);
        let (enriched_groups, supplier_displays, receipt_views, recipient_views) = load_payment_projections(
            display::enrich_payment_allocation_views_batched(&self.db, grouped_views),
            self.supplier_displays_by_ids(&ordered),
            self.bank_receipt_views_by_ids(&ordered),
            async {
                if include_payment_recipient {
                    self.recipient_views_by_ids(&ordered).await
                } else {
                    Ok(HashMap::new())
                }
            },
        )
        .await?;
        Ok(map_supplier_payment_views(
            &ordered,
            allocated_totals,
            enriched_groups,
            &supplier_displays,
            &receipt_views,
            &recipient_views,
        ))
    }

    /// 按付款集合一次批量解析供应商展示名（FIN-R02）。
    ///
    /// 供应商、主体、修订各一次 `$in` 查询；主数据或来源修订缺失时对应字段
    /// 为空，不阻断列表。
    async fn supplier_displays_by_ids(
        &self,
        payments: &[&SupplierPayment],
    ) -> Result<HashMap<String, (Option<String>, Option<String>)>> {
        let mut seen = HashSet::new();
        let mut supplier_ids = Vec::new();
        for payment in payments {
            if seen.insert(payment.supplier_id.to_string()) {
                supplier_ids.push(payment.supplier_id.clone());
            }
        }
        let suppliers =
            self.db.supplier_accounts().find_accounts_by_ids(&supplier_ids, &mut NoTransaction).await?;
        let mut seen_parties = HashSet::new();
        let mut party_ids = Vec::new();
        for supplier in &suppliers {
            if seen_parties.insert(supplier.party_id.to_string()) {
                party_ids.push(supplier.party_id.clone());
            }
        }
        let parties = self.db.parties().find_parties_by_ids(&party_ids, &mut NoTransaction).await?;
        let revision_ids: Vec<String> = parties
            .iter()
            .filter_map(|party| party.stable.current_revision_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let revisions =
            self.db.party_revisions().find_revisions_by_ids(&revision_ids, &mut NoTransaction).await?;
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

    /// 按付款集合一次批量装载银行回单安全元数据（FIN-R02）。
    ///
    /// 只返回安全展示字段；付款引用的回单缺失时返回 `NotFound`，
    /// 与原单笔语义一致。
    async fn bank_receipt_views_by_ids(
        &self,
        payments: &[&SupplierPayment],
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
        let assets = self.db.file_assets().find_by_ids(&asset_ids, &mut NoTransaction).await?;
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

    /// 按付款集合一次批量装载冻结收款账户掩码视图（FIN-R02，仅详情）。
    ///
    /// 账户缺失时对应付款的收款账户为空；掩码只含末四位，
    /// 不泄漏敏感明文。
    async fn recipient_views_by_ids(
        &self,
        payments: &[&SupplierPayment],
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
            self.db.party_bank_accounts().find_bank_accounts_by_ids(&account_ids, &mut NoTransaction).await?;
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
        allocations_by_payment
            .entry(allocation.supplier_payment_id.to_string())
            .or_default()
            .push(allocation);
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
                bank_reference: payment.bank_reference.clone(),
                bank_receipt: receipt_views.get(payment.base.id.as_str()).cloned(),
                version: payment.base.version,
                created_at: payment.base.created_at,
                unallocated_amount: payment.amount.checked_sub(allocated_total),
                allocated_total,
                allocations: enriched,
                related_reversals: Vec::new(),
            }
        })
        .collect()
}

/// 并行读取四组独立 NoTransaction 投影，并按既有读取顺序选择首个错误。
///
/// 各投影只读取事实，不执行审计或业务写入；等待全部结果后再依序传播错误，
/// 防止最快失败的读取改变付款视图原有错误优先级。
async fn load_payment_projections<A, B, C, D>(
    allocations: impl Future<Output = Result<A>>,
    suppliers: impl Future<Output = Result<B>>,
    receipts: impl Future<Output = Result<C>>,
    recipients: impl Future<Output = Result<D>>,
) -> Result<(A, B, C, D)> {
    let (allocations, suppliers, receipts, recipients) =
        tokio::join!(allocations, suppliers, receipts, recipients);
    Ok((allocations?, suppliers?, receipts?, recipients?))
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
    use super::load_payment_projections;
    use crate::Error;

    /// 独立投影的成功结果保持原装配次序。
    #[tokio::test]
    async fn payment_projections_keep_success_order() {
        let values =
            load_payment_projections(async { Ok(1) }, async { Ok(2) }, async { Ok(3) }, async { Ok(4) })
                .await
                .unwrap();
        assert_eq!(values, (1, 2, 3, 4));
    }

    /// 多个读取同时失败时，始终返回原顺序中的首错。
    #[tokio::test]
    async fn payment_projections_keep_first_error_order() {
        for first_failure in 0..4 {
            let result = load_payment_projections(
                projection_result(0, first_failure),
                projection_result(1, first_failure),
                projection_result(2, first_failure),
                projection_result(3, first_failure),
            )
            .await;
            assert!(matches!(result, Err(Error::NotFound(message)) if message == first_failure.to_string()));
        }
    }

    /// 在给定首错之后让所有投影失败，用于校验实际并行读取编排的错误仲裁。
    async fn projection_result(index: usize, first_failure: usize) -> crate::Result<usize> {
        if index >= first_failure { Err(Error::NotFound(index.to_string())) } else { Ok(index) }
    }
}
