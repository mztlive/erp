//! `ExternalOrderKey`：商城来源单二进制比较键（数据模型 §6.13）。

use std::fmt;

use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// 商城来源单二进制比较键。
///
/// 由 [`ExternalOrderKey::from_trimmed`] 生成：对来源单号移除首尾空白后取
/// UTF-8 字节，**保留大小写**、不做 Unicode 兼容折叠（数据模型 §6.13：
/// `external_order_key` 是二进制比较键，来源单号在同一来源商城内唯一且
/// 不得复用）。BSON 形态固定为 `Binary`（Generic subtype），唯一索引直接
/// 建在字节上。
///
/// 地基修订候选：与 `source_registry::ExternalIdKey` 同构（P0 冻结，暂在
/// 域内复制一份），建议后续下沉到 `common/` 统一实现。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalOrderKey(Vec<u8>);

impl ExternalOrderKey {
    /// 构造比较键。
    ///
    /// # 参数
    /// * `bytes` - 规范化后的 UTF-8 字节
    ///
    /// # 返回
    /// 返回比较键实例。
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// 由来源单号生成比较键。
    ///
    /// 规范化规则（数据模型 §6.13）：只移除首尾空白，不做大小写折叠；
    /// `ABC` 与 `abc` 是两个不同的来源单号。
    ///
    /// # 参数
    /// * `order_no` - 来源单号原值
    ///
    /// # 返回
    /// 返回去除首尾空白后按 UTF-8 编码的字节。
    pub fn from_trimmed(order_no: &str) -> Self {
        Self(order_no.trim().as_bytes().to_vec())
    }

    /// 返回比较键的字节切片。
    ///
    /// # 返回
    /// 返回规范化后的 UTF-8 字节。
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// 返回用于 MongoDB 唯一索引与等值筛选的 BSON 二进制值。
    ///
    /// # 返回
    /// 返回 Generic subtype 的 BSON 二进制，与实体持久化形态一致。
    pub fn to_bson_binary(&self) -> bson::Binary {
        bson::Binary {
            subtype: bson::spec::BinarySubtype::Generic,
            bytes: self.0.clone(),
        }
    }
}

impl fmt::Display for ExternalOrderKey {
    /// 以 UTF-8 字符串形式展示比较键（用于日志与调试）。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&String::from_utf8_lossy(&self.0))
    }
}

impl Serialize for ExternalOrderKey {
    /// 序列化比较键：human-readable（JSON）输出字节数组；
    /// 非 human-readable（MongoDB 驱动）委托 `bson::Binary` 输出真正的 BSON 二进制。
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.serialize_bytes(&self.0)
        } else {
            self.to_bson_binary().serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for ExternalOrderKey {
    /// 反序列化比较键：JSON 形态接受字节数组；BSON 形态接受 `Binary` 变体。
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            deserializer.deserialize_seq(ExternalOrderKeyVisitor)
        } else {
            let binary = bson::Binary::deserialize(deserializer)?;
            Ok(Self(binary.bytes))
        }
    }
}

/// JSON 形态（字节数组）的比较键访问器。
struct ExternalOrderKeyVisitor;

impl<'de> Visitor<'de> for ExternalOrderKeyVisitor {
    type Value = ExternalOrderKey;

    /// 描述期望的 JSON 形态。
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("来源单号比较键的 UTF-8 字节序列")
    }

    /// 从字节序列构造比较键。
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error> {
        let mut bytes = Vec::new();
        while let Some(byte) = seq.next_element::<u8>()? {
            bytes.push(byte);
        }
        Ok(ExternalOrderKey(bytes))
    }

    /// 接受 bson 以 human-readable 形态暴露的二进制值（`bson::deserialize_from_document` 默认模式）。
    fn visit_byte_buf<E>(self, bytes: Vec<u8>) -> std::result::Result<Self::Value, E> {
        Ok(ExternalOrderKey(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_trimmed_trims_outer_whitespace_and_keeps_case() {
        assert_eq!(ExternalOrderKey::from_trimmed("  S1  ").as_bytes(), b"S1");
        assert_ne!(
            ExternalOrderKey::from_trimmed("ABC"),
            ExternalOrderKey::from_trimmed("abc"),
            "ABC 与 abc 是两个不同的来源单号"
        );
        assert_eq!(
            ExternalOrderKey::from_trimmed("  ABC  "),
            ExternalOrderKey::from_trimmed("ABC")
        );
    }

    #[test]
    fn display_shows_utf8_lossy_text() {
        let key = ExternalOrderKey::from_trimmed("销售单-1");
        assert_eq!(key.to_string(), "销售单-1");
    }

    #[test]
    fn json_roundtrip_uses_byte_array() {
        let key = ExternalOrderKey::from_trimmed("S1");
        let json = serde_json::to_string(&key).unwrap();
        assert_eq!(json, "[83,49]");
        let back: ExternalOrderKey = serde_json::from_str(&json).unwrap();
        assert_eq!(back, key);
    }

    #[test]
    fn bson_wire_roundtrip_persists_binary() {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        struct Doc {
            key: ExternalOrderKey,
        }

        let doc = Doc {
            key: ExternalOrderKey::from_trimmed("S1"),
        };
        let bytes = bson::serialize_to_vec(&doc).unwrap();
        let wire_doc: bson::Document = bson::deserialize_from_slice(&bytes).unwrap();
        let bson::Bson::Binary(binary) = wire_doc.get("key").unwrap() else {
            panic!("key 必须以 BSON Binary 持久化");
        };
        assert_eq!(binary.bytes, b"S1");

        let back: Doc = bson::deserialize_from_slice(&bytes).unwrap();
        assert_eq!(back, doc);
    }
}
