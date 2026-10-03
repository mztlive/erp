//! 审批提交时冻结的工作台展示合同；仅保存已筛选的业务字段，不保存任意 JSON 或权限。
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

pub use super::material_file::ApprovalMaterialFile;

/// 同一审批版本的公共展示；身份只用于关联与导航，不授予阅读权限。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalDisplaySnapshot {
    pub root_document_id: String,
    pub counterparty_label: Option<String>,
    pub impact_summary: Option<String>,
    pub source: ApprovalBriefSource,
    /// 本次采购提交实际引用的不可变销售版本；历史缺失不读取当前销售单补齐。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_sales: Vec<ApprovalRelatedSalesSnapshot>,
}

/// 采购审批中冻结的来源销售摘要；只允许单层关联，不形成通用对象读取入口。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalRelatedSalesSnapshot {
    pub document_id: String,
    pub document_no: String,
    pub revision_id: String,
    pub revision_no: u32,
    pub source: ApprovalBriefSource,
}

impl ApprovalRelatedSalesSnapshot {
    /// 校验准确销售身份和单层摘要边界。
    /// # 参数
    /// 无；读取当前冻结字段。
    /// # 返回
    /// 身份、版本和摘要有效时成功。
    /// # 错误
    /// 缺失或越界身份、零版本及无界摘要时拒绝。
    pub fn validate(&self) -> Result<()> {
        if self.document_no.trim().is_empty()
            || self.revision_id.trim().is_empty()
            || self.revision_no == 0
            || self.document_no.chars().count() > 128
            || self.revision_id.chars().count() > 128
        {
            return Err(Error::from("审批关联销售版本身份无效"));
        }
        ApprovalDisplaySnapshot {
            root_document_id: self.document_id.clone(),
            counterparty_label: None,
            impact_summary: None,
            source: self.source.clone(),
            source_sales: Vec::new(),
        }
        .validate()
    }
}
impl ApprovalDisplaySnapshot {
    /// 以必填根单据构造展示快照；展示维度默认为空。
    ///
    /// # 参数
    /// * `root_document_id` - 根单据 ID
    ///
    /// # 返回
    /// 返回空展示来源的快照。
    ///
    /// # 错误
    /// 无。
    pub fn new(root_document_id: String) -> Self {
        Self {
            root_document_id,
            counterparty_label: None,
            impact_summary: None,
            source: ApprovalBriefSource::default(),
            source_sales: Vec::new(),
        }
    }
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
impl ApprovalBriefSection {
    /// 以必填键值构造展示节；关联默认为空。
    ///
    /// # 参数
    /// * `label` - 业务键
    /// * `value` - 业务值
    ///
    /// # 返回
    /// 返回非数值型的展示节。
    ///
    /// # 错误
    /// 无。
    pub fn new(label: String, value: String) -> Self {
        Self { label, value, numeric: false, object_id: None }
    }
}

impl ApprovalDisplaySnapshot {
    /// 校验快照边界，防止把无限明细或任意大文本写进审批事实。
    ///
    /// # 返回
    /// 合法展示返回成功。
    /// # 错误
    /// 缺失来源身份、超过 128 个字段/100 行或单项超过 8192 字符时返回错误。
    pub fn validate(&self) -> Result<()> {
        if self.source_sales.len() > 100 {
            return Err(Error::from("审批关联销售资料超过100个版本"));
        }
        for sales in &self.source_sales {
            sales.validate()?;
        }
        if self.root_document_id.trim().is_empty()
            || self.source.extra_sections.len() > 128
            || self.source.lines.len() > 100
        {
            return Err(Error::from("审批展示快照超出允许范围"));
        }
        let texts =
            [&self.root_document_id, &self.source.list_summary]
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
                    std::iter::once(&l.title).chain(l.quantity.iter()).chain(l.due_label.iter())
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
    #[test]
    fn brief_section_new_defaults_to_non_numeric() {
        let section = ApprovalBriefSection::new("k".into(), "v".into());
        assert!(!section.numeric && section.object_id.is_none());
    }

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
            source_sales: Vec::new(),
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

    #[test]
    fn related_sales_are_bounded_and_legacy_display_keeps_them_missing() {
        let mut display = ApprovalDisplaySnapshot::new("purchase-1".into());
        let legacy = serde_json::to_value(&display).unwrap();
        assert!(legacy.get("source_sales").is_none());
        assert!(serde_json::from_value::<ApprovalDisplaySnapshot>(legacy).unwrap().source_sales.is_empty());
        let source = ApprovalRelatedSalesSnapshot {
            document_id: "sales-1".into(),
            document_no: "XS-1".into(),
            revision_id: "revision-1".into(),
            revision_no: 1,
            source: ApprovalBriefSource::default(),
        };
        display.source_sales.push(source.clone());
        display.validate().unwrap();
        display.source_sales[0].revision_no = 0;
        assert!(display.validate().is_err());
        display.source_sales = vec![source; 101];
        assert!(display.validate().is_err());
    }
}
