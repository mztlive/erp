//! 我方公司身份；与客户、供应商身份共享 Party，但使用独立管理入口。
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// 公司维护资料；名称检索键由服务端生成并由唯一索引约束。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanyProfile {
    pub legal_name: String,
    pub short_name: Option<String>,
    pub aliases: Vec<String>,
    pub names: Vec<String>,
}

/// 用于公司别名与导入供应商名称的确定性匹配。
///
/// 去掉全部空白，把全角括号换成半角，再转为小写。
///
/// # 参数
/// * `raw` - 原始名称。
///
/// # 返回
/// 返回可用于精确比较的名称键。
///
/// # 错误
/// 不返回错误。
pub fn identity_name(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '（' => '(',
            '）' => ')',
            other => other,
        })
        .collect::<String>()
        .to_lowercase()
}

impl CompanyProfile {
    /// 校验公司全称、简称及导入别名，并生成精确匹配键。
    ///
    /// 全称与简称去首尾空白；空简称丢弃。别名去空白、去掉空项后排序去重。
    /// 别名个数按去空白前的输入长度判断。匹配键来自 [`identity_name`]。
    ///
    /// # 参数
    /// * `legal_name` - 公司全称。
    /// * `short_name` - 公司简称；空白视为没有简称。
    /// * `aliases` - 导入别名。
    ///
    /// # 返回
    /// 返回包含规范化名称与匹配键的公司资料。
    ///
    /// # 错误
    /// 全称为空、全称超过 256 字、别名超过 32 个，或简称/别名超过 128 字时，返回 [`erp_core::Error`]。
    pub fn new(legal_name: String, short_name: Option<String>, aliases: Vec<String>) -> Result<Self> {
        let legal_name = legal_name.trim().to_string();
        let short_name = short_name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        if legal_name.is_empty() || legal_name.chars().count() > 256 || aliases.len() > 32 {
            return Err(Error::from("公司全称必填且不超过 256 字，别名不超过 32 个"));
        }
        let mut aliases: Vec<String> =
            aliases.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        aliases.sort();
        aliases.dedup();
        let mut names = vec![identity_name(&legal_name)];
        for name in short_name.iter().chain(aliases.iter()) {
            if name.chars().count() > 128 {
                return Err(Error::from("公司简称或别名不能超过 128 字"));
            }
            names.push(identity_name(name));
        }
        names.sort();
        names.dedup();
        Ok(Self { legal_name, short_name, aliases, names })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aliases_are_normalized_and_bounded() {
        let p = CompanyProfile::new(
            " 公司（广东） ".into(),
            Some("公司".into()),
            vec![" 公司 ".into(), "科技".into()],
        )
        .unwrap();
        assert_eq!(p.names, vec!["公司", "公司(广东)", "科技"]);
        assert!(CompanyProfile::new(" ".into(), None, vec![]).is_err());
        assert!(CompanyProfile::new("公司".into(), None, vec!["x".into(); 33]).is_err());
    }
}
