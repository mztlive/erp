//! 签约前的不可变模板、年度流水和领号记录。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// 固定销售合同编号组；多个公司主体可共享同一组。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumberGroup {
    FSY,
    ZHYF,
    GYL,
    BDKJ,
}

impl NumberGroup {
    /// 返回不随公司名称变化的编号前缀。
    /// # 参数
    /// 无。
    /// # 返回
    /// 固定前缀。
    /// # 错误
    /// 无。
    pub fn prefix(self) -> &'static str {
        match self {
            Self::FSY => "FSY",
            Self::ZHYF => "ZHYF",
            Self::GYL => "GYL",
            Self::BDKJ => "BDKJ",
        }
    }

    /// 生成两位年度、四位流水的销售合同号。
    /// # 参数
    /// * `year` - 完整公历年度。
    /// * `sequence` - 1 至 9999 的年度流水。
    /// # 返回
    /// 固定格式合同号。
    /// # 错误
    /// 年份或流水越界时拒绝，禁止截断或回绕。
    pub fn number(self, year: i32, sequence: u32) -> Result<String> {
        validate_year(year)?;
        if !(1..=9999).contains(&sequence) {
            return Err(Error::BusinessLogicError("本年度合同流水已用完，请联系管理员".into()));
        }
        Ok(format!("{}-S-{:02}{sequence:04}", self.prefix(), year % 100))
    }
}

/// 只允许 2000 至 2099 年，避免两位年号在世纪间重复。
///
/// # 参数
/// * `year` - 完整公历年度。
///
/// # 返回
/// 年度在范围内时无返回值。
///
/// # 错误
/// 超出该范围时返回 `ValidationError`。
pub(crate) fn validate_year(year: i32) -> Result<()> {
    if !(2000..=2099).contains(&year) {
        return Err(Error::ValidationError("合同编号年度须在 2000 至 2099 年之间".into()));
    }
    Ok(())
}

/// 创建边界统一规范化短文本：去首尾空白，最长 256 个字符。
///
/// # 参数
/// * `raw` - 原始文本。
/// * `label` - 写入校验错误的字段名。
/// * `required` - 为真时，空白视为缺失。
///
/// # 返回
/// 去空白后的文本。非必填且原文为空白时返回空字符串。
///
/// # 错误
/// 必填为空，或字符数超过 256 时返回 `ValidationError`。
pub(crate) fn text(raw: &str, label: &str, required: bool) -> Result<String> {
    let value = raw.trim();
    if (required && value.is_empty()) || value.chars().count() > 256 {
        return Err(Error::ValidationError(format!(
            "{label}{}且不超过 256 字",
            if required { "必填" } else { "可留空" }
        )));
    }
    Ok(value.into())
}

/// 一个公司绑定一个稳定编号组；主体改名不改组。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct CompanyNumbering {
    #[serde(flatten)]
    pub base: BaseModel,
    pub group: NumberGroup,
}

/// Word 模板文件与签约主体固定，替换文件须创建新模板。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct ContractTemplate {
    #[serde(flatten)]
    pub base: BaseModel,
    pub name: String,
    pub company_id: String,
    pub company_name: String,
    pub group: NumberGroup,
    pub file_name: String,
    pub enabled: bool,
}

impl ContractTemplate {
    /// 获取本模板唯一且不可覆盖的对象路径。
    /// # 参数
    /// 无。
    /// # 返回
    /// 服务端生成的安全相对路径。
    /// # 错误
    /// 无。
    pub fn object_key(&self) -> String {
        format!("contract-templates/{}.docx", self.base.id)
    }
}

/// 编号组与年度共用流水，必须由管理员明确初始化。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct ContractCounter {
    #[serde(flatten)]
    pub base: BaseModel,
    pub group: NumberGroup,
    pub year: i32,
    pub last_sequence: u32,
}

impl ContractCounter {
    /// 按用户确认的历史号段生成年度起点。
    /// # 参数
    /// * `group` - 稳定编号组。
    /// * `year` - 完整年度。
    /// # 返回
    /// 2026 年承接线下已用流水，其他年度从零开始。
    /// # 错误
    /// 年份越界时拒绝。
    pub fn initial(group: NumberGroup, year: i32) -> Result<Self> {
        validate_year(year)?;
        let last_sequence = if year == 2026 {
            match group {
                NumberGroup::FSY => 452,
                NumberGroup::ZHYF => 22,
                NumberGroup::GYL => 15,
                NumberGroup::BDKJ => 24,
            }
        } else {
            0
        };
        Ok(Self { base: BaseModel::new(format!("{}-{year}", group.prefix())), group, year, last_sequence })
    }
    /// 只允许向前校准，禁止回收已分配的号码。
    /// # 参数
    /// * `last` - 确认已使用的最大年度流水。
    /// # 返回
    /// 校准成功返回空值。
    /// # 错误
    /// 回退或超过四位流水时拒绝。
    pub fn advance_to(&mut self, last: u32) -> Result<()> {
        if last < self.last_sequence || last > 9999 {
            return Err(Error::ValidationError("已用流水只能向前调整，范围为 0 至 9999".into()));
        }
        self.last_sequence = last;
        Ok(())
    }

    /// 在持久化事务中分配下一号；更新与申请记录必须一起提交。
    /// # 参数
    /// 无。
    /// # 返回
    /// 下一合同号。
    /// # 错误
    /// 流水用尽时拒绝且不修改计数。
    pub fn allocate(&mut self) -> Result<String> {
        let next =
            self.last_sequence.checked_add(1).ok_or_else(|| Error::ValidationError("合同流水超限".into()))?;
        let number = self.group.number(self.year, next)?;
        self.last_sequence = next;
        Ok(number)
    }
}

/// 不可变领号记录；重复下载只读取此记录。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct ContractApplication {
    #[serde(flatten)]
    pub base: BaseModel,
    pub command_id: String,
    pub applicant_id: String,
    pub template_id: String,
    pub template_name: String,
    pub company_name: String,
    pub purpose: String,
    pub contract_no: String,
}

impl ContractApplication {
    /// 相同申请键只允许重放同一模板与用途。
    /// # 参数
    /// * `template_id` - 重试请求模板。
    /// * `purpose` - 已规范化用途。
    /// # 返回
    /// 相同请求返回成功。
    /// # 错误
    /// 同一键携带不同内容返回冲突。
    pub fn require_same(&self, template_id: &str, purpose: &str) -> Result<()> {
        if self.template_id != template_id || self.purpose != purpose {
            return Err(Error::ConflictError("该申请已登记，请刷新后重新申请".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reproduces_all_four_examples_and_keeps_gyl() {
        for (group, sequence, expected) in [
            (NumberGroup::FSY, 452, "FSY-S-260452"),
            (NumberGroup::ZHYF, 22, "ZHYF-S-260022"),
            (NumberGroup::GYL, 15, "GYL-S-260015"),
            (NumberGroup::BDKJ, 24, "BDKJ-S-260024"),
        ] {
            assert_eq!(group.number(2026, sequence).unwrap(), expected);
        }
        assert_eq!(NumberGroup::FSY.number(2027, 1).unwrap(), "FSY-S-270001");
        assert!(NumberGroup::FSY.number(2026, 0).is_err());
        assert!(NumberGroup::FSY.number(2100, 1).is_err());
    }

    #[test]
    fn counter_cannot_rewind_or_overflow() {
        let mut counter = ContractCounter {
            base: BaseModel::fake(),
            group: NumberGroup::FSY,
            year: 2026,
            last_sequence: 452,
        };
        assert_eq!(counter.allocate().unwrap(), "FSY-S-260453");
        assert!(counter.advance_to(452).is_err());
        counter.advance_to(9999).unwrap();
        assert!(counter.allocate().is_err());
        assert_eq!(counter.last_sequence, 9999);
    }

    #[test]
    fn starts_after_confirmed_offline_numbers_and_resets_next_year() {
        for (group, expected) in [
            (NumberGroup::FSY, "FSY-S-260453"),
            (NumberGroup::ZHYF, "ZHYF-S-260023"),
            (NumberGroup::GYL, "GYL-S-260016"),
            (NumberGroup::BDKJ, "BDKJ-S-260025"),
        ] {
            assert_eq!(ContractCounter::initial(group, 2026).unwrap().allocate().unwrap(), expected);
            assert_eq!(
                ContractCounter::initial(group, 2027).unwrap().allocate().unwrap(),
                format!("{}-S-270001", group.prefix())
            );
        }
        let mut counter = ContractCounter::initial(NumberGroup::FSY, 2026).unwrap();
        assert!(counter.advance_to(451).is_err());
        assert!(counter.advance_to(10000).is_err());
    }

    #[test]
    fn replay_rejects_changed_content() {
        let application = ContractApplication {
            base: BaseModel::fake(),
            command_id: "cmd".into(),
            applicant_id: "sales".into(),
            template_id: "pdf-1".into(),
            template_name: "模板".into(),
            company_name: "公司".into(),
            purpose: "采购福利".into(),
            contract_no: "GYL-S-260001".into(),
        };
        assert!(application.require_same("pdf-1", "采购福利").is_ok());
        assert!(application.require_same("pdf-2", "采购福利").is_err());
        assert!(application.require_same("pdf-1", "另一个用途").is_err());
    }
}
