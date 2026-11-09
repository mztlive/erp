use serde::Serialize;
/// 事项简报中的只读键值。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemSummarySection {
    pub label: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numeric: Option<bool>,
    /// 可跳转关联单据的稳定身份；仅作路由键，不上屏。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_id: Option<String>,
}

impl WorkItemSummarySection {
    /// 构造事项简报中的只读键值。
    ///
    /// # 参数
    /// * `label` - 展示标签
    /// * `value` - 展示值
    ///
    /// # 返回
    /// 返回非数值、无跳转的键值。
    ///
    /// # 错误
    /// 无。
    pub fn new(label: String, value: String) -> Self {
        Self { label, value, numeric: None, object_id: None }
    }

    /// 设置数值标志。
    ///
    /// # 参数
    /// * `numeric` - 是否为数值
    ///
    /// # 返回
    /// 返回更新后的键值。
    ///
    /// # 错误
    /// 无。
    pub fn with_numeric(mut self, numeric: Option<bool>) -> Self {
        self.numeric = numeric;
        self
    }

    /// 设置可跳转关联单据。
    ///
    /// # 参数
    /// * `object_id` - 关联单据稳定身份
    ///
    /// # 返回
    /// 返回更新后的键值。
    ///
    /// # 错误
    /// 无。
    pub fn with_object_id(mut self, object_id: Option<String>) -> Self {
        self.object_id = object_id;
        self
    }
}

/// 事项简报中的一行业务明细。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemBriefLine {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_label: Option<String>,
}
