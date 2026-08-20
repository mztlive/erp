use crate::error::{Error, Result};

/// 管理员密码环境变量名。
pub const PASSWORD_ENV: &str = "ERP_ADMIN_PASSWORD";

/// 已解析的密码来源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordSource {
    /// 命令行 `--password`。
    Flag(String),
    /// 环境变量 `ERP_ADMIN_PASSWORD`。
    Env(String),
    /// 终端交互输入。
    Prompt,
}

/// 按标志、环境变量、交互输入的顺序选择密码来源。
///
/// 空字符串视为未提供，继续尝试下一来源。
///
/// # 参数
/// * `flag` - `--password` 的原始值
/// * `env_value` - 已读取的环境变量值
///
/// # 返回值
/// 返回第一个非空来源；都为空时返回交互输入。
///
/// # 错误
/// 无。
pub fn select_password_source(flag: Option<String>, env_value: Option<String>) -> PasswordSource {
    if let Some(password) = nonempty(flag) {
        return PasswordSource::Flag(password);
    }
    if let Some(password) = nonempty(env_value) {
        return PasswordSource::Env(password);
    }

    PasswordSource::Prompt
}

/// 按来源取出明文密码，交互来源会要求确认。
///
/// # 参数
/// * `source` - 已选择的密码来源
///
/// # 返回值
/// 返回明文密码。
///
/// # 错误
/// 交互输入失败或两次输入不一致时返回用法错误。
pub fn take_password(source: PasswordSource) -> Result<String> {
    match source {
        PasswordSource::Flag(password) | PasswordSource::Env(password) => Ok(password),
        PasswordSource::Prompt => prompt_confirmed_password(),
    }
}

/// 从命令行标志或环境变量解析密码，缺失时进入交互输入。
///
/// # 参数
/// * `flag` - `--password` 的原始值
///
/// # 返回值
/// 返回明文密码。
///
/// # 错误
/// 交互输入失败或两次输入不一致时返回用法错误。
pub fn resolve_password(flag: Option<String>) -> Result<String> {
    take_password(select_password_source(flag, std::env::var(PASSWORD_ENV).ok()))
}

/// 过滤空字符串。
///
/// # 参数
/// * `value` - 可选字符串
///
/// # 返回值
/// 非空字符串原值，空字符串视为缺失。
///
/// # 错误
/// 无。
fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|item| !item.is_empty())
}

/// 交互输入密码并要求确认一次。
///
/// # 返回值
/// 两次输入一致时返回明文密码。
///
/// # 错误
/// 终端不可用或两次输入不一致时返回用法错误。
fn prompt_confirmed_password() -> Result<String> {
    let first = prompt_once("请输入密码: ")?;
    let second = prompt_once("请再次输入密码: ")?;
    if first != second {
        return Err(Error::Usage("两次输入的密码不一致".to_string()));
    }

    Ok(first)
}

/// 从终端读取一次隐藏密码。
///
/// # 参数
/// * `prompt` - 提示文案
///
/// # 返回值
/// 返回用户输入的密码字符串。
///
/// # 错误
/// 终端不可读时返回用法错误。
fn prompt_once(prompt: &str) -> Result<String> {
    rpassword::prompt_password(prompt).map_err(|error| Error::Usage(format!("无法读取密码: {error}")))
}

#[cfg(test)]
mod tests {
    use super::{select_password_source, PasswordSource};

    #[test]
    fn flag_takes_priority_over_env() {
        let source = select_password_source(Some("from-flag".to_string()), Some("from-env".to_string()));

        assert_eq!(source, PasswordSource::Flag("from-flag".to_string()));
    }

    #[test]
    fn empty_flag_falls_back_to_env() {
        let source = select_password_source(Some(String::new()), Some("from-env".to_string()));

        assert_eq!(source, PasswordSource::Env("from-env".to_string()));
    }

    #[test]
    fn missing_values_select_prompt() {
        assert_eq!(select_password_source(None, None), PasswordSource::Prompt);
        assert_eq!(
            select_password_source(Some(String::new()), Some(String::new())),
            PasswordSource::Prompt
        );
    }
}
