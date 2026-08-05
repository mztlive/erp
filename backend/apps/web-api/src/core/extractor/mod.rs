use serde::Deserialize;

pub use entities::AccountKind;

/// ERP 操作人员 ID 包装类型。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UserID(pub String);

/// 账号包装类型
#[derive(Debug, Clone, Deserialize)]
pub struct Account(pub String);

impl From<UserID> for String {
    /// 将 UserID 包装转换为 String。
    ///
    /// # 返回值
    /// 返回内部的字符串值。
    fn from(user_id: UserID) -> Self {
        user_id.0
    }
}

impl From<Account> for String {
    /// 将 Account 包装转换为 String。
    ///
    /// # 返回值
    /// 返回内部的字符串值。
    fn from(account: Account) -> Self {
        account.0
    }
}
