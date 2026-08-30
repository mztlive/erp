use chrono::Local;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use entities::money::Quantity;

use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// 按主键读取未删除实体。
///
/// # 参数
/// * `db` - 数据库
/// * `collection_name` - 集合名
/// * `id` - 实体主键
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回匹配实体；不存在时返回 `None`。
///
/// # 错误
/// MongoDB 查询失败时返回错误。
pub(super) async fn active_entity_by_id<T>(
    db: &Database,
    collection_name: &str,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<T>>
where
    T: for<'de> Deserialize<'de> + Serialize + Send + Sync,
{
    mongo_ops::find_one(
        &db.collection::<T>(collection_name),
        doc! { "id": id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
        executor,
    )
    .await
}

/// 按主键集合批量读取未删除实体。
///
/// # 参数
/// * `db` - 数据库
/// * `collection_name` - 集合名
/// * `ids` - 主键集合
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回全部命中实体；空主键集合直接返回空列表。
///
/// # 错误
/// MongoDB 查询或游标读取失败时返回错误。
pub(super) async fn entities_by_ids<T>(
    db: &Database,
    collection_name: &str,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<T>>
where
    T: for<'de> Deserialize<'de> + Serialize + Send + Sync,
{
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    mongo_ops::find_many(
        &db.collection::<T>(collection_name),
        doc! {
            "id": { "$in": ids },
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        },
        FindOptions::default(),
        executor,
    )
    .await
}

/// 把 ID newtype 集合转为字符串集合（用于 `$in` 查询）。
///
/// # 参数
/// * `ids` - ID newtype 集合
///
/// # 返回
/// 返回字符串集合。
pub(super) fn ids_to_strings<T: AsRef<str>>(ids: &[T]) -> Vec<String> {
    ids.iter().map(|id| id.as_ref().to_string()).collect()
}

/// 按给定字段 `$in` 批量读取实体（空集合直接返回空列表）。
pub(super) async fn find_by_field_in<T>(
    db: &Database,
    collection_name: &str,
    field: &str,
    values: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<T>>
where
    T: for<'de> Deserialize<'de> + Serialize + Send + Sync,
{
    if values.is_empty() {
        return Ok(Vec::new());
    }
    let collection = db.collection::<T>(collection_name);
    mongo_ops::find_many(
        &collection,
        doc! {
            field: { "$in": values },
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        },
        FindOptions::default(),
        executor,
    )
    .await
}

/// 把 `Quantity` 转为 BSON `Decimal128`（存储形态由 P0 固化，不做任何舍入/换算）。
///
/// `bson::serialize_to_bson` 的人性化序列化器会把数量写成字符串，无法参与
/// `$inc`/`$gte`；这里直接构造 Decimal128，确保与实体持久化形态一致。
///
/// # 参数
/// * `quantity` - 定点数量
///
/// # 返回
/// 返回 Decimal128 BSON 值。
///
/// # 错误
/// 数量无法表示为 Decimal128 时返回错误。
pub(super) fn to_bson(quantity: Quantity) -> Result<Bson> {
    Ok(Bson::Decimal128(quantity.to_string().parse()?))
}

/// 对 `Quantity` 取相反数并转为 Decimal128 BSON（符号翻转，不改变精度与数值语义）。
///
/// 仅用于 `$inc` 的负方向累加；数量类型构造与 BSON 序列化不做任何舍入。
///
/// # 参数
/// * `quantity` - 定点数量
///
/// # 返回
/// Decimal128 返回符号位翻转后的值；其他 BSON 值原样克隆返回。
pub(super) fn negate_bson(quantity: &Bson) -> Bson {
    let Bson::Decimal128(decimal) = quantity else {
        return quantity.clone();
    };
    let mut bytes = decimal.bytes();
    bytes[15] ^= 0x80;
    Bson::Decimal128(mongodb::bson::Decimal128::from_bytes(bytes))
}

/// 构建两个字段同向增加的原子 `$inc` 更新（含 `version` 与 `updated_at` 元数据）。
///
/// # 参数
/// * `quantity` - 增加数量（Decimal128）
/// * `field_a` - 增加字段一
/// * `field_b` - 增加字段二
///
/// # 返回
/// 返回更新条件文档。
pub(super) fn both_inc(quantity: Quantity, field_a: &str, field_b: &str) -> Result<Document> {
    let quantity = to_bson(quantity)?;
    Ok(doc! {
        "$inc": { field_a: &quantity, field_b: &quantity, "version": 1 },
        "$set": { "updated_at": Local::now().timestamp() },
    })
}

/// 构建两个字段同向减少的原子 `$inc` 更新（含 `version` 与 `updated_at` 元数据）。
///
/// # 参数
/// * `quantity` - 减少数量（Decimal128）
/// * `field_a` - 减少字段一
/// * `field_b` - 减少字段二
///
/// # 返回
/// 返回更新条件文档。
pub(super) fn both_dec(quantity: Bson, field_a: &str, field_b: &str) -> Result<Document> {
    Ok(doc! {
        "$inc": { field_a: negate_bson(&quantity), field_b: negate_bson(&quantity), "version": 1 },
        "$set": { "updated_at": Local::now().timestamp() },
    })
}

/// 构建一个字段增加、另一个字段减少的原子 `$inc` 更新（含 `version` 与 `updated_at` 元数据）。
///
/// # 参数
/// * `quantity` - 增加数量（Decimal128）
/// * `increase_field` - 增加字段
/// * `decrease_field` - 减少字段
///
/// # 返回
/// 返回更新条件文档。
pub(super) fn cross_inc(quantity: Bson, increase_field: &str, decrease_field: &str) -> Result<Document> {
    Ok(doc! {
        "$inc": { increase_field: &quantity, decrease_field: negate_bson(&quantity), "version": 1 },
        "$set": { "updated_at": Local::now().timestamp() },
    })
}

/// 构建排序文档（字段名白名单映射）。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或不在白名单内时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
/// * `allowed` - 允许的排序字段白名单
///
/// # 返回
/// 返回排序条件文档。
pub(super) fn sort_doc(sort_by: Option<&str>, sort_ascending: bool, allowed: &[&str]) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by
        .filter(|field| allowed.contains(field))
        .unwrap_or("created_at");
    doc! { field: direction }
}

#[cfg(test)]
mod tests {
    use super::{ids_to_strings, negate_bson, sort_doc};
    use mongodb::bson::{doc, Bson};
    use std::str::FromStr;

    use entities::ids::WarehouseId;

    #[test]
    fn sort_doc_maps_whitelisted_fields_and_defaults_otherwise() {
        let allowed = ["occurred_at", "recorded_at"];
        assert_eq!(sort_doc(None, false, &allowed), doc! { "created_at": -1 });
        assert_eq!(
            sort_doc(Some("occurred_at"), true, &allowed),
            doc! { "occurred_at": 1 }
        );
        assert_eq!(
            sort_doc(Some("任意字段"), false, &allowed),
            doc! { "created_at": -1 },
            "白名单外的字段名回落默认排序"
        );
    }

    #[test]
    fn negate_bson_flips_sign_without_touching_magnitude() {
        let positive = Bson::Decimal128(mongodb::bson::Decimal128::from_str("12.345").unwrap());
        let negative = negate_bson(&positive);
        assert_eq!(
            negative,
            Bson::Decimal128(mongodb::bson::Decimal128::from_str("-12.345").unwrap())
        );
        assert_eq!(negate_bson(&negative), positive, "两次取反恢复原值");
    }

    #[test]
    fn ids_to_strings_converts_newtype_collection() {
        let ids = vec![WarehouseId::new("wh-1"), WarehouseId::new("wh-2")];
        assert_eq!(ids_to_strings(&ids), vec!["wh-1".to_string(), "wh-2".to_string()]);
        assert!(ids_to_strings::<WarehouseId>(&[]).is_empty());
    }
}
