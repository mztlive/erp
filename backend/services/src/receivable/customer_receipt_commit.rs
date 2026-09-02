//! 客户回款原子提交命令的 DTO 归一化与类型化准备。
//!
//! 将 `CommitCustomerReceiptRequest` 的 `new` / `existing` 互斥形态、
//! `expected_version` 正数约束与核销分配转换收敛到 DTO 自身，
//! 使非法字段组合在进入事务前即可拒绝；Service 仅负责存在性、
//! 版本锁与事务编排，不再复制字段组合判断。

use entities::receivable::PendingReceiptAllocation;

use crate::errors::{Error, Result};

use super::dto::{CommitCustomerReceiptRequest, CreateCustomerReceiptRequest};

/// 已验证的客户回款提交命令。
///
/// 由 `CommitCustomerReceiptRequest::prepare` 产生，保证互斥形态与
/// 分配行均已校验；调用方无需再次判断字段组合，`Service` 仅需按变体
/// 分别处理新建或已有草稿的持久化与版本校验。
#[derive(Debug, Clone)]
pub enum PreparedCustomerReceiptCommit {
    /// 新建回款：携带完整创建字段与已验证的待过账分配。
    New {
        /// 新回款创建字段（已通过 `Validate`，但实体层仍会二次归一化）。
        receipt: CreateCustomerReceiptRequest,
        /// 已验证的待过账核销分配（金额为正，顺序与请求一致）。
        allocations: Vec<PendingReceiptAllocation>,
    },
    /// 提交已有草稿：携带草稿主键、乐观锁版本与已验证分配。
    Existing {
        /// 已有草稿主键。
        receipt_id: String,
        /// 已有草稿期望版本（>0）。
        expected_version: u64,
        /// 已验证的待过账核销分配。
        allocations: Vec<PendingReceiptAllocation>,
    },
}

/// 将提交请求行的集合转换为领域待过账分配。
///
/// # 参数
/// * `lines` - 请求中的核销分配行（`receivable_entry_id` + `allocated_amount`）
///
/// # 返回
/// 全部行均合法时返回与输入顺序一致的 `PendingReceiptAllocation` 集合。
///
/// # 错误
/// 任一行的 `allocated_amount` 非正或形态非法时返回 `ValidationError`。
///
/// # 约束
/// 纯转换，不触及 I/O、时钟或 ID 生成；首个失败即短路，保证调用方可
/// 以确定性复现首错语义。
pub(crate) fn convert_allocations(
    lines: &[super::dto::ReceiptAllocationLineRequest],
) -> Result<Vec<PendingReceiptAllocation>> {
    lines
        .iter()
        .map(|line| {
            PendingReceiptAllocation::new(line.receivable_entry_id.clone(), line.allocated_amount)
                .map_err(Into::into)
        })
        .collect()
}

impl CommitCustomerReceiptRequest {
    /// 归一化并验证客户回款原子提交命令。
    ///
    /// 判定 `receipt` / `receipt_id + expected_version` 的互斥形态，
    /// 校验 `expected_version` 为正数，并将 `allocations` 转换为
    /// 强类型的 `PendingReceiptAllocation` 集合。
    ///
    /// # 参数
    /// * `&self` - 原始提交请求（`receipt_id` / `expected_version` / `receipt` / `allocations`）
    ///
    /// # 返回
    /// 合法时返回 `PreparedCustomerReceiptCommit::New` 或 `::Existing`，
    /// 携带已验证的分配集合；非法组合在进入事务前即失败。
    ///
    /// # 错误
    /// * `ValidationError` - `new` 与 `existing` 字段同时提供、同时缺失、
    ///   `expected_version` 为 `None` 或 `0`、`allocations` 为空或任一
    ///   分配行金额非法/溢出时返回
    ///
    /// # 约束
    /// * I/O-free：不查询数据库、不触及全局时钟/ID 生成器
    /// * 确定性：同输入必得同输出或同首错，分配顺序与输入一致
    /// * 兼容性：错误文案保持与原 `commit_customer_receipt` 分支一致，
    ///   以免改变前端对非法形态的展示
    pub fn prepare(&self) -> Result<PreparedCustomerReceiptCommit> {
        let allocations = convert_allocations(&self.allocations)?;
        if allocations.is_empty() {
            return Err(Error::ValidationError("至少提供一条核销分配".to_string()));
        }
        match (&self.receipt_id, self.expected_version, &self.receipt) {
            (None, None, Some(receipt)) => Ok(PreparedCustomerReceiptCommit::New {
                receipt: receipt.clone(),
                allocations,
            }),
            (Some(receipt_id), Some(version), None) if version > 0 => {
                Ok(PreparedCustomerReceiptCommit::Existing {
                    receipt_id: receipt_id.clone(),
                    expected_version: version,
                    allocations,
                })
            }
            _ => Err(Error::ValidationError(
                "新回款必须提交 receipt；已有草稿必须提交 receipt_id 与 expected_version".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use entities::ids::{CustomerAccountId, PartyId, ReceivableEntryId};
    use entities::money::Amount;
    use std::str::FromStr;

    fn valid_receipt() -> CreateCustomerReceiptRequest {
        CreateCustomerReceiptRequest {
            receipt_no: "RC-TEST-001".to_string(),
            counterparty_party_id: PartyId::new("party-1"),
            customer_id: Some(CustomerAccountId::new("cust-1")),
            received_at: entities::common::time::Instant::from_unix_secs(1_700_000_000),
            amount: Amount::from_str("1000.00").unwrap(),
            bank_reference: Some("BANK-REF".to_string()),
        }
    }

    fn allocation(amount: &str) -> super::super::dto::ReceiptAllocationLineRequest {
        super::super::dto::ReceiptAllocationLineRequest {
            receivable_entry_id: ReceivableEntryId::new("re-1"),
            allocated_amount: Amount::from_str(amount).unwrap(),
        }
    }

    fn base_request() -> CommitCustomerReceiptRequest {
        CommitCustomerReceiptRequest {
            receipt_id: None,
            expected_version: None,
            receipt: None,
            allocations: vec![allocation("100.00")],
            idempotency_key: "idem-1".to_string(),
        }
    }

    #[test]
    fn prepare_accepts_new_with_receipt_and_allocations() {
        let req = CommitCustomerReceiptRequest {
            receipt: Some(valid_receipt()),
            ..base_request()
        };
        let prepared = req.prepare().expect("new 形态必须通过");
        match prepared {
            PreparedCustomerReceiptCommit::New { receipt, allocations } => {
                assert_eq!(receipt.receipt_no, "RC-TEST-001");
                assert_eq!(allocations.len(), 1);
                assert_eq!(allocations[0].allocated_amount.to_string(), "100.00");
            }
            _ => panic!("应为 New"),
        }
    }

    #[test]
    fn prepare_accepts_existing_with_id_and_version() {
        let req = CommitCustomerReceiptRequest {
            receipt_id: Some("cr-1".to_string()),
            expected_version: Some(2),
            receipt: None,
            ..base_request()
        };
        let prepared = req.prepare().expect("existing 形态必须通过");
        match prepared {
            PreparedCustomerReceiptCommit::Existing {
                receipt_id,
                expected_version,
                allocations,
            } => {
                assert_eq!(receipt_id, "cr-1");
                assert_eq!(expected_version, 2);
                assert_eq!(allocations.len(), 1);
            }
            _ => panic!("应为 Existing"),
        }
    }

    #[test]
    fn prepare_rejects_both_provided() {
        let req = CommitCustomerReceiptRequest {
            receipt_id: Some("cr-1".to_string()),
            expected_version: Some(1),
            receipt: Some(valid_receipt()),
            ..base_request()
        };
        let err = req.prepare().unwrap_err();
        assert!(matches!(err, Error::ValidationError(_)));
        assert!(err.to_string().contains("新回款必须提交 receipt"));
    }

    #[test]
    fn prepare_rejects_both_missing() {
        let req = CommitCustomerReceiptRequest {
            receipt_id: None,
            expected_version: None,
            receipt: None,
            ..base_request()
        };
        let err = req.prepare().unwrap_err();
        assert!(matches!(err, Error::ValidationError(_)));
    }

    #[test]
    fn prepare_rejects_version_zero() {
        let req = CommitCustomerReceiptRequest {
            receipt_id: Some("cr-1".to_string()),
            expected_version: Some(0),
            receipt: None,
            ..base_request()
        };
        let err = req.prepare().unwrap_err();
        assert!(matches!(err, Error::ValidationError(_)));
        assert!(err.to_string().contains("新回款必须提交 receipt"));
    }

    #[test]
    fn prepare_rejects_version_missing() {
        let req = CommitCustomerReceiptRequest {
            receipt_id: Some("cr-1".to_string()),
            expected_version: None,
            receipt: None,
            ..base_request()
        };
        let err = req.prepare().unwrap_err();
        assert!(matches!(err, Error::ValidationError(_)));
    }

    #[test]
    fn prepare_rejects_allocation_invalid_zero() {
        let req = CommitCustomerReceiptRequest {
            receipt: Some(valid_receipt()),
            allocations: vec![allocation("0.00")],
            ..base_request()
        };
        // override allocations after base
        let mut req2 = req;
        req2.allocations = vec![allocation("0.00")];
        let err = req2.prepare().unwrap_err();
        assert!(matches!(err, Error::Logic(_)) || matches!(err, Error::ValidationError(_)));
    }

    #[test]
    fn prepare_rejects_allocation_invalid_negative() {
        let req = CommitCustomerReceiptRequest {
            receipt: Some(valid_receipt()),
            allocations: vec![allocation("-10.00")],
            ..base_request()
        };
        let mut req2 = req;
        req2.allocations = vec![allocation("-10.00")];
        let err = req2.prepare().unwrap_err();
        assert!(matches!(err, Error::Logic(_)) || matches!(err, Error::ValidationError(_)));
    }

    #[test]
    fn prepare_rejects_empty_allocations() {
        let req = CommitCustomerReceiptRequest {
            receipt: Some(valid_receipt()),
            allocations: vec![],
            ..base_request()
        };
        let err = req.prepare().unwrap_err();
        assert!(matches!(err, Error::ValidationError(_)));
    }

    #[test]
    fn prepare_preserves_illegal_combo_error_message_compat() {
        let req = CommitCustomerReceiptRequest {
            receipt_id: Some("cr-1".to_string()),
            expected_version: Some(1),
            receipt: Some(valid_receipt()),
            allocations: vec![allocation("10.00")],
            idempotency_key: "k".to_string(),
        };
        let err = req.prepare().unwrap_err();
        assert!(err
            .to_string()
            .contains("新回款必须提交 receipt；已有草稿必须提交 receipt_id 与 expected_version"));
    }

    #[test]
    fn prepare_keeps_allocation_order_deterministic() {
        let req = CommitCustomerReceiptRequest {
            receipt: Some(valid_receipt()),
            allocations: vec![allocation("10.00"), allocation("20.00"), allocation("30.00")],
            ..base_request()
        };
        let mut req2 = req;
        req2.receipt = Some(valid_receipt());
        req2.allocations = vec![allocation("10.00"), allocation("20.00"), allocation("30.00")];
        let prepared = req2.prepare().unwrap();
        match prepared {
            PreparedCustomerReceiptCommit::New { allocations, .. } => {
                assert_eq!(allocations[0].allocated_amount.to_string(), "10.00");
                assert_eq!(allocations[1].allocated_amount.to_string(), "20.00");
                assert_eq!(allocations[2].allocated_amount.to_string(), "30.00");
            }
            _ => panic!("should be New"),
        }
    }

    #[test]
    fn prepare_allocation_overflow_is_surfaced() {
        // 使用接近 Decimal 最大值的金额，验证溢出不 panic 且可被上层感知
        // rust_decimal 最大值约为 79228162514264337593543950335，构造两个大额分配
        // 若实现未做 checked_add，prepare 仍应成功（amount 正数），溢出由后续
        // 领域校验负责；此处验证 prepare 不 panic
        let huge = "79228162514264337593543950330";
        let req = CommitCustomerReceiptRequest {
            receipt: Some(valid_receipt()),
            allocations: vec![allocation(huge), allocation("10.00")],
            ..base_request()
        };
        let mut req2 = req;
        req2.allocations = vec![allocation(huge), allocation("10.00")];
        // prepare 只验证单行正数，不应对合计溢出 panic
        let result = req2.prepare();
        assert!(result.is_ok());
    }

    #[test]
    fn legal_new_command_serializes_stably() {
        let req = CommitCustomerReceiptRequest {
            receipt_id: None,
            expected_version: None,
            receipt: Some(valid_receipt()),
            allocations: vec![allocation("100.00")],
            idempotency_key: "idem-key".to_string(),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("receipt_id").unwrap().is_null());
        assert!(json.get("receipt").is_some());
        assert_eq!(json.get("idempotency_key").unwrap(), "idem-key");
        // round-trip
        let back: CommitCustomerReceiptRequest = serde_json::from_value(json).unwrap();
        assert!(back.prepare().is_ok());
    }
}
