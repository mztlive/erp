//! 销售草稿/变更/提交内容身份：已持久化 `draft:`/`change:`/`sub:` 形态。
//!
//! 本值对象只派生历史 wire 字符串，不计算命令幂等 SHA-256 fingerprint。
//! 正式版本 `sub:{id}` 语义与既有聚合工厂一致，不得更换算法或前缀。

use crate::errors::{Error, Result};

/// 已持久化内容指纹最大长度（与工作副本 `content_hash` 上限一致）。
const CONTENT_HASH_MAX_LEN: usize = 128;

/// 销售内容身份（draft/change/submission 派生，不包含命令幂等哈希）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SalesContentHash(String);

impl SalesContentHash {
    /// 由草稿身份与草稿版本派生 `draft:{id}:{version}` 指纹。
    ///
    /// # 参数
    /// * `id` - 草稿或销售单稳定身份（调用方选择写入哪一个历史身份）
    /// * `version` - 草稿版本，必须为正整数
    ///
    /// # 返回
    /// 返回可写入工作副本的内容指纹。
    ///
    /// # 错误
    /// 身份为空、版本为 0，或格式化结果超过 128 字符时返回错误。
    ///
    /// # 关键业务约束
    /// 必须保持历史 `draft:{id}:{version}` 形态；空身份与非法版本失败关闭。
    pub fn draft(id: &str, version: u32) -> Result<Self> {
        Self::prefixed("draft", id, Some(version))
    }

    /// 由销售变更单身份与草稿版本派生 `change:{id}:{version}` 指纹。
    ///
    /// # 参数
    /// * `id` - 销售变更单稳定身份
    /// * `version` - 变更工作副本草稿版本，必须为正整数
    ///
    /// # 返回
    /// 返回可写入变更工作副本的内容指纹。
    ///
    /// # 错误
    /// 身份为空、版本为 0，或格式化结果超过 128 字符时返回错误。
    ///
    /// # 关键业务约束
    /// 必须保持历史 `change:{id}:{version}` 形态；不得并入命令幂等 SHA-256。
    pub fn change(id: &str, version: u32) -> Result<Self> {
        Self::prefixed("change", id, Some(version))
    }

    /// 由提交主键派生 `sub:{id}` 指纹。
    ///
    /// # 参数
    /// * `id` - 首次提交或变更提交主键
    ///
    /// # 返回
    /// 返回正式版本/审批目标使用的内容指纹。
    ///
    /// # 错误
    /// 身份为空或格式化结果超过 128 字符时返回错误。
    ///
    /// # 关键业务约束
    /// 必须保持历史 `sub:{id}` 形态，不得更换算法或增加版本段。
    pub fn submission(id: &str) -> Result<Self> {
        Self::prefixed("sub", id, None)
    }

    /// 返回可持久化的指纹字符串。
    ///
    /// # 返回
    /// 返回历史 wire 形态切片。
    ///
    /// # 错误
    /// 无。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 消费自身并返回可写入实体的指纹字符串。
    ///
    /// # 返回
    /// 返回历史 wire 形态所有权字符串。
    ///
    /// # 错误
    /// 无。
    pub fn into_wire(self) -> String {
        self.0
    }

    /// 按历史前缀拼接身份，可选附加版本段。
    ///
    /// # 参数
    /// * `prefix` - `draft` / `change` / `sub`
    /// * `id` - 稳定身份
    /// * `version` - 草稿版本；`None` 表示提交指纹不含版本段
    ///
    /// # 返回
    /// 返回已校验长度的内容指纹。
    ///
    /// # 错误
    /// 身份为空、版本为 0，或结果超过上限时返回错误。
    ///
    /// # 关键业务约束
    /// 输出必须与迁移前 `format!` 字面量完全一致。
    fn prefixed(prefix: &str, id: &str, version: Option<u32>) -> Result<Self> {
        if id.is_empty() {
            return Err(Error::from("内容身份不能为空"));
        }
        let wire = match version {
            Some(0) => return Err(Error::from("内容版本必须为正整数")),
            Some(version) => format!("{prefix}:{id}:{version}"),
            None => format!("{prefix}:{id}"),
        };
        if wire.len() > CONTENT_HASH_MAX_LEN {
            return Err(Error::from("内容指纹过长"));
        }
        Ok(Self(wire))
    }
}

#[cfg(test)]
mod tests {
    use super::{SalesContentHash, CONTENT_HASH_MAX_LEN};

    #[test]
    fn golden_wire_formats_match_persisted_semantics() {
        assert_eq!(
            SalesContentHash::draft("so-1", 1).unwrap().as_str(),
            "draft:so-1:1"
        );
        assert_eq!(
            SalesContentHash::draft("wc-abc", 12).unwrap().into_wire(),
            "draft:wc-abc:12"
        );
        assert_eq!(
            SalesContentHash::change("change-order-1", 1).unwrap().as_str(),
            "change:change-order-1:1"
        );
        assert_eq!(SalesContentHash::submission("s-1").unwrap().as_str(), "sub:s-1");
        assert_eq!(
            SalesContentHash::submission("chg-sub-9").unwrap().into_wire(),
            "sub:chg-sub-9"
        );
    }

    #[test]
    fn empty_id_and_zero_version_fail_closed() {
        assert_eq!(
            SalesContentHash::draft("", 1).unwrap_err().to_string(),
            "内容身份不能为空"
        );
        assert_eq!(
            SalesContentHash::change("", 2).unwrap_err().to_string(),
            "内容身份不能为空"
        );
        assert_eq!(
            SalesContentHash::submission("").unwrap_err().to_string(),
            "内容身份不能为空"
        );
        assert_eq!(
            SalesContentHash::draft("so-1", 0).unwrap_err().to_string(),
            "内容版本必须为正整数"
        );
        assert_eq!(
            SalesContentHash::change("co-1", 0).unwrap_err().to_string(),
            "内容版本必须为正整数"
        );
    }

    #[test]
    fn length_boundary_fails_when_wire_exceeds_working_copy_max() {
        let id = "a".repeat(CONTENT_HASH_MAX_LEN);
        assert_eq!(
            SalesContentHash::draft(&id, 1).unwrap_err().to_string(),
            "内容指纹过长"
        );
        assert_eq!(
            SalesContentHash::change(&id, 1).unwrap_err().to_string(),
            "内容指纹过长"
        );
        assert_eq!(
            SalesContentHash::submission(&id).unwrap_err().to_string(),
            "内容指纹过长"
        );

        let max_submission_id = "b".repeat(CONTENT_HASH_MAX_LEN - "sub:".len());
        assert_eq!(
            SalesContentHash::submission(&max_submission_id)
                .unwrap()
                .as_str()
                .len(),
            CONTENT_HASH_MAX_LEN
        );
    }
}
