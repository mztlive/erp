//! 销项发票原子提交命令的 DTO 归一化与类型化准备（FIN-E08）。
//!
//! 将 `CommitInvoiceRequest` 的 `new` / `existing` 互斥形态、
//! `expected_version` 正数约束、销项发票类型校验与分配行转换收敛到 DTO
//! 自身，使非法字段组合在进入事务前即可拒绝；`SalesInvoiceAllocationPlan`
//! 随后完成总额／税额三口径守恒、序号与实体构造，`Service` 仅负责
//! 存在性、版本锁、事务编排与批量数据面，不再复制字段组合判断与总额
//! 守恒计算。

use entities::receivable::sales_invoice_allocation_plan::SalesInvoiceAllocationLine;
use entities::receivable::{Invoice, InvoiceDirection};

use crate::errors::{Error, Result};

use super::dto::{CommitInvoiceRequest, CreateInvoiceRequest, SalesInvoiceAllocationLineRequest};

/// 校验发票方向为销项（FIN-E08 共享销售专用守卫）。
///
/// 两条入口（`commit_invoice` 的 `Existing` 与 `post_invoice`）在装载
/// 持久化发票后必须共用本校验，保证非销项发票以同一错误码 fail-closed，
/// 文案与 `CommitInvoiceRequest::prepare` 的新建分支保持一致。
///
/// # 参数
/// * `invoice` - 已从仓储装载的发票实体
///
/// # 返回
/// 方向为 `Sales` 时返回 `Ok(())`。
///
/// # 错误
/// * `ValidationError` - 发票方向非销项时返回 `应收登记命令只接受销项发票`
///
/// # 约束
/// 纯内存校验，不触及 I/O、时钟或事务。
pub(crate) fn ensure_sales_invoice(invoice: &Invoice) -> Result<()> {
    if invoice.invoice_direction != InvoiceDirection::Sales {
        return Err(Error::ValidationError("应收登记命令只接受销项发票".to_string()));
    }
    Ok(())
}

/// 已验证的销项发票提交命令。
///
/// 由 `CommitInvoiceRequest::prepare` 产生，保证互斥形态、销项类型与
/// 分配行均已校验；调用方无需再次判断字段组合，`Service` 仅需按变体
/// 分别处理新建或已有草稿的持久化与版本校验。
#[derive(Debug, Clone)]
pub enum PreparedInvoiceCommit {
    /// 新建发票：携带完整创建字段与已验证的分配行。
    New {
        /// 新发票创建字段（已通过 `Validate`，但实体层仍会二次归一化）。
        invoice: CreateInvoiceRequest,
        /// 已验证的待过账分配（顺序与请求一致，金额形态已校验）。
        allocations: Vec<SalesInvoiceAllocationLine>,
    },
    /// 提交已有草稿：携带草稿主键、乐观锁版本与已验证分配。
    Existing {
        /// 已有草稿主键。
        invoice_id: String,
        /// 已有草稿期望版本（>0）。
        expected_version: u64,
        /// 已验证的待过账分配。
        allocations: Vec<SalesInvoiceAllocationLine>,
    },
}

/// 将销项发票分配请求行转换为领域计划输入行。
///
/// # 参数
/// * `lines` - 请求中的销项分配行（含税/不含税/税额三元组）
///
/// # 返回
/// 全部行均合法时返回与输入顺序一致的 `SalesInvoiceAllocationLine` 集合；
/// 调用方随后交由 `SalesInvoiceAllocationPlan` 完成守恒与序号校验。
///
/// # 错误
/// 不返回错误；金额正数与恒等校验由 `SalesInvoiceAllocationPlan` 统一完成，
/// 本函数仅做结构映射，保持 DTO 职责的单一性。
///
/// # 约束
/// 纯转换，不触及 I/O、时钟或 ID 生成；顺序与输入一致，保证调用方可
/// 以确定性复现首错语义与序号分配。
pub(crate) fn convert_invoice_allocations(
    lines: &[SalesInvoiceAllocationLineRequest],
) -> Vec<SalesInvoiceAllocationLine> {
    lines
        .iter()
        .map(|line| SalesInvoiceAllocationLine {
            receivable_account_id: line.receivable_account_id.clone(),
            allocated_gross_amount: line.allocated_gross_amount,
            allocated_net_amount: line.allocated_net_amount,
            allocated_tax_amount: line.allocated_tax_amount,
        })
        .collect()
}

impl CommitInvoiceRequest {
    /// 归一化并验证销项发票原子提交命令。
    ///
    /// 判定 `invoice` / `invoice_id + expected_version` 的互斥形态，
    /// 校验 `expected_version` 为正数、销项发票类型与分配行结构，
    /// 并将 `allocations` 转换为强类型的计划输入行集合。
    ///
    /// # 参数
    /// * `&self` - 原始提交请求（`invoice_id` / `expected_version` / `invoice` / `allocations`）
    ///
    /// # 返回
    /// 合法时返回 `PreparedInvoiceCommit::New` 或 `::Existing`，
    /// 携带已验证的分配集合；非法组合在进入事务前即失败。
    ///
    /// # 错误
    /// * `ValidationError` - `new` 与 `existing` 字段同时提供、同时缺失、
    ///   `expected_version` 为 `None` 或 `0`、非销项发票类型、`allocations` 为空
    ///   时返回
    ///
    /// # 约束
    /// * I/O-free：不查询数据库、不触及全局时钟/ID 生成器
    /// * 确定性：同输入必得同输出或同首错，分配顺序与输入一致
    /// * 兼容性：错误文案保持与原 `commit_invoice` 分支一致，
    ///   以免改变前端对非法形态与非销项类型的展示
    pub fn prepare(&self) -> Result<PreparedInvoiceCommit> {
        if self.allocations.is_empty() {
            return Err(Error::ValidationError("至少提供一条发票分配".to_string()));
        }
        let allocations = convert_invoice_allocations(&self.allocations);
        match (&self.invoice_id, self.expected_version, &self.invoice) {
            (None, None, Some(invoice)) => {
                if invoice.invoice_direction != InvoiceDirection::Sales {
                    return Err(Error::ValidationError("应收登记命令只接受销项发票".to_string()));
                }
                Ok(PreparedInvoiceCommit::New {
                    invoice: invoice.clone(),
                    allocations,
                })
            }
            (Some(invoice_id), Some(version), None) if version > 0 => Ok(PreparedInvoiceCommit::Existing {
                invoice_id: invoice_id.clone(),
                expected_version: version,
                allocations,
            }),
            _ => Err(Error::ValidationError(
                "新发票必须提交 invoice；已有草稿必须提交 invoice_id 与 expected_version".to_string(),
            )),
        }
    }
}

/// 将 `PostInvoiceRequest` 的分配行转换为领域计划输入行，供 `post_invoice`
/// 与 `commit_invoice` 复用同一 `SalesInvoiceAllocationPlan`。
///
/// # 参数
/// * `lines` - `PostInvoiceRequest.allocations` 集合
///
/// # 返回
/// 返回与输入顺序一致的 `SalesInvoiceAllocationLine` 集合。
///
/// # 错误
/// 不返回错误；总量守恒与单行恒等由 `Plan` 完成，转换本身只映射结构。
///
/// # 约束
/// 纯转换，不触及 I/O；顺序确定性与 `commit` 路径一致，保证两条入口对
/// 同一 `facts` 产出完全相同 `plan`（FIN-E08 关闭验收）。
pub(crate) fn convert_post_allocations(
    lines: &[SalesInvoiceAllocationLineRequest],
) -> Vec<SalesInvoiceAllocationLine> {
    convert_invoice_allocations(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use entities::common::time::BusinessDate;
    use entities::ids::{PartyId, ReceivableAccountId};
    use entities::money::Amount;
    use std::str::FromStr;

    fn valid_invoice() -> CreateInvoiceRequest {
        CreateInvoiceRequest {
            invoice_direction: InvoiceDirection::Sales,
            invoice_kind: entities::receivable::InvoiceKind::Blue,
            party_id: PartyId::new("party-1"),
            invoice_code: None,
            invoice_no: "INV-001".to_string(),
            invoice_date: BusinessDate::from_ymd(2026, 8, 6).unwrap(),
            gross_amount: Amount::from_str("100.00").unwrap(),
            net_amount: Amount::from_str("88.00").unwrap(),
            tax_amount: Amount::from_str("12.00").unwrap(),
            rounding_adjustment_amount: None,
            rounding_reason: None,
        }
    }

    fn alloc(gross: &str, net: &str, tax: &str) -> SalesInvoiceAllocationLineRequest {
        SalesInvoiceAllocationLineRequest {
            receivable_account_id: ReceivableAccountId::new("ra-1"),
            allocated_gross_amount: Amount::from_str(gross).unwrap(),
            allocated_net_amount: Amount::from_str(net).unwrap(),
            allocated_tax_amount: Amount::from_str(tax).unwrap(),
        }
    }

    fn base_request() -> CommitInvoiceRequest {
        use entities::ids::WorkItemId;
        CommitInvoiceRequest {
            work_item_id: WorkItemId::new("wi-1"),
            expected_task_version: "1".to_string(),
            invoice_id: None,
            expected_version: None,
            invoice: None,
            allocations: vec![alloc("100.00", "88.00", "12.00")],
            idempotency_key: "idem-1".to_string(),
        }
    }

    #[test]
    fn prepare_accepts_new_with_invoice_and_allocations() {
        let req = CommitInvoiceRequest {
            invoice: Some(valid_invoice()),
            ..base_request()
        };
        let prepared = req.prepare().expect("new 形态必须通过");
        match prepared {
            PreparedInvoiceCommit::New { invoice, allocations } => {
                assert_eq!(invoice.invoice_no, "INV-001");
                assert_eq!(allocations.len(), 1);
                assert_eq!(allocations[0].allocated_gross_amount.to_string(), "100.00");
            }
            _ => panic!("应为 New"),
        }
    }

    #[test]
    fn prepare_accepts_existing_with_id_and_version() {
        let req = CommitInvoiceRequest {
            invoice_id: Some("inv-1".to_string()),
            expected_version: Some(2),
            invoice: None,
            ..base_request()
        };
        let prepared = req.prepare().expect("existing 形态必须通过");
        match prepared {
            PreparedInvoiceCommit::Existing {
                invoice_id,
                expected_version,
                allocations,
            } => {
                assert_eq!(invoice_id, "inv-1");
                assert_eq!(expected_version, 2);
                assert_eq!(allocations.len(), 1);
            }
            _ => panic!("应为 Existing"),
        }
    }

    #[test]
    fn prepare_rejects_both_provided() {
        let req = CommitInvoiceRequest {
            invoice_id: Some("inv-1".to_string()),
            expected_version: Some(1),
            invoice: Some(valid_invoice()),
            ..base_request()
        };
        let err = req.prepare().unwrap_err();
        assert!(matches!(err, Error::ValidationError(_)));
        assert!(err
            .to_string()
            .contains("新发票必须提交 invoice；已有草稿必须提交 invoice_id 与 expected_version"));
    }

    #[test]
    fn prepare_rejects_both_missing() {
        let req = CommitInvoiceRequest {
            invoice_id: None,
            expected_version: None,
            invoice: None,
            ..base_request()
        };
        let err = req.prepare().unwrap_err();
        assert!(matches!(err, Error::ValidationError(_)));
    }

    #[test]
    fn prepare_rejects_version_zero() {
        let req = CommitInvoiceRequest {
            invoice_id: Some("inv-1".to_string()),
            expected_version: Some(0),
            invoice: None,
            ..base_request()
        };
        let err = req.prepare().unwrap_err();
        assert!(matches!(err, Error::ValidationError(_)));
        assert!(err
            .to_string()
            .contains("新发票必须提交 invoice；已有草稿必须提交 invoice_id 与 expected_version"));
    }

    #[test]
    fn prepare_rejects_version_missing() {
        let req = CommitInvoiceRequest {
            invoice_id: Some("inv-1".to_string()),
            expected_version: None,
            invoice: None,
            ..base_request()
        };
        let err = req.prepare().unwrap_err();
        assert!(matches!(err, Error::ValidationError(_)));
    }

    #[test]
    fn prepare_rejects_non_sales_invoice() {
        let mut invoice = valid_invoice();
        invoice.invoice_direction = InvoiceDirection::Purchase;
        let req = CommitInvoiceRequest {
            invoice: Some(invoice),
            ..base_request()
        };
        let err = req.prepare().unwrap_err();
        assert!(matches!(err, Error::ValidationError(_)));
        assert!(err.to_string().contains("应收登记命令只接受销项发票"));
    }

    #[test]
    fn prepare_rejects_empty_allocations() {
        let req = CommitInvoiceRequest {
            invoice: Some(valid_invoice()),
            allocations: vec![],
            ..base_request()
        };
        let err = req.prepare().unwrap_err();
        assert!(matches!(err, Error::ValidationError(_)));
        assert!(err.to_string().contains("至少提供一条发票分配"));
    }

    #[test]
    fn prepare_keeps_error_message_compat_for_illegal_combo() {
        let req = CommitInvoiceRequest {
            invoice_id: Some("inv-1".to_string()),
            expected_version: Some(1),
            invoice: Some(valid_invoice()),
            allocations: vec![alloc("100.00", "88.00", "12.00")],
            ..base_request()
        };
        let err = req.prepare().unwrap_err();
        assert!(err
            .to_string()
            .contains("新发票必须提交 invoice；已有草稿必须提交 invoice_id 与 expected_version"));
    }

    #[test]
    fn prepare_keeps_allocation_order_deterministic() {
        let req = CommitInvoiceRequest {
            invoice: Some(valid_invoice()),
            allocations: vec![alloc("60.00", "52.80", "7.20"), alloc("40.00", "35.20", "4.80")],
            ..base_request()
        };
        // Adjust invoice totals to match sum for this deterministic test should not be checked here; prepare only checks shape
        // But we set invoice gross 100, so prepare will succeed
        let prepared = req.prepare().unwrap();
        match prepared {
            PreparedInvoiceCommit::New { allocations, .. } => {
                assert_eq!(allocations[0].allocated_gross_amount.to_string(), "60.00");
                assert_eq!(allocations[1].allocated_gross_amount.to_string(), "40.00");
            }
            _ => panic!("should be New"),
        }
    }

    #[test]
    fn legal_new_command_serializes_stably() {
        let req = CommitInvoiceRequest {
            invoice_id: None,
            expected_version: None,
            invoice: Some(valid_invoice()),
            allocations: vec![alloc("100.00", "88.00", "12.00")],
            idempotency_key: "idem-key".to_string(),
            ..base_request()
        };
        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("invoice_id").unwrap().is_null());
        assert!(json.get("invoice").is_some());
        assert_eq!(json.get("idempotency_key").unwrap(), "idem-key");
        let back: CommitInvoiceRequest = serde_json::from_value(json).unwrap();
        assert!(back.prepare().is_ok());
    }

    #[test]
    fn convert_post_allocations_preserves_order() {
        let lines = vec![alloc("10.00", "8.80", "1.20"), alloc("20.00", "17.60", "2.40")];
        let converted = convert_post_allocations(&lines);
        assert_eq!(converted[0].allocated_gross_amount.to_string(), "10.00");
        assert_eq!(converted[1].allocated_gross_amount.to_string(), "20.00");
        // Ensure same as commit conversion
        let via_commit = convert_invoice_allocations(&lines);
        assert_eq!(
            via_commit, converted,
            "两条入口对同一 facts 必须产出完全相同 plan 输入"
        );
    }

    #[test]
    fn two_entries_same_facts_produce_identical_plan() {
        use entities::ids::{InvoiceId, SalesInvoiceAllocationId};
        use entities::receivable::SalesInvoiceAllocationPlan;

        let lines = vec![alloc("60.00", "52.80", "7.20"), alloc("40.00", "35.20", "4.80")];
        let invoice_id = InvoiceId::new("inv-plan-1");
        let gross = Amount::from_str("100.00").unwrap();
        let net = Amount::from_str("88.00").unwrap();
        let tax = Amount::from_str("12.00").unwrap();
        let allocation_ids = vec![
            SalesInvoiceAllocationId::new("alloc-1"),
            SalesInvoiceAllocationId::new("alloc-2"),
        ];
        let via_commit = convert_invoice_allocations(&lines);
        let via_post = convert_post_allocations(&lines);
        assert_eq!(via_commit, via_post, "两条入口转换必须产出相同输入行");
        let plan_commit = SalesInvoiceAllocationPlan::new(
            invoice_id.clone(),
            gross,
            net,
            tax,
            &via_commit,
            &allocation_ids,
        )
        .expect("commit 计划必须构造成功");
        let plan_post =
            SalesInvoiceAllocationPlan::new(invoice_id, gross, net, tax, &via_post, &allocation_ids)
                .expect("post 计划必须构造成功");
        assert_eq!(
            plan_commit.new_allocations(),
            plan_post.new_allocations(),
            "同一 facts 的两条入口必须产出完全相同 allocations"
        );
        assert_eq!(
            plan_commit.account_invoicing_deltas(),
            plan_post.account_invoicing_deltas(),
            "同一 facts 的两条入口必须产出完全相同 account_deltas"
        );
        let seqs_commit: Vec<u32> = plan_commit
            .new_allocations()
            .iter()
            .map(|a| a.allocation_seq)
            .collect();
        let seqs_post: Vec<u32> = plan_post
            .new_allocations()
            .iter()
            .map(|a| a.allocation_seq)
            .collect();
        assert_eq!(seqs_commit, vec![1, 2]);
        assert_eq!(
            seqs_commit, seqs_post,
            "序号必须按输入顺序从 1 连续且两条入口一致"
        );

        // swapped input must produce different plan (order sensitivity)
        let swapped = vec![lines[1].clone(), lines[0].clone()];
        let via_swapped = convert_invoice_allocations(&swapped);
        let plan_swapped = SalesInvoiceAllocationPlan::new(
            InvoiceId::new("inv-plan-1"),
            gross,
            net,
            tax,
            &via_swapped,
            &allocation_ids,
        )
        .expect("swapped 计划必须构造成功");
        assert_ne!(
            plan_commit.new_allocations()[0].allocated_gross_amount,
            plan_swapped.new_allocations()[0].allocated_gross_amount,
            "输入顺序不同必须产出不同 plan"
        );
        assert_ne!(plan_commit, plan_swapped);
    }

    #[test]
    fn ensure_sales_invoice_rejects_non_sales() {
        use entities::common::time::BusinessDate;
        use entities::ids::InvoiceId;
        use entities::ids::PartyId;
        use entities::receivable::{Invoice, InvoiceData, InvoiceDirection, InvoiceKind};

        let purchase_invoice = Invoice::new(
            InvoiceId::new("inv-purchase"),
            InvoiceData {
                invoice_direction: InvoiceDirection::Purchase,
                invoice_kind: InvoiceKind::Blue,
                party_id: PartyId::new("party-1"),
                invoice_code: None,
                invoice_no: "INV-P-1".to_string(),
                invoice_date: BusinessDate::from_ymd(2026, 8, 6).unwrap(),
                gross_amount: Amount::from_str("100.00").unwrap(),
                net_amount: Amount::from_str("88.00").unwrap(),
                tax_amount: Amount::from_str("12.00").unwrap(),
                rounding_adjustment_amount: Amount::from_str("0.00").unwrap(),
                rounding_reason: None,
                original_invoice_id: None,
            },
            "tester",
        )
        .expect("进项发票必须可构造");
        let err = ensure_sales_invoice(&purchase_invoice).unwrap_err();
        assert!(matches!(err, Error::ValidationError(_)));
        assert!(err.to_string().contains("应收登记命令只接受销项发票"));

        let sales_invoice = Invoice::new(
            InvoiceId::new("inv-sales"),
            InvoiceData {
                invoice_direction: InvoiceDirection::Sales,
                invoice_kind: InvoiceKind::Blue,
                party_id: PartyId::new("party-1"),
                invoice_code: None,
                invoice_no: "INV-S-1".to_string(),
                invoice_date: BusinessDate::from_ymd(2026, 8, 6).unwrap(),
                gross_amount: Amount::from_str("100.00").unwrap(),
                net_amount: Amount::from_str("88.00").unwrap(),
                tax_amount: Amount::from_str("12.00").unwrap(),
                rounding_adjustment_amount: Amount::from_str("0.00").unwrap(),
                rounding_reason: None,
                original_invoice_id: None,
            },
            "tester",
        )
        .expect("销项发票必须可构造");
        assert!(ensure_sales_invoice(&sales_invoice).is_ok());
    }
}
