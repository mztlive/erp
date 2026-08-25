//! 定点数值（P0-1.3 共享基元任务，P0 冻结后全域唯一实现）。
//!
//! `Amount`(2) / `UnitPrice`(4) / `Quantity`(6) / `Rate`(6) 四个 newtype 封装
//! `rust_decimal::Decimal`：
//! - 禁止外部直接构造裸 `Decimal`，只能通过 `FromStr` / `TryFrom<Decimal>` 构造；
//! - 构造时校验小数位：超过类型上限（含有效位）一律拒绝，不静默舍入；
//! - 唯一舍入实现是 [`round_to_cent`]（银行家舍入到分，见 conventions.md 第 5 节），
//!   行金额由 [`line_amounts`] 统一计算，满足 `gross = net + tax` 精确恒等。
//!
//! # BSON 持久化形态（Decimal128）
//!
//! 自定义 `Serialize`/`Deserialize`：
//! - human-readable（serde_json，HTTP 传输层）：序列化为字符串、反序列化从字符串，
//!   满足 P0-4.1「金额与数量以字符串传输」；
//! - 非 human-readable（mongodb 驱动 raw/to_vec 序列化与 `bson::to_document_with_options`
//!   `human_readable(false)`）：委托 `bson::Decimal128` 的 serde 形态（扩展文档
//!   `{"$numberDecimal": ...}`，由 bson 的 `from_extended_document` / raw 序列化器还原为
//!   真正的 BSON `Decimal128` 变体；已核对 bson 2.15 `ser/decimal128` 源码）；
//! - 反序列化同时接受字符串与 Decimal128 变体（bson 的 `Decimal128Access` 以
//!   `$numberDecimalBytes` + 原始 16 字节形式暴露，见 money.rs 实现）。

use std::fmt;
use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{de, de::MapAccess, de::Visitor, Deserialize, Deserializer, Serialize, Serializer};

use crate::errors::{Error, Result};

/// 金额小数位（人民币元）。
const AMOUNT_SCALE: u32 = 2;

/// 批量生成一个定点数值 newtype。
///
/// 提供 `FromStr` / `TryFrom<Decimal>`（校验并归整小数位）、`to_decimal`、
/// `Display`/`Debug`/`Copy`/`Clone`/`PartialEq`/`Eq`/`PartialOrd`/`Ord`/`Hash`，
/// 以及 human-readable 字符串 / Decimal128 双形态 serde。
macro_rules! decimal_newtype {
    ($name:ident, $scale:literal) => {
        /// 定点数值（小数位上限 `$scale` 位）。
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(Decimal);

        impl $name {
            /// 返回底层 `Decimal`（小数位已受类型约束）。
            ///
            /// # 返回
            /// 返回封装值的 `Decimal` 副本。
            pub fn to_decimal(self) -> Decimal {
                self.0
            }
        }

        impl TryFrom<Decimal> for $name {
            type Error = Error;

            /// 从 `Decimal` 构造并校验小数位。
            ///
            /// # 参数
            /// * `value` - 原始定点数值
            ///
            /// # 返回
            /// 小数位（去尾零后）不超过上限时返回 `Ok`，否则返回 `Err`。
            ///
            /// # 错误
            /// 有效小数位超过 `$scale` 时返回 `LogicError`（禁止静默舍入；
            /// 需要舍入请显式使用 [`round_to_cent`]）。
            fn try_from(value: Decimal) -> Result<Self> {
                if value.normalize().scale() > $scale {
                    return Err(Error::from(format!(
                        "{} 有效小数位最多 {} 位",
                        stringify!($name),
                        $scale
                    )));
                }
                Ok(Self(value))
            }
        }

        impl FromStr for $name {
            type Err = Error;

            /// 从字符串解析并校验小数位。
            ///
            /// # 参数
            /// * `value` - 定点数值字符串（如 `"123.45"`）
            ///
            /// # 返回
            /// 解析成功且小数位合规时返回 `Ok`，否则返回 `Err`。
            ///
            /// # 错误
            /// 字符串不是合法数字或有效小数位超过 `$scale` 时返回 `LogicError`。
            fn from_str(value: &str) -> Result<Self> {
                let decimal =
                    Decimal::from_str(value).map_err(|_| Error::from(format!("{value} 不是合法定点数值")))?;
                Self::try_from(decimal)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "{}", self.0)
            }
        }

        impl Serialize for $name {
            /// 序列化：human-readable 输出字符串；非 human-readable（mongodb
            /// 驱动 raw 序列化）委托 `bson::Decimal128` 的 serde 形态。
            fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
                if serializer.is_human_readable() {
                    serializer.serialize_str(&self.0.to_string())
                } else {
                    let decimal128 =
                        bson::Decimal128::from_str(&self.0.to_string()).map_err(serde::ser::Error::custom)?;
                    decimal128.serialize(serializer)
                }
            }
        }

        impl<'de> Deserialize<'de> for $name {
            /// 反序列化：接受字符串（JSON）与 Decimal128 变体（BSON，经
            /// `$numberDecimal` / `$numberDecimalBytes` 扩展形态）。
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
                let decimal = deserializer.deserialize_any(MoneyVisitor)?;
                Self::try_from(decimal).map_err(de::Error::custom)
            }
        }
    };
}

decimal_newtype!(Amount, 2);
decimal_newtype!(UnitPrice, 4);
decimal_newtype!(Quantity, 6);
decimal_newtype!(Rate, 6);

impl Amount {
    /// 两个金额相加（精确，不触发舍入）。
    pub fn checked_add(self, other: Amount) -> Amount {
        Amount(self.0 + other.0)
    }

    /// 两个金额相减（精确，不触发舍入）。
    pub fn checked_sub(self, other: Amount) -> Amount {
        Amount(self.0 - other.0)
    }
}

/// 唯一舍入实现：四舍五入到分（银行家舍入，`MidpointNearestEven`，
/// conventions.md 第 5 节固定规则）。
///
/// # 参数
/// * `value` - 任意定点数值
///
/// # 返回
/// 舍入到 2 位小数后的 `Decimal`（`.005` 边界按银行家规则取偶数位，
/// 负数按绝对值对称处理）。
pub fn round_to_cent(value: Decimal) -> Decimal {
    value.round_dp(AMOUNT_SCALE)
}

/// 计算一行销售/采购明细的金额三元组。
///
/// 规则（数据模型 4.2 铁律 1）：
/// - `gross` = 单价 × 数量，舍入到分；
/// - `tax` = `gross` × 税率，舍入到分；
/// - `net` = `gross` − `tax`（精确）。
///
/// 恒等式：`gross = net + tax` 精确成立。
///
/// # 参数
/// * `unit_price` - 含税单价
/// * `quantity` - 数量
/// * `tax_rate` - 税率（如 `0.13` 表示 13%）
///
/// # 返回
/// 返回 `(gross, net, tax)` 三元组，均为 `Amount`。
pub fn line_amounts(unit_price: UnitPrice, quantity: Quantity, tax_rate: Rate) -> (Amount, Amount, Amount) {
    let gross = round_to_cent(unit_price.to_decimal() * quantity.to_decimal());
    let tax = round_to_cent(gross * tax_rate.to_decimal());
    let net = gross - tax;

    // round_to_cent 保证小数位 ≤ 2，以下构造不会失败。
    let gross = Amount::try_from(gross).expect("gross 舍入后小数位不超过 2 位");
    let tax = Amount::try_from(tax).expect("tax 舍入后小数位不超过 2 位");
    let net = Amount::try_from(net).expect("net 舍入后小数位不超过 2 位");
    (gross, net, tax)
}

/// 定点数值的反序列化访问器：接受字符串或扩展文档形态。
struct MoneyVisitor;

impl<'de> Visitor<'de> for MoneyVisitor {
    type Value = Decimal;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("定点小数字符串，或 $numberDecimal/$numberDecimalBytes 扩展形态")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<Decimal, E> {
        Decimal::from_str(value).map_err(E::custom)
    }

    fn visit_string<E: de::Error>(self, value: String) -> std::result::Result<Decimal, E> {
        self.visit_str(&value)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<Decimal, A::Error> {
        let mut value: Option<String> = None;
        let mut bytes: Option<Vec<u8>> = None;

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "$numberDecimal" => value = Some(map.next_value()?),
                "$numberDecimalBytes" => bytes = Some(map.next_value_seed(D128BytesSeed)?),
                _ => return Err(de::Error::custom(format!("未知金额扩展键：{key}"))),
            }
        }

        if let Some(value) = value {
            return Decimal::from_str(&value).map_err(de::Error::custom);
        }
        if let Some(bytes) = bytes {
            return decimal_from_d128_bytes(bytes).map_err(de::Error::custom);
        }
        Err(de::Error::custom("缺少金额值"))
    }
}

/// 读取 Decimal128 原始 16 字节的 DeserializeSeed。
///
/// bson 的 `Decimal128Access` 以 `visit_bytes` 暴露原始字节（`serde_bytes` 形态），
/// 标准 `Vec<u8>` 的 Deserialize 只接受序列形态，这里自行接住字节访问。
struct D128BytesSeed;

impl<'de> de::DeserializeSeed<'de> for D128BytesSeed {
    type Value = Vec<u8>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> std::result::Result<Vec<u8>, D::Error> {
        struct BytesVisitor;

        impl<'de> Visitor<'de> for BytesVisitor {
            type Value = Vec<u8>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("Decimal128 原始 16 字节")
            }

            fn visit_bytes<E: de::Error>(self, value: &[u8]) -> std::result::Result<Vec<u8>, E> {
                Ok(value.to_vec())
            }

            fn visit_byte_buf<E: de::Error>(self, value: Vec<u8>) -> std::result::Result<Vec<u8>, E> {
                Ok(value)
            }
        }

        deserializer.deserialize_any(BytesVisitor)
    }
}

/// 将 BSON Decimal128 的 16 字节原始表示转回 `Decimal`。
///
/// # 参数
/// * `bytes` - Decimal128 原始 16 字节（bson `Decimal128Access` 以字节暴露）
///
/// # 返回
/// 解析成功返回 `Ok(Decimal)`，字节长度非法或解析失败返回 `Err(String)`。
fn decimal_from_d128_bytes(bytes: Vec<u8>) -> std::result::Result<Decimal, String> {
    let bytes: [u8; 16] = bytes
        .try_into()
        .map_err(|_| "Decimal128 必须为 16 字节".to_string())?;
    let decimal128 = bson::Decimal128::from_bytes(bytes);
    Decimal::from_str(&decimal128.to_string()).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct MoneyDoc {
        amount: Amount,
        unit_price: UnitPrice,
        quantity: Quantity,
        rate: Rate,
    }

    fn sample_doc() -> MoneyDoc {
        MoneyDoc {
            amount: Amount::from_str("123.45").unwrap(),
            unit_price: UnitPrice::from_str("9.9900").unwrap(),
            quantity: Quantity::from_str("3.000000").unwrap(),
            rate: Rate::from_str("0.130000").unwrap(),
        }
    }

    /// 小数位上限校验：超位（含有效尾数）拒绝构造，禁止静默舍入。
    #[test]
    fn scale_limit_is_enforced() {
        assert!(Amount::from_str("1.23").is_ok());
        assert!(Amount::from_str("1.230000").is_ok(), "尾零不增加有效精度");
        assert!(Amount::from_str("1.234").is_err());
        assert!(Amount::try_from(Decimal::new(123, 3)).is_err());

        assert!(UnitPrice::from_str("1.2345").is_ok());
        assert!(UnitPrice::from_str("1.23456").is_err());

        assert!(Quantity::from_str("1.234567").is_ok());
        assert!(Quantity::from_str("1.2345678").is_err());

        assert!(Rate::from_str("0.130000").is_ok());
        assert!(Rate::from_str("0.1300001").is_err());

        assert!(Amount::from_str("not a number").is_err());
    }

    /// 舍入边界：.005、负数与大额（银行家舍入，MidpointNearestEven）。
    #[test]
    fn round_to_cent_boundaries() {
        // 0.005 → 0.00（偶数位）；0.015 → 0.02；0.025 → 0.02（保留偶数）
        assert_eq!(round_to_cent(Decimal::new(5, 3)), Decimal::new(0, 2));
        assert_eq!(round_to_cent(Decimal::new(15, 3)), Decimal::new(2, 2));
        assert_eq!(round_to_cent(Decimal::new(25, 3)), Decimal::new(2, 2));

        // 负数按绝对值对称处理
        assert!(round_to_cent(Decimal::new(-5, 3)).is_zero());
        assert_eq!(round_to_cent(Decimal::new(-15, 3)), Decimal::new(-2, 2));

        // 大额：9999999999.999 → 10000000000.00
        assert_eq!(
            round_to_cent(Decimal::new(9_999_999_999_999, 3)),
            Decimal::new(1_000_000_000_000, 2)
        );
    }

    /// 行金额一致性：gross = net + tax 精确成立，各分量均舍入到分。
    #[test]
    fn line_amounts_consistency() {
        let cases = [
            ("9.9900", "3.000000", "0.130000"), // 29.97 → 税 3.90，净额 26.07
            ("100.0000", "2.500000", "0.060000"),
            ("0.3333", "3.000000", "0.130000"), // 0.9999 → gross 1.00
            ("999999.9900", "0.000001", "0.130000"),
        ];

        for (price, quantity, rate) in cases {
            let unit_price = UnitPrice::from_str(price).unwrap();
            let quantity = Quantity::from_str(quantity).unwrap();
            let tax_rate = Rate::from_str(rate).unwrap();

            let (gross, net, tax) = line_amounts(unit_price, quantity, tax_rate);
            assert_eq!(
                gross.to_decimal(),
                net.to_decimal() + tax.to_decimal(),
                "gross = net + tax 对 {price} × {quantity} @ {rate} 不成立"
            );
            assert_eq!(gross.to_decimal().scale(), 2);
        }
    }

    /// 确定性用例：9.99 × 3 @ 13% = (29.97, 26.07, 3.90)。
    #[test]
    fn line_amounts_known_case() {
        let (gross, net, tax) = line_amounts(
            UnitPrice::from_str("9.9900").unwrap(),
            Quantity::from_str("3.000000").unwrap(),
            Rate::from_str("0.130000").unwrap(),
        );
        assert_eq!(gross.to_decimal(), Decimal::new(2997, 2));
        assert_eq!(net.to_decimal(), Decimal::new(2607, 2));
        assert_eq!(tax.to_decimal(), Decimal::new(390, 2));
    }

    /// serde_json 往返：金额/单价/数量/税率均为字符串。
    #[test]
    fn json_roundtrip_as_strings() {
        let doc = sample_doc();
        let json = serde_json::to_string(&doc).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["amount"], serde_json::json!("123.45"));
        assert_eq!(value["unit_price"], serde_json::json!("9.9900"));
        assert_eq!(value["quantity"], serde_json::json!("3.000000"));
        assert_eq!(value["rate"], serde_json::json!("0.130000"));

        let back: MoneyDoc = serde_json::from_str(&json).unwrap();
        assert_eq!(back, doc);
    }

    /// Decimal128 往返（非 human-readable，mongodb 驱动 wire 路径）：
    /// 序列化为原始字节后，BSON 中必须为 Decimal128 变体，且 to_vec/from_slice
    /// 与 deserialize_from_document 两条路径均可还原。
    ///
    /// 说明：bson 2.15 的 `serialize_to_document`（默认选项）以 human-readable 形态工作
    /// （bson 文档明确：`serialize_to_document` 的 Serializer 表现为 human readable，
    /// 与 `to_vec` 不同），因此这里用非 human-readable 的 `to_vec`/`from_slice`
    /// 精确对应 mongodb 驱动的持久化路径（`human_readable(false)` 选项的
    /// builder 在 bson 2.15 已废弃）；`deserialize_from_document` 默认形态也兼容
    /// Decimal128 变体。
    #[test]
    fn decimal128_wire_roundtrip() {
        use bson::Bson;

        let doc = sample_doc();

        let bytes = bson::serialize_to_vec(&doc).unwrap();
        let wire_doc: bson::Document = bson::deserialize_from_slice(&bytes).unwrap();
        assert!(matches!(wire_doc.get("amount"), Some(Bson::Decimal128(_))));
        assert!(matches!(wire_doc.get("unit_price"), Some(Bson::Decimal128(_))));
        assert!(matches!(wire_doc.get("quantity"), Some(Bson::Decimal128(_))));
        assert!(matches!(wire_doc.get("rate"), Some(Bson::Decimal128(_))));

        let back_from_wire: MoneyDoc = bson::deserialize_from_slice(&bytes).unwrap();
        assert_eq!(back_from_wire, doc);

        let bson_doc = bson::serialize_to_document(&doc).unwrap();
        let back_from_doc: MoneyDoc = bson::deserialize_from_document(bson_doc).unwrap();
        assert_eq!(back_from_doc, doc);
    }

    /// Amount 精确加减：不触发舍入，恒等式可用 `Amount::checked_add` 复现。
    #[test]
    fn amount_add_sub_is_exact() {
        let gross = Amount::from_str("29.97").unwrap();
        let tax = Amount::from_str("3.90").unwrap();
        assert_eq!(gross.checked_sub(tax), Amount::from_str("26.07").unwrap());
        assert_eq!(Amount::from_str("26.07").unwrap().checked_add(tax), gross);
    }
}
