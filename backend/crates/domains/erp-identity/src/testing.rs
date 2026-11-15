//! 仅测试支持 feature 提供的拒绝式设施；不得用于应用装配。
use std::sync::Arc;

use crate::AuthorizationService;

/// 构造始终拒绝取证的测试授权服务。
/// # 参数
/// 无。
/// # 返回
/// 用于验证输入校验顺序的不可用设施；不构造生产绑定，不授予任何访问。
/// # 错误
/// 每次授权或资格取证均返回错误；不连接数据库。
pub fn unavailable_authorization() -> Arc<AuthorizationService> {
    AuthorizationService::unavailable_for_test()
}
