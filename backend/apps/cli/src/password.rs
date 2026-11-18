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

/// 为管理员命令决定密码来源：优先使用非空的 `--password`，其次使用非空环境变量值，
/// 都未提供才进入终端交互。此处只决定来源，不读取终端。
pub fn select_password_source(flag: Option<String>, env_value: Option<String>) -> PasswordSource {
    if let Some(password) = nonempty(flag) {
        return PasswordSource::Flag(password);
    }
    if let Some(password) = nonempty(env_value) {
        return PasswordSource::Env(password);
    }

    PasswordSource::Prompt
}

/// 将已选来源中的明文密码交给管理员用例。脚本提供的值可直接使用；终端输入必须确认，
/// 读取失败或两次不一致时返回用法错误。
pub fn take_password(source: PasswordSource) -> Result<String> {
    match source {
        PasswordSource::Flag(password) | PasswordSource::Env(password) => Ok(password),
        PasswordSource::Prompt => prompt_confirmed_password(),
    }
}

/// 管理员初始化或重置密码前统一从 `--password`、`ERP_ADMIN_PASSWORD` 或终端取得密码。
/// 交互失败时在连接目标数据库之前终止命令。
pub fn resolve_password(flag: Option<String>) -> Result<String> {
    take_password(select_password_source(flag, std::env::var(PASSWORD_ENV).ok()))
}

// 选择密码来源时把空字符串视为缺省；其他值原样保留，不替用户裁剪空白。
fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|item| !item.is_empty())
}

// 没有非交互密码来源时，让运维人员输入并确认新密码；两次不一致时中止操作，防止误设。
fn prompt_confirmed_password() -> Result<String> {
    let first = prompt_once("请输入密码: ")?;
    let second = prompt_once("请再次输入密码: ")?;
    if first != second {
        return Err(Error::Usage("两次输入的密码不一致".to_string()));
    }

    Ok(first)
}

// 初次输入和确认共用同一个不回显的终端入口；终端读取失败时按命令用法错误处理。
fn prompt_once(prompt: &str) -> Result<String> {
    rpassword::prompt_password(prompt).map_err(|error| Error::Usage(format!("无法读取密码: {error}")))
}

#[cfg(test)]
mod tests {
    use super::{PasswordSource, select_password_source};

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
        assert_eq!(select_password_source(Some(String::new()), Some(String::new())), PasswordSource::Prompt);
    }
}
