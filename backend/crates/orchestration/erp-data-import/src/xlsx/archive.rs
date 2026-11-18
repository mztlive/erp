use std::collections::HashSet;
use std::io::{Cursor, Read};

use zip::ZipArchive;

use super::{document, invalid, rich_text};
use crate::Result;

pub(crate) const MAX_XML_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Copy)]
pub(crate) struct ArchiveLimits {
    pub(crate) expanded_bytes: u64,
    pub(crate) entry_bytes: u64,
}

impl ArchiveLimits {
    pub(crate) const CUSTOMER: Self = Self { expanded_bytes: 32 * 1024 * 1024, entry_bytes: MAX_XML_BYTES };
    // 商品原表含图片，沿用 700 MiB 上传能力，XML 仍单独限制为 16 MiB。
    pub(crate) const PRODUCT: Self =
        Self { expanded_bytes: 1024 * 1024 * 1024, entry_bytes: 700 * 1024 * 1024 };
}

pub(crate) struct Archive<'a> {
    archive: ZipArchive<Cursor<&'a [u8]>>,
}

impl<'a> Archive<'a> {
    // 校验 ZIP 条目与展开边界后创建读取器。
    //
    // `bytes` 为工作簿字节，`limits` 为调用场景的展开限制。
    //
    // 可按安全路径读取 XML 的工作簿。
    //
    // ZIP 损坏、路径不安全、宏、外部链接或展开超限时拒绝。
    pub(crate) fn open(bytes: &'a [u8], limits: ArchiveLimits) -> Result<Self> {
        let mut archive =
            ZipArchive::new(Cursor::new(bytes)).map_err(|_| invalid("请上传有效 .xlsx 工作簿"))?;
        check_archive(&mut archive, limits)?;
        Ok(Self { archive })
    }

    // 有界读取必要 XML，读取错误不得降级为空内容。
    //
    // `name` 为包内路径。
    //
    // UTF-8 XML 原文。
    //
    // 缺项、CRC/解压失败、编码错误或超过 16 MiB 时拒绝。
    pub(crate) fn xml(&mut self, name: &str) -> Result<String> {
        let file = self
            .archive
            .by_name(name)
            .map_err(|_| invalid("工作簿缺少必要内容，请用原模板重新另存为 .xlsx 后上传"))?;
        let mut bytes = Vec::new();
        file.take(MAX_XML_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("工作簿无法读取，请用表格软件重新另存为 .xlsx 后上传"))?;
        if u64::try_from(bytes.len()).map_err(|_| invalid("工作簿过大"))? > MAX_XML_BYTES {
            return Err(invalid("工作簿单个工作表或内嵌资料过大，请拆分文件后上传"));
        }

        String::from_utf8(bytes).map_err(|_| invalid("工作簿无法读取，请用表格软件重新另存为 .xlsx 后上传"))
    }

    // 检查包内条目是否存在，不读取内容。
    //
    // `name` 为经过解析的包内路径。
    //
    // 条目存在时为真。
    pub(crate) fn contains(&self, name: &str) -> bool {
        self.archive.index_for_name(name).is_some()
    }

    // 仅将条目不存在视为可选内容缺失。
    //
    // `name` 为包内路径。
    //
    // 存在时返回 XML，不存在时返回 `None`。
    //
    // 条目存在但读取失败时返回原读取错误。
    pub(crate) fn optional_xml(&mut self, name: &str) -> Result<Option<String>> {
        if !self.contains(name) {
            return Ok(None);
        }
        self.xml(name).map(Some)
    }

    // 读取字符串表并对 XML 实体执行一次解码。
    //
    // 使用当前工作簿。
    //
    // 按索引排列的文本；字符串表不存在时为空。
    //
    // 已存在的字符串表读取失败、结构损坏或 XML 非法时拒绝。
    pub(crate) fn shared_strings(&mut self) -> Result<Vec<String>> {
        let Some(xml) = self.optional_xml("xl/sharedStrings.xml")? else {
            return Ok(Vec::new());
        };

        let doc = document(&xml)?;
        if doc.root_element().tag_name().name() != "sst" {
            return Err(invalid("工作簿中的文字无法读取，请用原模板重新另存为 .xlsx 后上传"));
        }

        Ok(doc
            .root_element()
            .children()
            .filter(|node| node.is_element() && node.tag_name().name() == "si")
            .map(rich_text)
            .collect())
    }
}

fn check_archive(archive: &mut ZipArchive<Cursor<&[u8]>>, limits: ArchiveLimits) -> Result<()> {
    if archive.len() > 4096 {
        return Err(invalid("工作簿包含过多工作表或内嵌资料，请精简或拆分后上传"));
    }
    let mut expanded = 0_u64;
    let mut names = HashSet::new();
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|_| invalid("工作簿包含无法读取的内容，请用原模板重新另存后上传"))?;
        expanded =
            expanded.checked_add(file.size()).ok_or_else(|| invalid("工作簿内容过多，请拆分后上传"))?;
        let name = file.name();
        if file.size() > limits.entry_bytes || expanded > limits.expanded_bytes {
            return Err(invalid("工作簿内容过多，请删减图片或拆分后上传"));
        }

        if file.enclosed_name().is_none()
            || name.contains(['\\', ':', '\0'])
            || name.split('/').any(|part| part == ".." || part == ".")
            || !names.insert(name.to_string())
        {
            return Err(invalid("工作簿结构异常，请将数据复制到新的导入模板后上传"));
        }

        if name.contains("vbaProject") || name.starts_with("xl/externalLinks/") {
            return Err(invalid("工作簿不得包含宏或外部链接，请移除后重新上传"));
        }
    }
    Ok(())
}
