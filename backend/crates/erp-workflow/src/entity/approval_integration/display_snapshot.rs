//! 审批提交时冻结的工作台展示合同；仅保存已筛选的业务字段，不保存任意 JSON 或权限。
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// 同一审批版本的公共展示；身份只用于关联与导航，不授予阅读权限。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalDisplaySnapshot {
    pub root_document_id: String,
    pub counterparty_label: Option<String>,
    pub impact_summary: Option<String>,
    pub source: ApprovalBriefSource,
}

/// 经业务读模型筛选的固定展示字段。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ApprovalBriefSource {
    pub customer: Option<String>,
    pub amount_label: Option<String>,
    pub lines: Vec<ApprovalBriefLine>,
    pub more_count: u32,
    pub submitter_name: Option<String>,
    pub list_summary: String,
    pub extra_sections: Vec<ApprovalBriefSection>,
}

/// 冻结明细摘要；完整行数由 more_count 保留。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalBriefLine {
    pub title: String,
    pub quantity: Option<String>,
    pub due_label: Option<String>,
}

/// 有界的业务键值；object_id 仅作关联路由使用。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalBriefSection {
    pub label: String,
    pub value: String,
    pub numeric: bool,
    pub object_id: Option<String>,
}

impl ApprovalDisplaySnapshot {
    /// 校验快照边界，防止把无限明细或任意大文本写进审批事实。
    ///
    /// # 返回
    /// 合法展示返回成功。
    /// # 错误
    /// 缺失来源身份、超过 128 个字段/100 行或单项超过 8192 字符时返回错误。
    pub fn validate(&self) -> Result<()> {
        if self.root_document_id.trim().is_empty()
            || self.source.extra_sections.len() > 128
            || self.source.lines.len() > 100
        {
            return Err(Error::from("审批展示快照超出允许范围"));
        }
        let texts = [&self.root_document_id, &self.source.list_summary]
            .into_iter()
            .chain(self.counterparty_label.iter())
            .chain(self.impact_summary.iter())
            .chain(self.source.customer.iter())
            .chain(self.source.amount_label.iter())
            .chain(self.source.submitter_name.iter())
            .chain(
                self.source
                    .extra_sections
                    .iter()
                    .flat_map(|s| [&s.label, &s.value].into_iter().chain(s.object_id.iter())),
            )
            .chain(self.source.lines.iter().flat_map(|l| {
                std::iter::once(&l.title)
                    .chain(l.quantity.iter())
                    .chain(l.due_label.iter())
            }));
        if texts.into_iter().any(|s| s.chars().count() > 8192) {
            return Err(Error::from("审批展示快照文本过长"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 有界快照持久化后按原值还原；拒绝无界文本、字段和空关联。
    #[test]
    fn display_snapshot_roundtrip_and_bounds() {
        let mut display = ApprovalDisplaySnapshot {
            root_document_id: "receipt-1".into(),
            counterparty_label: Some("客户甲".into()),
            impact_summary: None,
            source: ApprovalBriefSource {
                list_summary: "原始回款".into(),
                extra_sections: vec![ApprovalBriefSection {
                    label: "银行流水".into(),
                    value: "BANK-001".into(),
                    numeric: false,
                    object_id: None,
                }],
                ..Default::default()
            },
        };
        display.validate().unwrap();
        assert_eq!(
            serde_json::from_value::<ApprovalDisplaySnapshot>(serde_json::to_value(&display).unwrap())
                .unwrap(),
            display
        );
        display.source.extra_sections[0].value = "字".repeat(8193);
        assert!(display.validate().is_err());
        display.source.extra_sections.clear();
        display.root_document_id.clear();
        assert!(display.validate().is_err());
    }
}
