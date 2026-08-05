//! 定点数值（P0-1.3 共享基元任务）。
//!
//! `Amount`(2) / `UnitPrice`(4) / `Quantity`(6) / `Rate`(6)，BSON 形态固定 `Decimal128`，
//! 唯一舍入实现 `round_to_cent` 与行金额三元组计算 `line_amounts`。
