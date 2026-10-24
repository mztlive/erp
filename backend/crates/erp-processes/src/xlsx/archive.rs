use std::collections::HashSet;
use std::io::{Cursor, Read};

use zip::ZipArchive;

use super::{document, invalid, text};
use crate::Result;

pub(crate) const MAX_XML_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Copy)]
pub(crate) struct ArchiveLimits {
    pub expanded_bytes: u64,
    pub entry_bytes: u64,
}

impl ArchiveLimits {
    pub const CUSTOMER: Self = Self { expanded_bytes: 32 * 1024 * 1024, entry_bytes: MAX_XML_BYTES };
    // 商品原表含图片，沿用 700 MiB 上传能力，XML 仍单独限制为 16 MiB。
    pub const PRODUCT: Self = Self { expanded_bytes: 1024 * 1024 * 1024, entry_bytes: 700 * 1024 * 1024 };
}

pub(crate) struct Archive<'a> {
    archive: ZipArchive<Cursor<&'a [u8]>>,
}

impl<'a> Archive<'a> {
    /// 校验 ZIP 条目与展开边界后创建读取器。
    /// # 参数
    /// `bytes` 为工作簿字节，`limits` 为调用场景的展开限制。
    /// # 返回
    /// 可按安全路径读取 XML 的工作簿。
    /// # 错误
    /// ZIP 损坏、路径不安全、宏、外部链接或展开超限时拒绝。
    pub fn open(bytes: &'a [u8], limits: ArchiveLimits) -> Result<Self> {
        let mut archive =
            ZipArchive::new(Cursor::new(bytes)).map_err(|_| invalid("请上传有效 .xlsx 工作簿"))?;
        check_archive(&mut archive, limits)?;
        Ok(Self { archive })
    }

    /// 有界读取必要 XML，读取错误不得降级为空内容。
    /// # 参数
    /// `name` 为包内路径。
    /// # 返回
    /// UTF-8 XML 原文。
    /// # 错误
    /// 缺项、CRC/解压失败、编码错误或超过 16 MiB 时拒绝。
    pub fn xml(&mut self, name: &str) -> Result<String> {
        let file = self.archive.by_name(name).map_err(|_| invalid(format!("工作簿缺少必要内容：{name}")))?;
        let mut bytes = Vec::new();
        file.take(MAX_XML_BYTES + 1).read_to_end(&mut bytes).map_err(|_| invalid("工作簿解压失败"))?;
        if u64::try_from(bytes.len()).map_err(|_| invalid("工作簿过大"))? > MAX_XML_BYTES {
            return Err(invalid("XML 解压大小超过 16 MiB"));
        }
        String::from_utf8(bytes).map_err(|_| invalid("工作簿编码无效"))
    }

    /// 检查包内条目是否存在，不读取内容。
    /// # 参数
    /// `name` 为经过解析的包内路径。
    /// # 返回
    /// 条目存在时为真。
    /// # 错误
    /// 无。
    pub fn contains(&self, name: &str) -> bool {
        self.archive.index_for_name(name).is_some()
    }

    /// 仅将条目不存在视为可选内容缺失。
    /// # 参数
    /// `name` 为包内路径。
    /// # 返回
    /// 存在时返回 XML，不存在时返回 `None`。
    /// # 错误
    /// 条目存在但读取失败时返回原读取错误。
    pub fn optional_xml(&mut self, name: &str) -> Result<Option<String>> {
        if !self.contains(name) {
            return Ok(None);
        }
        self.xml(name).map(Some)
    }

    /// 读取字符串表并对 XML 实体执行一次解码。
    /// # 参数
    /// 使用当前工作簿。
    /// # 返回
    /// 按索引排列的文本；字符串表不存在时为空。
    /// # 错误
    /// 已存在的字符串表读取失败、结构损坏或 XML 非法时拒绝。
    pub fn shared_strings(&mut self) -> Result<Vec<String>> {
        let Some(xml) = self.optional_xml("xl/sharedStrings.xml")? else {
            return Ok(Vec::new());
        };
        let doc = document(&xml)?;
        if doc.root_element().tag_name().name() != "sst" {
            return Err(invalid("共享文本表结构无效"));
        }
        Ok(doc
            .root_element()
            .children()
            .filter(|node| node.is_element() && node.tag_name().name() == "si")
            .map(|node| {
                node.descendants()
                    .filter(|node| node.is_element() && node.tag_name().name() == "t")
                    .filter(|node| {
                        !node
                            .ancestors()
                            .any(|parent| parent.is_element() && parent.tag_name().name() == "rPh")
                    })
                    .map(text)
                    .collect()
            })
            .collect())
    }
}

fn check_archive(archive: &mut ZipArchive<Cursor<&[u8]>>, limits: ArchiveLimits) -> Result<()> {
    if archive.len() > 4096 {
        return Err(invalid("工作簿压缩条目超过 4096 个"));
    }
    let mut expanded = 0_u64;
    let mut names = HashSet::new();
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(|_| invalid("压缩条目无效"))?;
        expanded = expanded.checked_add(file.size()).ok_or_else(|| invalid("解压大小越限"))?;
        let name = file.name();
        if file.size() > limits.entry_bytes || expanded > limits.expanded_bytes {
            return Err(invalid("工作簿解压大小超过限制"));
        }
        if file.enclosed_name().is_none()
            || name.contains(['\\', ':', '\0'])
            || name.split('/').any(|part| part == ".." || part == ".")
            || !names.insert(name.to_string())
        {
            return Err(invalid("工作簿路径重复或不安全"));
        }
        if name.contains("vbaProject") || name.starts_with("xl/externalLinks/") {
            return Err(invalid("工作簿不得包含宏或外部链接"));
        }
    }
    Ok(())
}
