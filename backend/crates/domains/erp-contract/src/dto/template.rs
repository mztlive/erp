//! 模板与领号接口；文件存储键不进入公开视图。

use serde::{Deserialize, Serialize};

use crate::entity::template::{ContractApplication, ContractCounter, ContractTemplate, NumberGroup};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateTemplateRequest {
    pub name: String,
    pub company_id: String,
    pub group: NumberGroup,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateStatusRequest {
    pub version: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigureCounterRequest {
    pub group: NumberGroup,
    pub year: i32,
    pub last_sequence: u32,
    /// 首次初始化为 None，后续调整必须携带当前版本。
    pub version: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyTemplateRequest {
    pub command_id: String,
    pub template_id: String,
    #[serde(default)]
    pub purpose: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TemplateListParams {
    pub page: Option<u64>,
    pub page_size: Option<u32>,
    pub include_disabled: Option<bool>,
}

impl TemplateListParams {
    /// 统一分页边界。
    /// # 参数
    /// 无。
    /// # 返回
    /// 一基页码与最多 100 条的页容量。
    /// # 错误
    /// 无。
    pub fn pagination(&self) -> (u64, u32) {
        (self.page.unwrap_or(1).clamp(1, 1_000_000), self.page_size.unwrap_or(20).clamp(1, 100))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateView {
    pub id: String,
    pub version: u64,
    pub name: String,
    pub company_id: String,
    pub company_name: String,
    pub group: NumberGroup,
    pub file_name: String,
    pub enabled: bool,
    pub created_at: u64,
}

impl From<ContractTemplate> for TemplateView {
    fn from(value: ContractTemplate) -> Self {
        Self {
            id: value.base.id,
            version: value.base.version,
            name: value.name,
            company_id: value.company_id,
            company_name: value.company_name,
            group: value.group,
            file_name: value.file_name,
            enabled: value.enabled,
            created_at: value.base.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplicationView {
    pub id: String,
    pub template_name: String,
    pub company_name: String,
    pub purpose: String,
    pub contract_no: String,
    pub created_at: u64,
}

impl From<ContractApplication> for ApplicationView {
    fn from(value: ContractApplication) -> Self {
        Self {
            id: value.base.id,
            template_name: value.template_name,
            company_name: value.company_name,
            purpose: value.purpose,
            contract_no: value.contract_no,
            created_at: value.base.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CounterView {
    pub group: NumberGroup,
    pub year: i32,
    pub last_sequence: u32,
    pub version: Option<u64>,
}

impl From<ContractCounter> for CounterView {
    fn from(value: ContractCounter) -> Self {
        Self {
            group: value.group,
            year: value.year,
            last_sequence: value.last_sequence,
            version: Some(value.base.version),
        }
    }
}
