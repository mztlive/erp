//! 域 D07 `party`：party、party_revision、party_contact、party_address、party_tax_profile、party_bank_account（页面：W14、W03）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D07 仓储访问器（P2 填充）。
pub trait PartyExt: Sized {}

impl PartyExt for mongodb::Database {}
