//! 应收应付共用的核销/开票进度条件更新 helper。
//!
//! `receivable::account` 与 `payable::account::write` 原先各持一份字节一致的
//! `amount_bson`／`progress_pipeline`；现收敛到本模块唯一实现，两域经
//! `pub(super) use` 转发，保持原有 `super::account::`／`super::write::`
//! 引用路径与仓储外可见性不变。

use erp_core::money::Amount;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::Result;

/// 将金额按 BSON Decimal128 形态转换（仓储层禁止任何舍入或换算）。
///
/// `bson::serialize_to_bson` 默认走 human-readable 字符串形态，与实体持久化的
/// Decimal128 形态不一致；这里直接构造 Decimal128，确保 `$add`/`$lte`
/// 等表达式与库内金额类型一致。
///
/// # 参数
/// * `amount` - 定点金额
///
/// # 返回
/// 返回 Decimal128 形态的 BSON 值。
///
/// # 错误
/// 金额无法表示为 Decimal128 时返回错误。
pub(super) fn amount_bson(amount: &Amount) -> Result<Bson> {
    Ok(Bson::Decimal128(amount.to_string().parse()?))
}

/// 构建核销/收票进度条件更新管道。
///
/// 在单条 MongoDB 原子更新内重算进度字段、开放余额与派生状态：
/// 增加方向 `progress = progress + amount`、`balance = total - progress`；
/// 减少方向 `progress = progress - amount`、`balance = total - progress`。
/// 状态仅由开放余额派生：增加后开放余额归零为 `settled`，减少后已核销归零为
/// `open`，其余为 `partially_settled`；收票/开票进度不派生状态。
///
/// # 参数
/// * `progress_field` - 进度字段名（`settled_total` 或 `invoiced_total`）
/// * `balance_field` - 开放余额字段名（`open_total` 或 `open_invoiceable_total`）
/// * `amount` - 本次金额（正数，Decimal128 形态）
/// * `increase` - `true` 为增加进度，`false` 为冲减进度
/// * `updated_by` - 本次更新执行人
///
/// # 返回
/// 返回聚合管道更新文档。
pub(super) fn progress_pipeline(
    progress_field: &str,
    balance_field: &str,
    amount: &Bson,
    increase: bool,
    updated_by: &str,
) -> Vec<Document> {
    let total_field = if progress_field == "settled_total" { "gross_total" } else { "invoiceable_total" };
    let new_progress = if increase {
        doc! { "$add": ["$" .to_owned() + progress_field, amount] }
    } else {
        doc! { "$subtract": ["$" .to_owned() + progress_field, amount] }
    };
    let new_balance = doc! { "$subtract": ["$" .to_owned() + total_field, &new_progress] };
    let mut set = doc! {
        progress_field: &new_progress,
        balance_field: &new_balance,
        "updated_by": updated_by,
        "version": { "$add": ["$version", 1] },
        "updated_at": chrono::Local::now().timestamp(),
    };
    if progress_field == "settled_total" {
        set.insert(
            "status",
            doc! {
                "$cond": [
                    { "$eq": [&new_balance, { "$toDecimal": "0" }] },
                    "settled",
                    {
                        "$cond": [
                            { "$eq": [&new_progress, { "$toDecimal": "0" }] },
                            "open",
                            "partially_settled",
                        ]
                    },
                ],
            },
        );
    }
    vec![doc! { "$set": set }]
}
