//! 资金范围行视图与关联责任映射。

use std::collections::HashMap;

use erp_core::money::Amount;
use erp_finance::repository::prelude::*;
use erp_finance::repository::{PayableExt, ReceivableExt};
use erp_procurement::repository::PurchaseOrderExt;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::WorkItem;
use persistence_core::Executor;
use serde::Serialize;

use super::authorization::*;
use super::invoice::sales_invoice_allocation_view;
use super::payable_source::source_key;
use super::receipt::receipt_allocation_view;
use crate::Result;

mod payable;
pub use payable::ScopedPayableAccountRow;

/// M07/M08/M09 列表共用的范围分页视图；跨页必须原样回传 `scope_version`。
///
/// 负责人候选不在本视图返回，由销售或采购人员目录按自身权限单独查询。
#[derive(Debug, Clone, Serialize)]
pub struct FundsScopedPage<T> {
    /// 本页已按同一对象映射裁剪的业务行。
    pub items: Vec<T>,
    /// 符合授权与业务条件的总数（截断前计数，超限整体拒绝）。
    pub total: u64,
    /// 同一快照下按负责人分组的匹配份额汇总；与列表复用同一授权条件和金额口径。
    pub summary: FundsSummaryView,
    /// 当前页码。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 跨页与导出必须原样回传的范围版本。
    pub scope_version: String,
    /// RBAC 策略版本。
    pub policy_version: u64,
    /// 组织配置版本。
    pub organization_version: u64,
    /// 授权解析时点。
    pub as_of: String,
    /// 无范围时的空结果原因。
    pub empty_reason: Option<&'static str>,
    /// 面向客户端的范围摘要。
    pub scope_summary: &'static str,
    /// 归属口径说明。
    pub ownership_basis: &'static str,
}

/// M07 应收往来子账范围行：部分授权只返获授权核销份额，整单金额为 null。
#[derive(Debug, Clone, Serialize)]
pub struct ScopedReceivableAccountRow {
    /// 子账主键。
    pub id: String,
    /// 来源销售单。
    pub sales_order_id: String,
    /// 往来子账序号。
    pub account_seq: u32,
    /// 子账状态。
    pub status: erp_finance::entity::receivable::ReceivableAccountStatus,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 获授权核销份额合计（匹配份额求和，60/40 不重复记）。
    pub visible_settled_share: Amount,
    /// 整单含税应收总额；部分授权为 null。
    pub gross_total: Option<Amount>,
    /// 整单已核销合计；部分授权为 null。
    pub settled_total: Option<Amount>,
    /// 未分配余额；按资金单据自身规则判定，部分授权为 null。
    pub open_total: Option<Amount>,
    /// 部分受限时为 true，客户端必须提示权限限制与可见金额。
    pub permission_limited: bool,
    /// 关联销售单当前负责人。
    pub sales_owner_user_id: Option<String>,
    /// 关联销售单当前业务组织。
    pub business_org_unit_id: Option<String>,
    /// 来源销售单业务单号。核销池按单号定位，范围裁剪不得丢掉。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sales_order_no: String,
    /// 经营客户。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub customer_id: String,
    /// 客户名称。缺失时由客户端占位，不回退内部 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub customer_name: Option<String>,
    /// 结算往来主体。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub counterparty_party_id: String,
    /// 结算往来主体法定名称。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counterparty_party_name: Option<String>,
    /// 子账乐观锁版本。
    #[serde(default)]
    pub version: u64,
    /// 剩余可开票额度。部分授权为 null。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_invoiceable_total: Option<Amount>,
    /// 应收分录。回款核销池按增加分录建行。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<erp_finance::dto::receivable::ReceivableEntryView>,
}

/// M07 客户回款范围行：登记/核销经办人分别查询，整单与未分配部分授权为 null。
#[derive(Debug, Clone, Serialize)]
pub struct ScopedCustomerReceiptRow {
    /// 回款单主键。
    pub id: String,
    /// 回款单号。
    pub receipt_no: String,
    /// 回款单状态。
    pub status: erp_finance::entity::receivable::CustomerReceiptStatus,
    /// 实际到账时间（秒级时间戳）。
    pub received_at: erp_core::common::time::Instant,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 结算主体；仅整单读取资格返回，供已有回款继续核销。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counterparty_party_id: Option<String>,
    /// 经营客户；仅整单读取资格返回。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_id: Option<String>,
    /// 原银行引用；仅整单读取资格返回。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bank_reference: Option<String>,
    /// 回款单乐观锁版本；仅整单读取资格返回。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<u64>,
    /// 获授权核销份额合计。
    pub visible_allocated_share: Amount,
    /// 整单到账金额；部分授权为 null。
    pub amount: Option<Amount>,
    /// 整单已核销合计；部分授权为 null。
    pub allocated_total: Option<Amount>,
    /// 未分配余额；部分授权为 null。
    pub unallocated_amount: Option<Amount>,
    /// 获授权分配行；部分授权仅含获授权份额行，其他分配不返回。
    pub allocations: Option<Vec<erp_finance::dto::receivable::ReceiptAllocationView>>,
    /// 部分受限时为 true。
    pub permission_limited: bool,
}

/// M08 销项发票范围行：整单金额与完整凭证部分授权为 null。
#[derive(Debug, Clone, Serialize)]
pub struct ScopedInvoiceRow {
    /// 发票主键。
    pub id: String,
    /// 发票号码。
    pub invoice_no: String,
    /// 发票方向。
    pub invoice_direction: erp_finance::entity::receivable::InvoiceDirection,
    /// 蓝红类型。
    pub invoice_kind: erp_finance::entity::receivable::InvoiceKind,
    /// 发票状态。
    pub status: erp_finance::entity::receivable::InvoiceStatus,
    /// 开票日期。
    pub invoice_date: erp_core::common::time::BusinessDate,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 获授权分配份额合计。
    pub visible_allocated_share: Amount,
    /// 整单含税金额；部分授权为 null。
    pub gross_amount: Option<Amount>,
    /// 整单已分配合计；部分授权为 null。
    pub allocated_total: Option<Amount>,
    /// 未分配余额；部分授权为 null。
    pub unallocated_amount: Option<Amount>,
    /// 获授权分配行；部分授权仅含获授权份额行。
    pub allocations: Option<Vec<erp_finance::dto::receivable::SalesInvoiceAllocationView>>,
    /// 进项方向获授权分配行；销项发票为空，部分授权仅含获授权份额行。
    pub purchase_allocations: Option<Vec<erp_finance::dto::payable::PurchaseInvoiceAllocationView>>,
    /// 部分受限时为 true。
    pub permission_limited: bool,
}

/// M08 开票申请范围行：负责销售/申请人/当前开票处理人分别查询，不改变正式开票准入。
#[derive(Debug, Clone, Serialize)]
pub struct ScopedInvoiceRequestRow {
    /// 申请主键。
    pub id: String,
    /// 申请单号。
    pub request_no: String,
    /// 关联销售单。
    pub sales_order_id: String,
    /// 关联销售单业务单号。
    pub sales_order_no: String,
    /// 申请状态。
    pub status: erp_finance::entity::receivable::InvoiceRequestStatus,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 申请人（创建人）。
    pub applicant_user_id: String,
    /// 当前开票处理人（关联工作项当前负责人）；无工作项为空。
    pub handler_user_id: Option<String>,
    /// 申请金额；部分授权仍返回本行申请金额（行级事实），整单汇总为 null 由汇总视图承担。
    pub amount: Amount,
    /// 部分受限时为 true。
    pub permission_limited: bool,
    /// 关联销售单当前负责人。
    pub sales_owner_user_id: Option<String>,
    /// 关联销售单当前业务组织。
    pub business_org_unit_id: Option<String>,
}

/// M09 供应商付款范围行：采购负责人与付款经办人分别查询。
#[derive(Debug, Clone, Serialize)]
pub struct ScopedSupplierPaymentRow {
    /// 付款单主键。
    pub id: String,
    /// 付款单号。
    pub payment_no: String,
    /// 付款单状态。
    pub status: erp_finance::entity::payable::SupplierPaymentStatus,
    /// 收款供应商。
    pub supplier_id: String,
    /// 实际付款时间（秒级时间戳）。
    pub paid_at: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 获授权核销份额合计。
    pub visible_allocated_share: Amount,
    /// 整单付款金额；部分授权为 null。
    pub amount: Option<Amount>,
    /// 整单已核销合计；部分授权为 null。
    pub allocated_total: Option<Amount>,
    /// 未分配余额；部分授权为 null。
    pub unallocated_amount: Option<Amount>,
    /// 获授权分配行；部分授权仅含获授权份额行。
    pub allocations: Option<Vec<erp_finance::dto::payable::PaymentAllocationView>>,
    /// 部分受限时为 true。
    pub permission_limited: bool,
}

/// M09 进项发票分配范围行：按应付子账来源采购责任过滤。
#[derive(Debug, Clone, Serialize)]
pub struct ScopedPurchaseInvoiceAllocationRow {
    /// 分配主键。
    pub id: String,
    /// 进项发票。
    pub invoice_id: String,
    /// 进项发票号码。
    pub invoice_no: Option<String>,
    /// 应付子账。
    pub payable_account_id: String,
    /// 应付子账真实来源业务单号；采购单号或结算单号，缺失时不回退内部 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_document_no: Option<String>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 获授权分配金额。
    pub visible_allocated_amount: Amount,
    /// 整单分配金额；部分授权为 null（本行即份额时仍返 null，由可见份额承担）。
    pub allocated_gross_amount: Option<Amount>,
    /// 部分受限时为 true。
    pub permission_limited: bool,
}

/// 人员汇总行：只算匹配份额，未分配单列，部分授权仅返获授权份额。
#[derive(Debug, Clone, Serialize)]
pub struct FundsPersonShare {
    /// 负责人。
    pub owner_user_id: String,
    /// 匹配份额合计。
    pub visible_share: Amount,
}

/// 汇总视图：分组份额与未分配余额；整单合计部分授权为 null。
#[derive(Debug, Clone, Serialize)]
pub struct FundsSummaryView {
    /// 按负责人分组的匹配份额。
    pub grouped: Vec<FundsPersonShare>,
    /// 未分配余额（无关联单据份额）。
    pub unassigned: Amount,
    /// 整单合计；部分授权为 null，禁止差额推导。
    pub whole_total: Option<Amount>,
    /// 部分受限时为 true。
    pub permission_limited: bool,
    /// 跨页与导出必须原样回传的范围版本。
    pub scope_version: String,
}

/// 候选行：可区分离线授权候选与无资格对象；不可见对象不返回。
#[derive(Debug, Clone, Serialize)]
pub struct FundsCandidate {
    /// 候选主键。
    pub id: String,
    /// 展示标签。
    pub label: String,
    /// 关联负责人。
    pub owner_user_id: Option<String>,
    /// 部分受限原因；整单可见时为空。
    pub permission_limited_reason: Option<&'static str>,
}
// ===== S3-03 M07/M08/M09 范围查询：关联责任映射与金额裁剪 =====
//
// 同一对象映射供列表、详情、汇总、候选、导出、命令复用；各入口按自身动作
// 独立解析，授权与业务读取沿用调用方同一执行器。

/// 关联销售单的当前责任事实。
#[derive(Debug, Clone)]
pub(super) struct LinkedSalesFact {
    /// 当前负责销售。
    pub(super) owner_user_id: String,
    /// 当前业务组织。
    pub(super) business_org_unit_id: String,
    /// 单据业务版本。
    pub(super) version: u64,
}

/// 关联采购单的当前责任事实；老单缺责任人时负责人为空。
#[derive(Debug, Clone)]
pub(super) struct LinkedPurchaseFact {
    /// 来源当前负责人；采购与结算分别解释。
    pub(super) owner_user_id: Option<String>,
    /// 当前业务组织。
    pub(super) business_org_unit_id: String,
    /// 单据业务版本。
    pub(super) version: u64,
    /// 采购单号。空字符串表示单据没有业务单号。
    pub(super) document_no: String,
}

/// 单据某条分配归属的关联单据；`None` 表示无关联单据的未分配份额。
pub(super) type LinkedOrderId = Option<String>;

/// 回款分配的授权裁剪单元：金额方向、归属订单与响应视图。
pub(super) struct ReceiptLink {
    /// 分配主键。
    pub(super) id: String,
    /// 正反动作后的记账方向金额。
    pub(super) signed: Amount,
    /// 归属销售单；缺失时计入未分配。
    pub(super) order: LinkedOrderId,
    /// 响应视图。
    pub(super) view: erp_finance::dto::receivable::ReceiptAllocationView,
}

/// 销项发票分配的授权裁剪单元。
pub(super) struct SalesInvoiceLink {
    /// 正反动作后的含税方向金额。
    pub(super) signed: Amount,
    /// 归属销售单；缺失时计入未分配。
    pub(super) order: LinkedOrderId,
    /// 响应视图。
    pub(super) view: erp_finance::dto::receivable::SalesInvoiceAllocationView,
}

/// 付款核销的授权裁剪单元。
pub(super) struct PaymentLink {
    /// 正反动作后的记账方向金额。
    pub(super) signed: Amount,
    /// 归属采购单；结算来源使用带类型关联键；缺失来源拒绝。
    pub(super) order: LinkedOrderId,
    /// 响应视图（来源单号不回填，不得当单号展示）。
    pub(super) view: erp_finance::dto::payable::PaymentAllocationView,
}

impl FundsAccess {
    /// 批量读取关联销售单当前责任；缺失单据按无责任处理，调用方跳过该行。
    ///
    /// # 参数
    /// * `ids` - 销售单主键；重复值会去重。
    /// * `executor` - 调用方事务。
    ///
    /// # 返回
    /// 销售单 ID 到当前负责人、业务组织与版本；查不到的单据不进入映射。
    ///
    /// # 错误
    /// 销售单读取失败时返回对应错误。
    pub(super) async fn sales_fact_map(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, LinkedSalesFact>> {
        use erp_core::ids::SalesOrderId;
        let mut map = HashMap::new();
        let unique = crate::support::dedup_sorted(ids.iter().cloned());
        for chunk in unique.chunks(500) {
            let keys = chunk.iter().map(|id| SalesOrderId::new(id.clone())).collect::<Vec<_>>();
            for order in self.db.sales_orders().find_orders_by_ids(&keys, executor).await? {
                map.insert(
                    order.base.id.clone(),
                    LinkedSalesFact {
                        owner_user_id: order.sales_owner_user_id.clone(),
                        business_org_unit_id: order.business_org_unit_id.clone(),
                        version: order.base.version,
                    },
                );
            }
        }
        Ok(map)
    }

    /// 批量读取关联采购单当前责任；责任人缺失的老单保留组织事实。
    ///
    /// # 参数
    /// * `ids` - 采购单主键或带类型的结算关联键。
    /// * `executor` - 调用方事务。
    ///
    /// # 返回
    /// 采购单与结算来源的责任事实；含 `:` 的键不按采购单查询。
    ///
    /// # 错误
    /// 采购单或结算单读取失败时返回对应错误。
    pub(super) async fn purchase_fact_map(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, LinkedPurchaseFact>> {
        let mut map = HashMap::new();
        let unique = crate::support::dedup_sorted(ids.iter().filter(|id| !id.contains(':')).cloned());
        for chunk in unique.chunks(500) {
            let keys = chunk.to_vec();
            for order in self.db.purchase_order().find_orders_by_ids(&keys, executor).await? {
                map.insert(
                    order.base.id.clone(),
                    LinkedPurchaseFact {
                        owner_user_id: order.current_owner_user_id().ok().map(str::to_string),
                        business_org_unit_id: order.business_org_unit_id.clone(),
                        version: order.base.version,
                        document_no: order.purchase_no.clone(),
                    },
                );
            }
        }
        map.extend(self.settlement_fact_map(ids, executor).await?);
        Ok(map)
    }
}

/// 多关联单据行的条件匹配：负责人与组织任一关联命中，操作人按单据事实判定。
///
/// # 参数
/// * `orders` - 关联单据责任元组。
/// * `operators` - 第一经办人。
/// * `secondary` - 第二经办人。
/// * `condition` - 负责人、经办人与组织条件。
///
/// # 返回
/// 已提供的经办条件都命中，且在提供了负责人或组织时至少一条关联同时命中时返回 true。
///
/// # 错误
/// 不返回错误。
pub(super) fn matches_multi_condition(
    orders: &[LinkedOrderRow],
    operators: &[String],
    secondary: &[String],
    condition: &FundsLinkedCondition,
) -> bool {
    if let Some(wanted) = &condition.operator_user_ids
        && !operators.iter().any(|id| wanted.iter().any(|item| item == id))
    {
        return false;
    }
    if let Some(wanted) = &condition.secondary_operator_user_ids
        && !secondary.iter().any(|id| wanted.iter().any(|item| item == id))
    {
        return false;
    }
    if condition.owner_user_ids.is_none() && condition.org_unit_ids.is_none() {
        return true;
    }
    orders.iter().any(|(linked, owner, org, _, _)| {
        linked.is_some()
            && condition
                .owner_user_ids
                .as_ref()
                .is_none_or(|wanted| owner.as_ref().is_some_and(|id| wanted.iter().any(|item| item == id)))
            && condition
                .org_unit_ids
                .as_ref()
                .is_none_or(|wanted| org.as_ref().is_some_and(|id| wanted.iter().any(|item| item == id)))
    })
}

use erp_finance::entity::payable::AllocationAction as PayableAllocationAction;
use erp_finance::entity::receivable::AllocationAction as ReceivableAllocationAction;

impl FundsAccess {
    /// 回款分配按核销分录反查所属子账与销售单；金额按正反动作记方向。
    ///
    /// # 参数
    /// * `receipt_ids` - 客户回款主键。
    /// * `executor` - 调用方事务。
    ///
    /// # 返回
    /// 回款 ID 到分配关联；冲正金额为负，找不到销售单时 `order` 为 `None`。
    ///
    /// # 错误
    /// 分配、分录或子账读取失败时返回对应错误。
    pub(super) async fn receipt_matched_links(
        &self,
        receipt_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<ReceiptLink>>> {
        use erp_core::ids::{CustomerReceiptId, ReceivableEntryId};
        let keys = receipt_ids.iter().map(|id| CustomerReceiptId::new(id.clone())).collect::<Vec<_>>();
        let allocations = self.db.receipt_allocations().find_allocations_by_receipts(&keys, executor).await?;
        let entry_keys = allocations
            .iter()
            .map(|item| ReceivableEntryId::new(item.receivable_entry_id.to_string()))
            .collect::<Vec<_>>();
        let entries = self.db.receivable_entries().find_entries_by_ids(&entry_keys, executor).await?;
        let entry_account = entries
            .into_iter()
            .map(|entry| (entry.base.id.clone(), entry.receivable_account_id.to_string()))
            .collect::<HashMap<_, _>>();
        let account_keys = entry_account.values().cloned().collect::<Vec<_>>();
        let accounts = self.db.receivable_accounts().find_accounts_by_ids(&account_keys, executor).await?;
        let account_order = accounts
            .into_iter()
            .map(|account| (account.base.id.clone(), account.sales_order_id.to_string()))
            .collect::<HashMap<_, _>>();
        let mut links: HashMap<String, Vec<ReceiptLink>> = HashMap::new();
        for item in allocations {
            let signed = match item.allocation_action {
                ReceivableAllocationAction::Apply => item.allocated_amount,
                ReceivableAllocationAction::Reverse => zero_amount().checked_sub(item.allocated_amount),
            };
            let order = entry_account
                .get(&item.receivable_entry_id.to_string())
                .and_then(|account| account_order.get(account).cloned());
            let view = receipt_allocation_view(&item);
            let id = item.base.id.clone();
            let receipt = item.customer_receipt_id.to_string();
            links.entry(receipt).or_default().push(ReceiptLink { id, signed, order, view });
        }
        Ok(links)
    }

    /// 销项发票分配按子账反查销售单；含税金额按正反动作记方向。
    ///
    /// # 参数
    /// * `invoice_ids` - 发票主键。
    /// * `executor` - 调用方事务。
    ///
    /// # 返回
    /// 发票 ID 到销项分配关联；冲正金额为负。
    ///
    /// # 错误
    /// 分配或应收子账读取失败时返回对应错误。
    pub(super) async fn sales_invoice_matched_links(
        &self,
        invoice_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<SalesInvoiceLink>>> {
        use erp_core::ids::InvoiceId;
        let keys = invoice_ids.iter().map(|id| InvoiceId::new(id.clone())).collect::<Vec<_>>();
        let allocations =
            self.db.sales_invoice_allocations().find_allocations_by_invoices(&keys, executor).await?;
        let account_keys =
            allocations.iter().map(|item| item.receivable_account_id.to_string()).collect::<Vec<_>>();
        let accounts = self.db.receivable_accounts().find_accounts_by_ids(&account_keys, executor).await?;
        let account_order = accounts
            .into_iter()
            .map(|account| (account.base.id.clone(), account.sales_order_id.to_string()))
            .collect::<HashMap<_, _>>();
        let mut links: HashMap<String, Vec<SalesInvoiceLink>> = HashMap::new();
        for item in allocations {
            let signed = match item.allocation_action {
                ReceivableAllocationAction::Apply => item.allocated_gross_amount,
                ReceivableAllocationAction::Reverse => zero_amount().checked_sub(item.allocated_gross_amount),
            };
            let order = account_order.get(&item.receivable_account_id.to_string()).cloned();
            let view = sales_invoice_allocation_view(&item);
            let invoice = item.invoice_id.to_string();
            links.entry(invoice).or_default().push(SalesInvoiceLink { signed, order, view });
        }
        Ok(links)
    }

    /// 付款核销按应付分录反查子账与采购单；结算来源保留自身责任边界。
    ///
    /// # 参数
    /// * `payment_ids` - 供应商付款主键。
    /// * `executor` - 调用方事务。
    ///
    /// # 返回
    /// 付款 ID 到核销关联；来源键带真实来源类型，冲正金额为负。
    ///
    /// # 错误
    /// 分配、应付分录或子账读取失败时返回对应错误。
    pub(super) async fn payment_matched_links(
        &self,
        payment_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<PaymentLink>>> {
        use erp_core::ids::{PayableAccountId, PayableEntryId, SupplierPaymentId};
        let keys = payment_ids.iter().map(|id| SupplierPaymentId::new(id.clone())).collect::<Vec<_>>();
        let allocations = self.db.payment_allocations().find_allocations_by_payments(&keys, executor).await?;
        let entry_keys = allocations
            .iter()
            .map(|item| PayableEntryId::new(item.payable_entry_id.to_string()))
            .collect::<Vec<_>>();
        let entries = self.db.payable_entries().find_entries_by_ids(&entry_keys, executor).await?;
        let entry_account = entries
            .into_iter()
            .map(|entry| (entry.base.id.clone(), entry.payable_account_id.to_string()))
            .collect::<HashMap<_, _>>();
        let account_keys =
            entry_account.values().map(|id| PayableAccountId::new(id.clone())).collect::<Vec<_>>();
        let accounts = self.db.payable_accounts().find_accounts_by_ids(&account_keys, executor).await?;
        let account_order = accounts
            .into_iter()
            .map(|account| {
                let order = Some(source_key(account.source_type, &account.source_document_id));
                (account.base.id.clone(), order)
            })
            .collect::<HashMap<_, _>>();
        let mut links: HashMap<String, Vec<PaymentLink>> = HashMap::new();
        for item in allocations {
            let signed = match item.allocation_action {
                PayableAllocationAction::Apply => item.allocated_amount,
                PayableAllocationAction::Reverse => zero_amount().checked_sub(item.allocated_amount),
            };
            let order = entry_account
                .get(&item.payable_entry_id.to_string())
                .and_then(|account| account_order.get(account).cloned())
                .flatten();
            let view = erp_finance::dto::payable::PaymentAllocationView::from(&item);
            let payment = item.supplier_payment_id.to_string();
            links.entry(payment).or_default().push(PaymentLink { signed, order, view });
        }
        Ok(links)
    }

    /// 工作项当前处理人；已关闭任务回退完成人，缺失任务按无处理人。
    ///
    /// # 参数
    /// * `work_item_ids` - 工作项主键；空 ID 会被去掉。
    /// * `executor` - 调用方事务。
    ///
    /// # 返回
    /// 工作项 ID 到处理人；优先负责人，否则完成人。查不到的任务不进入映射。
    ///
    /// # 错误
    /// 工作项读取失败时返回对应错误。
    pub(super) async fn work_item_handlers(
        &self,
        work_item_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let mut handlers = HashMap::new();
        let unique = handler_work_item_ids(work_item_ids);
        for chunk in unique.chunks(500) {
            let items = self.db.work_items().list_active_by_ids(chunk, executor).await?;
            handlers.extend(handlers_from_work_items(items));
        }
        Ok(handlers)
    }
}

/// 工作项读取键去重并去掉空 ID，空结果不会产生批次或数据库读取。
fn handler_work_item_ids(ids: &[String]) -> Vec<String> {
    crate::support::dedup_sorted(ids.iter().filter(|id| !id.is_empty()).cloned())
}

/// 已装载工作项映射为处理人；负责人优先，缺负责人时使用完成人，不按状态裁剪。
fn handlers_from_work_items(items: Vec<WorkItem>) -> HashMap<String, String> {
    items
        .into_iter()
        .filter_map(|item| item.owner_user_id.or(item.completed_by).map(|handler| (item.base.id, handler)))
        .collect()
}

/// 分配金额求和的零值；与应收映射保持同一零金额口径。
///
/// # 参数
/// 无。
///
/// # 返回
/// 应收映射使用的零金额。
///
/// # 错误
/// 不返回错误。
pub(super) fn zero_amount() -> Amount {
    erp_finance::service::receivable::mapping::zero_amount()
}

/// 同一快照的匹配份额汇总；未分配单列，整单合计部分授权为 null。
///
/// # 参数
/// * `matched` - 分配主键、方向金额与来源单。
/// * `owner_of` - 来源单到负责人。
/// * `whole_total` - 整单合计；部分授权时由调用方传入 `None`。
/// * `scope_version` - 本次范围版本。
/// * `permission_limited` - 为 true 时丢掉没有来源单的份额。
///
/// # 返回
/// 按负责人排序的可见份额、未分配金额和整单合计。
///
/// # 错误
/// 不返回错误。金额归并始终成功。
pub(super) fn build_summary(
    matched: &[(String, Amount, LinkedOrderId)],
    owner_of: &HashMap<String, String>,
    whole_total: Option<Amount>,
    scope_version: &str,
    permission_limited: bool,
) -> Result<FundsSummaryView> {
    let visible = matched
        .iter()
        .filter(|(_, _, order)| !permission_limited || order.is_some())
        .cloned()
        .collect::<Vec<_>>();
    let (grouped, unassigned) = summarize_matched_shares(&visible, |order| owner_of.get(order).cloned())?;
    let mut grouped = grouped
        .into_iter()
        .map(|(owner_user_id, visible_share)| FundsPersonShare { owner_user_id, visible_share })
        .collect::<Vec<_>>();
    grouped.sort_by(|left, right| left.owner_user_id.cmp(&right.owner_user_id));
    Ok(FundsSummaryView {
        grouped,
        unassigned,
        whole_total,
        permission_limited,
        scope_version: scope_version.to_string(),
    })
}

/// 单据行关联责任元组：关联单据、负责人、组织、事实主键与版本。
pub(super) type OrderTuple = (LinkedOrderId, Option<String>, Option<String>, String, u64);

/// 关联责任行：字段顺序与 [`OrderTuple`] 相同。
pub(super) type LinkedOrderRow = OrderTuple;

#[cfg(test)]
mod tests {
    use erp_core::common::time::BusinessDate;
    use erp_finance::entity::payable::{PayableAccountStatus, PayableSourceType};
    use erp_finance::entity::receivable::{
        InvoiceDirection, InvoiceKind, InvoiceRequestStatus, InvoiceStatus, ReceivableAccountStatus,
    };

    use super::*;

    fn summary(whole: bool) -> FundsSummaryView {
        FundsSummaryView {
            grouped: vec![FundsPersonShare { owner_user_id: "owner-1".into(), visible_share: zero_amount() }],
            unassigned: zero_amount(),
            whole_total: whole.then(zero_amount),
            permission_limited: !whole,
            scope_version: "scope-v1".into(),
        }
    }

    fn page<T>(items: Vec<T>, basis: &'static str, whole: bool) -> FundsScopedPage<T> {
        FundsScopedPage {
            items,
            total: 1,
            summary: summary(whole),
            page: 2,
            page_size: 20,
            scope_version: "scope-v1".into(),
            policy_version: 3,
            organization_version: 4,
            as_of: "2026-09-23T00:00:00Z".into(),
            empty_reason: None,
            scope_summary: "范围摘要",
            ownership_basis: basis,
        }
    }

    fn assert_page_keeps_scope_without_candidates(json: &serde_json::Value, basis: &str, whole: bool) {
        assert!(json.get("owner_options").is_none());
        assert_eq!(json["total"], 1);
        assert_eq!(json["page"], 2);
        assert_eq!(json["page_size"], 20);
        assert_eq!(json["scope_version"], "scope-v1");
        assert_eq!(json["ownership_basis"], basis);
        assert_eq!(json["summary"]["grouped"][0]["owner_user_id"], "owner-1");
        assert_eq!(json["summary"]["permission_limited"], !whole);
        assert_eq!(json["summary"]["whole_total"].is_null(), !whole);
    }

    /// 资金列表不携带负责人候选；行上的负责人、经办与发票方向仍是业务事实。
    #[test]
    fn funds_pages_omit_owner_candidates_and_keep_row_owner_facts() {
        let receivable = page(
            vec![ScopedReceivableAccountRow {
                id: "ra-1".into(),
                sales_order_id: "so-1".into(),
                account_seq: 1,
                status: ReceivableAccountStatus::Open,
                created_at: 1,
                visible_settled_share: zero_amount(),
                gross_total: Some(zero_amount()),
                settled_total: Some(zero_amount()),
                open_total: Some(zero_amount()),
                permission_limited: false,
                sales_owner_user_id: Some("sales-1".into()),
                business_org_unit_id: Some("org-1".into()),
                sales_order_no: "XS-1".into(),
                customer_id: "cust-1".into(),
                customer_name: None,
                counterparty_party_id: "party-1".into(),
                counterparty_party_name: Some("结算客户".into()),
                version: 1,
                open_invoiceable_total: Some(zero_amount()),
                entries: Vec::new(),
            }],
            "current_sales_owner_and_register_operator",
            true,
        );
        let receivable_json = serde_json::to_value(&receivable).unwrap();
        assert_page_keeps_scope_without_candidates(
            &receivable_json,
            "current_sales_owner_and_register_operator",
            true,
        );
        assert_eq!(receivable_json["items"][0]["sales_owner_user_id"], "sales-1");
        assert_eq!(receivable_json["items"][0]["sales_order_no"], "XS-1");
        assert_eq!(receivable_json["items"][0]["counterparty_party_name"], "结算客户");
        assert!(receivable_json["items"][0].get("customer_name").is_none());
        assert!(receivable_json["items"][0].get("owner_user_name").is_none());

        let payable = page(
            vec![ScopedPayableAccountRow {
                id: "pa-1".into(),
                source_document_id: "po-1".into(),
                source_type: PayableSourceType::PurchaseOrder,
                supplier_id: "sup-1".into(),
                status: PayableAccountStatus::Open,
                created_at: 1,
                visible_settled_share: zero_amount(),
                gross_total: None,
                settled_total: None,
                open_total: None,
                open_invoiceable_total: None,
                invoiced_total: None,
                permission_limited: true,
                procurement_owner_user_id: Some("buyer-1".into()),
                business_org_unit_id: Some("org-2".into()),
                payment_recipient: None,
                payment_guidance: None,
                entries: Vec::new(),
                source_document_no: Some("PO-1".into()),
                supplier_name: Some("示例供应商".into()),
            }],
            "linked_purchase_owner",
            false,
        );
        let payable_json = serde_json::to_value(&payable).unwrap();
        assert_page_keeps_scope_without_candidates(&payable_json, "linked_purchase_owner", false);
        assert_eq!(payable_json["items"][0]["procurement_owner_user_id"], "buyer-1");
        assert_eq!(payable_json["items"][0]["source_document_no"], "PO-1");
        assert_eq!(payable_json["items"][0]["supplier_name"], "示例供应商");
        assert!(payable_json["items"][0]["gross_total"].is_null());
        assert!(payable_json["items"][0]["open_invoiceable_total"].is_null());

        let request = page(
            vec![ScopedInvoiceRequestRow {
                id: "irq-1".into(),
                request_no: "IRQ-1".into(),
                sales_order_id: "so-1".into(),
                sales_order_no: "SO-1".into(),
                status: InvoiceRequestStatus::Draft,
                created_at: 1,
                applicant_user_id: "applicant-1".into(),
                handler_user_id: Some("handler-1".into()),
                amount: zero_amount(),
                permission_limited: false,
                sales_owner_user_id: Some("sales-1".into()),
                business_org_unit_id: Some("org-1".into()),
            }],
            "linked_sales_owner_applicant_and_handler",
            true,
        );
        let request_json = serde_json::to_value(&request).unwrap();
        assert_page_keeps_scope_without_candidates(
            &request_json,
            "linked_sales_owner_applicant_and_handler",
            true,
        );
        assert_eq!(request_json["items"][0]["sales_owner_user_id"], "sales-1");
        assert_eq!(request_json["items"][0]["applicant_user_id"], "applicant-1");
        assert_eq!(request_json["items"][0]["handler_user_id"], "handler-1");

        for direction in [InvoiceDirection::Sales, InvoiceDirection::Purchase] {
            let invoice = page(
                vec![ScopedInvoiceRow {
                    id: "inv-1".into(),
                    invoice_no: "FP-1".into(),
                    invoice_direction: direction,
                    invoice_kind: InvoiceKind::Blue,
                    status: InvoiceStatus::Registered,
                    invoice_date: BusinessDate::from_ymd(2026, 9, 23).unwrap(),
                    created_at: 1,
                    visible_allocated_share: zero_amount(),
                    gross_amount: Some(zero_amount()),
                    allocated_total: Some(zero_amount()),
                    unallocated_amount: Some(zero_amount()),
                    allocations: Some(Vec::new()),
                    purchase_allocations: Some(Vec::new()),
                    permission_limited: false,
                }],
                "linked_sales_and_purchase_owner_and_register_operator",
                true,
            );
            let invoice_json = serde_json::to_value(&invoice).unwrap();
            assert_page_keeps_scope_without_candidates(
                &invoice_json,
                "linked_sales_and_purchase_owner_and_register_operator",
                true,
            );
            assert_eq!(invoice_json["items"][0]["invoice_direction"], direction.as_str());
            assert!(invoice_json["items"][0].get("sales_owner_user_id").is_none());
            assert!(invoice_json["items"][0].get("procurement_owner_user_id").is_none());
        }
    }
}

/// 已存在的分配必须指向存在的来源；真正零分配单据单独由整账职责授权。
///
/// # 参数
/// * `sources` - 分配上的来源主键；`None` 表示来源缺失。
/// * `facts` - 已装载的来源事实。
///
/// # 返回
/// 每个来源都非空且存在于 `facts` 时返回 true；空迭代器返回 true。
///
/// # 错误
/// 不返回错误。
pub(super) fn linked_sources_exist<'a, T>(
    mut sources: impl Iterator<Item = Option<&'a str>>,
    facts: &HashMap<String, T>,
) -> bool {
    sources.all(|source| source.is_some_and(|id| facts.contains_key(id)))
}

#[cfg(test)]
mod source_integrity_tests {
    use super::*;

    #[test]
    fn unallocated_is_distinct_from_dangling_allocation() {
        let facts = [("known".into(), ())].into();
        assert!(linked_sources_exist(std::iter::empty(), &facts));
        assert!(linked_sources_exist([Some("known")].into_iter(), &facts));
        assert!(!linked_sources_exist([Some("missing")].into_iter(), &facts));
        assert!(!linked_sources_exist([Some("known"), None].into_iter(), &facts));
    }
}

#[cfg(test)]
mod work_item_handler_tests {
    use erp_core::ids::WorkItemId;
    use erp_workflow::entity::work_item::{
        AssignmentSource, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType,
    };

    use super::*;

    /// 构造已装载的工作项事实，终态仅调整与映射有关的公共字段。
    fn item(id: &str, owner: Option<&str>, completed: Option<&str>, status: WorkItemStatus) -> WorkItem {
        let mut item = WorkItem::new_with_responsibility_key(
            WorkItemId::new(id),
            WorkItemData {
                work_item_type: WorkItemType::SalesInvoiceExecution,
                business_object_type: "receivable_account".into(),
                business_object_id: "account".into(),
                subject_version: "1".into(),
                owner_role: "invoice".into(),
                owner_organization_id: "finance".into(),
                owner_user_id: "initial".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            "receivable_account:account",
        )
        .unwrap();
        item.owner_user_id = owner.map(str::to_string);
        item.completed_by = completed.map(str::to_string);
        item.status = status;
        item
    }

    /// 读取键过滤保持空 ID 不查、重复去重及稳定批次边界。
    #[test]
    fn work_item_handler_ids_remove_empty_and_deduplicate_before_batching() {
        assert!(handler_work_item_ids(&[String::new(), String::new()]).is_empty());
        assert_eq!(handler_work_item_ids(&["b".into(), "".into(), "a".into(), "b".into()]), ["a", "b"]);
        let mut ids = (0..501).map(|index| format!("task-{index:03}")).collect::<Vec<_>>();
        ids.push(ids[0].clone());
        ids.push(String::new());
        let unique = handler_work_item_ids(&ids);
        assert_eq!(unique.chunks(500).map(<[String]>::len).collect::<Vec<_>>(), [500, 1]);
    }

    /// 终态不改变负责人优先规则，缺失任务或无责任事实保持无处理人。
    #[test]
    fn work_item_handlers_keep_owner_priority_completed_fallback_and_missing_absence() {
        let handlers = handlers_from_work_items(vec![
            item("owner", Some("current"), Some("previous"), WorkItemStatus::Completed),
            item("fallback", None, Some("finisher"), WorkItemStatus::Closed),
            item("empty-owner", Some(""), Some("finisher"), WorkItemStatus::Closed),
            item("no-handler", None, None, WorkItemStatus::Open),
        ]);
        assert_eq!(handlers["owner"], "current");
        assert_eq!(handlers["fallback"], "finisher");
        assert_eq!(handlers["empty-owner"], "");
        assert!(!handlers.contains_key("no-handler"));
        assert!(!handlers.contains_key("missing-task"));
        assert!(handlers_from_work_items(Vec::new()).is_empty());
    }
}
