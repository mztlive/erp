//! 域 D12 `contract`：contract、contract_revision（页面：W04）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D12 仓储访问器（P2 填充）。
pub trait ContractExt: Sized {}

impl ContractExt for mongodb::Database {}
