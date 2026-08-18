//! ERP 审批集成仓储访问器。P2 填充快照与 outbox 访问方法。

/// 审批集成仓储访问器。P0 仅注册 trait，不含任何可成功执行的集合操作。
pub trait ApprovalIntegrationExt {}

impl ApprovalIntegrationExt for mongodb::Database {}
