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
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::errors::{Error, Result};

/// 金额小数位（人民币元）。
const AMOUNT_SCALE: u32 = 2;

/// 批量生成一个定点数值 newtype。
///
/// 提供 `FromStr` / `TryFrom<Decimal>`（校验并归整小数位）、`to_decimal`、
/// `Display`/`Debug`/`Copy`/`Clone`/`PartialEq`/`Eq`/`PartialOrd`/`Ord`/`Hash`，
/// 以及 human-readable 字符串 / Decimal128 双形态 serde。
macro_rules! decimal_newtype {
    ($name:ident, $label:literal, $scale:literal) => {
        /// 定点数值（小数位上限 `$scale` 位）。
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(Decimal);

        impl $name {
            /// 在领域计算需要底层定点运算时取 `Decimal` 副本；原值的有效小数位已由对应类型约束。
            pub fn to_decimal(self) -> Decimal {
                self.0
            }
        }

        impl TryFrom<Decimal> for $name {
            type Error = Error;

            // 将计算结果收回定点类型前，以去尾零后的有效小数位检查上限，保留原 `Decimal`。超出类型精度即拒绝；
            // 金额需舍入时显式调用 `round_to_cent`。
            fn try_from(value: Decimal) -> Result<Self> {
                if value.normalize().scale() > $scale {
                    return Err(Error::from(format!("{}最多保留 {} 位小数，请修改后重试", $label, $scale)));
                }
                Ok(Self(value))
            }
        }

        impl FromStr for $name {
            type Err = Error;

            // 将表单或导入的十进制文本解析成业务数值（如 `123.45`），不经浮点转换；
            // 无效数字或超过类型的有效小数位限制时返回业务值错误。
            fn from_str(value: &str) -> Result<Self> {
                let decimal =
                    Decimal::from_str(value).map_err(|_| Error::from(concat!("请填写有效的", $label)))?;
                Self::try_from(decimal)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "{}", self.0)
            }
        }

        impl Serialize for $name {
            // HTTP 等可读格式用十进制字符串避免浮点精度丢失；MongoDB 原始序列化交给 `bson::Decimal128`。
            // 转换或写入失败返回 serde 错误。
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
            // 读入 HTTP 数值字符串或 BSON Decimal128 扩展形态，再检查对应类型的有效小数位上限。
            // JSON 数字、非法表示和超位均拒绝。
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
                let decimal = deserializer.deserialize_any(MoneyVisitor)?;
                Self::try_from(decimal).map_err(de::Error::custom)
            }
        }
    };
}

decimal_newtype!(Amount, "金额", 2);
decimal_newtype!(UnitPrice, "单价", 4);
decimal_newtype!(Quantity, "数量", 6);
decimal_newtype!(Rate, "比率", 6);

impl Amount {
    /// 为折叠汇总金额提供精确的 `0.00` 起点，直接构造而不解析字符串；用 [`Amount::checked_add`] 累加。
    pub fn zero() -> Amount {
        Amount(Decimal::new(0, 2))
    }

    /// 汇总或核对两个金额时直接相加，不触发业务舍入。保留历史方法名；
    /// 它直接返回 `Amount`，不是标准 `checked_*` 的 `Option` 合同，改用 `add` 则与 `Add` 同名并触发 Clippy。
    ///
    /// # Panics
    /// 和超出 `Decimal` 可表示范围时 panic，不回绕。
    pub fn checked_add(self, other: Amount) -> Amount {
        Amount(self.0 + other.0)
    }

    /// 需要从含税金额扣除税额等场景时直接相减，不重复舍入。
    /// 沿用 [`Amount::checked_add`] 的历史命名和非 `Option` 返回合同。
    ///
    /// # Panics
    /// 差超出 `Decimal` 可表示范围时 panic，不回绕。
    pub fn checked_sub(self, other: Amount) -> Amount {
        Amount(self.0 - other.0)
    }
}

/// 统一把定点金额舍入到分（conventions.md 第 5 节）。恰在中点时取偶数位（`MidpointNearestEven`），
/// 负数按绝对值对称处理。
pub fn round_to_cent(value: Decimal) -> Decimal {
    value.round_dp(AMOUNT_SCALE)
}

/// 销售或采购明细用含税单价乘数量并以 [`round_to_cent`] 舍入，再乘税率（如 `0.13` 表示 13%）并舍入得税额。
/// 净额直接以含税额减税额，保证返回的 `(gross, net, tax)` 精确满足 `gross = net + tax`。
///
/// # Panics
/// 乘法或减法超出 `Decimal` 范围时 panic；调试构建还断言三个结果的有效小数位均不超过两位。
pub fn line_amounts(unit_price: UnitPrice, quantity: Quantity, tax_rate: Rate) -> (Amount, Amount, Amount) {
    let gross = Amount(round_to_cent(unit_price.to_decimal() * quantity.to_decimal()));
    let amounts = fee_amounts(gross, tax_rate);

    // round_to_cent 保证小数位 ≤ 2，直接构造并以 debug 断言显式化不变式。
    debug_assert!(
        [amounts.0, amounts.1, amounts.2].iter().all(|amount| amount.0.normalize().scale() <= AMOUNT_SCALE),
        "round_to_cent 后小数位必须不超过 2 位"
    );
    amounts
}

/// 拆分不带数量的含税费用时，沿用 [`line_amounts`] 的税额口径：含税额乘税率后舍入到分，净额用原含税额减税额。
/// 返回 `(gross, net, tax)`，确保 `gross = net + tax` 精确成立。
///
/// # Panics
/// 含税额乘税率或扣税超出 `Decimal` 范围时 panic。
pub fn fee_amounts(gross: Amount, tax_rate: Rate) -> (Amount, Amount, Amount) {
    let tax = round_to_cent(gross.0 * tax_rate.to_decimal());
    let net = gross.0 - tax;
    (gross, Amount(net), Amount(tax))
}

/// 定点数值的反序列化访问器：接受字符串或扩展文档形态。
struct MoneyVisitor;

impl<'de> Visitor<'de> for MoneyVisitor {
    type Value = Decimal;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("定点小数字符串，或 $numberDecimal/$numberDecimalBytes 扩展形态")
    }

    // HTTP 字符串直接按十进制解析，不经浮点；文本非法时由 serde 报解码错误。
    fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<Decimal, E> {
        Decimal::from_str(value).map_err(E::custom)
    }

    fn visit_string<E: de::Error>(self, value: String) -> std::result::Result<Decimal, E> {
        self.visit_str(&value)
    }

    // 有符号 JSON 整数也不得进入定点数值，复用要求用字符串传输的提示。
    fn visit_i64<E: de::Error>(self, value: i64) -> std::result::Result<Decimal, E> {
        Err(json_integer_rejected(value))
    }

    // 无符号 JSON 整数与有符号整数遵循同一拒绝规则，不因缺少负号而接受。
    fn visit_u64<E: de::Error>(self, value: u64) -> std::result::Result<Decimal, E> {
        Err(json_integer_rejected(value))
    }

    // 拒绝 JSON 浮点，避免以二进制近似值构造本应精确的业务金额。
    fn visit_f64<E: de::Error>(self, value: f64) -> std::result::Result<Decimal, E> {
        Err(E::custom(format!("金额必须用字符串传输，不接受 JSON 浮点数 {value}，见 P0-4.1")))
    }

    // 解码 BSON 扩展表示时按键读取文本或原始字节，两者都有时优先文本。未知键、缺值或无效编码均向 serde 报错。
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

// 为两条整数解码路径统一提示 P0-4.1 的字符串传输约束，并示例如何传该值。
fn json_integer_rejected<E: de::Error>(value: impl fmt::Display) -> E {
    E::custom(format!("金额必须用字符串传输（如 \"{value}\"），见 P0-4.1，不接受 JSON 数字"))
}

/// 读取 Decimal128 原始 16 字节的 DeserializeSeed。
///
/// bson 的 `Decimal128Access` 以 `visit_bytes` 暴露原始字节（`serde_bytes` 形态），
/// 标准 `Vec<u8>` 的 Deserialize 只接受序列形态，这里自行接住字节访问。
struct D128BytesSeed;

impl<'de> de::DeserializeSeed<'de> for D128BytesSeed {
    type Value = Vec<u8>;

    // 通过字节访问而非序列接收 BSON 驱动暴露的 Decimal128 数据；借用切片须复制，已有缓冲区直接移交。
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

// 将 BSON 扩展访问器给出的 16 字节转换为 Decimal128，再将十进制文本读入 `Decimal`；长度或数值不支持时拒绝。
fn decimal_from_d128_bytes(bytes: Vec<u8>) -> std::result::Result<Decimal, String> {
    let bytes: [u8; 16] = bytes.try_into().map_err(|_| "Decimal128 必须为 16 字节".to_string())?;
    let decimal128 = bson::Decimal128::from_bytes(bytes);
    Decimal::from_str(&decimal128.to_string()).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;

    #[test]
    fn fee_amounts_split_gross_with_line_rounding_rule() {
        let (gross, net, tax) =
            fee_amounts(Amount::from_str("106.00").unwrap(), Rate::from_str("0.06").unwrap());
        assert_eq!(
            (gross.to_string(), net.to_string(), tax.to_string()),
            ("106.00".into(), "99.64".into(), "6.36".into())
        );

        let (_, net, tax) = fee_amounts(Amount::from_str("0.05").unwrap(), Rate::from_str("0.13").unwrap());
        assert_eq!(net.checked_add(tax), Amount::from_str("0.05").unwrap());

        let (_, net, tax) = fee_amounts(Amount::from_str("88.00").unwrap(), Rate::from_str("0").unwrap());
        assert_eq!((net, tax), (Amount::from_str("88.00").unwrap(), Amount::zero()));
    }

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

    // 确认汇总的起点数值精确为零，展示初始金额时仍保留两位小数。
    #[test]
    fn zero_is_exact_with_two_decimal_display() {
        let zero = Amount::zero();
        assert_eq!(zero.to_decimal(), Decimal::new(0, 2));
        assert_eq!(zero.to_string(), "0.00");
        assert_eq!(zero, Amount::from_str("0.00").unwrap());
    }

    // 覆盖四种定点类型的有效小数位上限：尾零不提高精度，真正超位不允许靠暗中舍入通过。
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

    // 固定财务舍入的中点取偶规则，覆盖负数对称和大额跨位进位。
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
        assert_eq!(round_to_cent(Decimal::new(9_999_999_999_999, 3)), Decimal::new(1_000_000_000_000, 2));
    }

    // 用不同单价、数量和税率核对明细含税额等于净额加税额，并检查含税额以分计。
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

    // 固定单价 `9.99`、数量 `3`、税率 `13%` 的含税额、净额和税额，防止明细拆税公式漂移。
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

    // 锁定四种定点值的 JSON 字符串合同及往返，保留单价、数量和税率的显示精度。
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

    // 验证正负整数及浮点都不能绕过字符串合同，且合法字符串仍须满足小数位限制。
    #[test]
    fn json_numbers_are_rejected() {
        let err = serde_json::from_str::<Amount>("123").unwrap_err().to_string();
        assert!(err.contains("字符串传输"), "u64 路径文案漂移：{err}");
        let err = serde_json::from_str::<Amount>("-5").unwrap_err().to_string();
        assert!(err.contains("字符串传输"), "i64 路径文案漂移：{err}");
        assert!(serde_json::from_str::<Amount>("123.45").is_err());
        assert!(serde_json::from_str::<Amount>("\"1.234\"").is_err());
    }

    // 用非 human-readable 的字节路径模拟驱动持久化，断言四种定点值在 BSON 上均为 Decimal128 且可回读。
    // 默认文档序列化是 human-readable，不能代替字节路径的类型断言；单独核对文档往返。
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

    // 用一笔扣税和还原含税额的计算核对加减不触发舍入，并确认零值可作为汇总起点。
    #[test]
    fn amount_add_sub_is_exact() {
        let gross = Amount::from_str("29.97").unwrap();
        let tax = Amount::from_str("3.90").unwrap();
        assert_eq!(gross.checked_sub(tax), Amount::from_str("26.07").unwrap());
        assert_eq!(Amount::from_str("26.07").unwrap().checked_add(tax), gross);
        assert_eq!(gross.checked_add(Amount::zero()), gross);
    }
}
