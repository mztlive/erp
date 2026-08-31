//! 域 D28 `card_instance`：mall_consumption_cutover、mall_card_instance(+_correction)、
//! mall_balance_snapshot（页面：W28）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype、`entities::money` 定点类型
//! 与 `common` 基元。字段字典见数据模型 §6.17；公共字段归属按 §4.3 判定：
//! - `mall_consumption_cutover` 是上线切换配置（准备/已启用状态机）→ 按字典精确建模，
//!   `enabled_at` 一经启用不可修改（§6.17）；
//! - `mall_card_instance` 是卡实例稳定基线，首次成功写入后不可覆盖（§6.17）→ 只 `new()`，
//!   不提供更新；
//! - `mall_card_instance_correction` / `mall_balance_snapshot` 是不可变纠错事实与快照
//!   → 只 `new()`。
//!
//! D28 敏感字段禁令（P1 §2.1 + 数据模型 §4.5.6）：ERP 不保存卡号、卡密、卡实例绑定
//! 手机号及其可逆映射。本域全部实体只承载 `opaque_instance_ref`（不可反推卡号、卡密的
//! 稳定引用），实体结构不出现任何卡号/卡密/手机号字段，并由内联测试做静态断言。
pub mod balance_snapshot;
pub mod baseline;
pub mod cutover;
pub mod mall_card_instance;

pub use crate::ids::{
    MallBalanceSnapshotId, MallCardInstanceCorrectionId, MallCardInstanceId, MallConsumptionCutoverId,
};
pub use balance_snapshot::*;
pub use baseline::*;
pub use cutover::*;
pub use mall_card_instance::*;
