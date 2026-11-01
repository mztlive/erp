//! 未启用的售价输入仅用于切回模式时恢复，不参与批量计算。

use erp_core::money::Quantity;
use serde::{Deserialize, Serialize};

/// 原样保存未完成输入，并读取旧版暂存规则中的 BSON Decimal128。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DraftPriceValue {
    Text(String),
    Decimal(Quantity),
}

/// 暂存的计算方式允许参数尚未填完；启用后必须作为 PriceRule 重新校验。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "value", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum PriceRuleDraft {
    Unset,
    Percentage(DraftPriceValue),
    Add(DraftPriceValue),
    Fixed(DraftPriceValue),
}

/// 未启用模式的输入快照；与 BatchPricing 保持相同 JSON 结构，但不能用于计算。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum BatchPricingDraft {
    DropshipOnly { dropship: PriceRuleDraft },
    Same { rule: PriceRuleDraft, minimum_quantity: Option<DraftPriceValue> },
    Separate { dropship: PriceRuleDraft, bulk: PriceRuleDraft, minimum_quantity: Option<DraftPriceValue> },
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use bson::{Decimal128, doc};
    use serde_json::json;

    use super::*;

    #[test]
    fn incomplete_input_survives_json_and_bson_round_trips() {
        let input = json!({
            "mode":"SEPARATE",
            "dropship":{"method":"FIXED","value":""},
            "bulk":{"method":"PERCENTAGE","value":"15."},
            "minimum_quantity":" "
        });
        let draft: BatchPricingDraft = serde_json::from_value(input.clone()).unwrap();
        let stored = bson::serialize_to_document(&draft).unwrap();
        let restored: BatchPricingDraft = bson::deserialize_from_document(stored).unwrap();
        assert_eq!(serde_json::to_value(restored).unwrap(), input);
    }

    #[test]
    fn existing_decimal128_drafts_keep_their_values() {
        let stored = doc! {
            "mode":"SEPARATE",
            "dropship":{"method":"FIXED","value":Decimal128::from_str("200.00").unwrap()},
            "bulk":{"method":"PERCENTAGE","value":Decimal128::from_str("15.1250").unwrap()},
            "minimum_quantity":Decimal128::from_str("100.000001").unwrap()
        };
        let restored: BatchPricingDraft = bson::deserialize_from_document(stored).unwrap();
        assert_eq!(
            serde_json::to_value(restored).unwrap(),
            json!({
                "mode":"SEPARATE",
                "dropship":{"method":"FIXED","value":"200.00"},
                "bulk":{"method":"PERCENTAGE","value":"15.1250"},
                "minimum_quantity":"100.000001"
            })
        );
    }

    #[test]
    fn draft_still_rejects_numbers_unknown_methods_and_unknown_fields() {
        for input in [
            json!({"mode":"SAME","rule":{"method":"FIXED","value":200}}),
            json!({"mode":"SAME","rule":{"method":"OTHER","value":"200"}}),
            json!({"mode":"SAME","rule":{"method":"UNSET"},"extra":true}),
        ] {
            assert!(serde_json::from_value::<BatchPricingDraft>(input).is_err());
        }
    }
}
