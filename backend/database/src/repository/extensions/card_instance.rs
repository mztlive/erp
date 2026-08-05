//! 域 D28 `card_instance`：mall_consumption_cutover、mall_card_instance(+_correction)、mall_balance_snapshot（页面：W28）。P0 预声明空 trait；P2 在本文件填充仓储访问器。

/// 域 D28 仓储访问器（P2 填充）。
pub trait CardInstanceExt: Sized {}

impl CardInstanceExt for mongodb::Database {}
