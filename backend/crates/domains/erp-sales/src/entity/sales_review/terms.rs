//! 销售变更可编辑条款；客户、合同和结算主体身份保持不变。

use erp_core::money::Rate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::entity::sales_order::{HeaderSnapshotData, SalesOrderWorkingCopy, SalesOrderWorkingCopyUpdate};
use crate::{Error, Result};

/// 随销售变更冻结并生效的商业条款。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SalesChangeTerms {
    /// 项目名称；空字符串表示清除。
    pub project_name: String,
    /// 付款条件编码。
    pub payment_term_code: String,
    /// 付款条件名称。
    pub payment_term_name: String,
    /// 开票要求。
    pub invoice_type: String,
    /// 表头税点，百分数文本。
    pub tax_point: String,
}

impl SalesChangeTerms {
    /// 读取工作副本的可编辑条款。
    ///
    /// # 参数
    /// * `copy` - 已授权的销售变更工作副本。
    ///
    /// # 返回
    /// 返回完整条款。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn from_copy(copy: &SalesOrderWorkingCopy) -> Self {
        Self {
            project_name: copy.project_name.clone().unwrap_or_default(),
            payment_term_code: copy.payment_term_snapshot.payment_term_code.clone(),
            payment_term_name: copy.payment_term_snapshot.payment_term_name.clone(),
            invoice_type: copy.invoice_requirement_snapshot.invoice_type.clone(),
            tax_point: copy.invoice_requirement_snapshot.tax_point.clone(),
        }
    }

    /// 通过实体快照校验更新条款，不接收客户或主体身份。
    ///
    /// # 参数
    /// * `copy` - 已证明可编辑的工作副本。
    /// * `actor_id` - 编辑人。
    ///
    /// # 返回
    /// 更新成功返回空结果。
    ///
    /// # 错误
    /// 必填、长度或工作副本状态校验失败时拒绝。
    pub fn apply(self, copy: &mut SalesOrderWorkingCopy, actor_id: &str) -> Result<()> {
        let tax: Rate = self.tax_point.parse()?;
        if !(Decimal::ZERO..=Decimal::from(100)).contains(&tax.to_decimal()) {
            return Err(Error::ValidationError("税点须在0至100之间".into()));
        }
        let snapshot = HeaderSnapshotData {
            customer_name: copy.customer_snapshot.customer_name.clone(),
            contract_no: copy.contract_snapshot.as_ref().map(|value| value.contract_no.clone()),
            settlement_party_name: copy
                .settlement_party_snapshot
                .as_ref()
                .map(|value| value.settlement_party_name.clone()),
            payment_term_code: self.payment_term_code,
            payment_term_name: self.payment_term_name,
            invoice_type: self.invoice_type,
            tax_point: self.tax_point,
        };
        Ok(copy.update(
            SalesOrderWorkingCopyUpdate {
                snapshot: Some(snapshot),
                project_name: Some(self.project_name),
                ..Default::default()
            },
            actor_id,
        )?)
    }
}
