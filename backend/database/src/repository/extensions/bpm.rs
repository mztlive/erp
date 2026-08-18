//! BPM 模型仓储访问器。P2 填充集合访问方法。

/// BPM 仓储访问器。P0 仅注册 trait，不含任何可成功执行的集合操作。
pub trait BpmExt {}

impl BpmExt for mongodb::Database {}
