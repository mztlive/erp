//! 域 D21 `returns`：sales_return_case、sales_return_line、purchase_return_order、
//! purchase_return_line、customer_refund、supplier_refund、receipt_reversal、
//! payment_reversal（页面：W05、W09、W11、W12）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 common 基元。
//! 公共字段归属按 §4.3 判定：
//! - `sales_return_case` / `purchase_return_order` 是处理单主表类 → 组合
//!   [`crate::common::stable::StableBase`]；
//! - 其余表是行项或正式事实（§4.5 不设业务软删除），按 §6.11 字段字典建模；
//!   §6.11 财务纠错表未含 FactBase 全部语义字段，因此不组合 FactBase，仅用
//!   `BaseModel` 持久化元数据；
//! - 状态机 §7.5 覆盖退款与冲正。客户退款已收敛为
//!   DRAFT→IN_APPROVAL→POSTED→REVERSED；其余资金纠错单据仍待各自子阶段
//!   删除 `PENDING_REVIEW`。退货/退货单状态是第 7 章未定义的固定枚举，
//!   不发明状态机；
//! - 财务纠错共同不变量（§6.11）：经办人与复核人不得相同；过账后原事实保留，
//!   追加反向分录；同一原事实的累计有效冲正不得超过原金额（跨实体部分归 P3）。

pub mod customer_refund;
pub mod payment_reversal;
pub mod purchase_return_line;
pub mod purchase_return_order;
pub mod receipt_reversal;
pub mod sales_return_case;
pub mod sales_return_line;
pub mod supplier_refund;

pub use customer_refund::*;
pub use payment_reversal::*;
pub use purchase_return_line::*;
pub use purchase_return_order::*;
pub use receipt_reversal::*;
pub use sales_return_case::*;
pub use sales_return_line::*;
pub use supplier_refund::*;
