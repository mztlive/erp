//! 域 D17 `inventory`：stock_movement、stock_balance、stock_reservation(+_entry)、stock_adjustment(+_line)（页面：W10）。P0 骨架占位；P3 填充服务与事务（Service 不跨域依赖，跨域只调对方 Repository）。

pub mod dto;
