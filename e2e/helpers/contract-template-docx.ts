import { deflateRawSync, inflateRawSync } from "node:zlib"

import { expect, type Page } from "@playwright/test"

export const DOCX_MIME =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
export type ZipPart = { name: string; bytes: Buffer }

const WORD_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
const REL_NS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
const PACKAGE_NS = "http://schemas.openxmlformats.org/package/2006/relationships"
const IMAGE = Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a6WQAAAAASUVORK5CYII=",
    "base64",
)
const CRC_TABLE = Array.from({ length: 256 }, (_, value) => {
    for (let bit = 0; bit < 8; bit += 1) {
        value = value & 1 ? 0xedb88320 ^ (value >>> 1) : value >>> 1
    }
    return value >>> 0
})

function crc32(bytes: Buffer): number {
    let crc = 0xffffffff
    for (const byte of bytes) crc = CRC_TABLE[(crc ^ byte) & 0xff]! ^ (crc >>> 8)
    return (crc ^ 0xffffffff) >>> 0
}

/** 以标准 ZIP 生成 DOCX；重复或危险条目仅用于上传拒绝用例。 */
export function zipParts(parts: ZipPart[]): Buffer {
    const files: Buffer[] = []
    const central: Buffer[] = []
    let offset = 0
    for (const part of parts) {
        const name = Buffer.from(part.name)
        const compressed = deflateRawSync(part.bytes)
        const crc = crc32(part.bytes)
        const header = Buffer.alloc(30)
        header.writeUInt32LE(0x04034b50)
        header.writeUInt16LE(20, 4)
        header.writeUInt16LE(8, 8)
        header.writeUInt32LE(crc, 14)
        header.writeUInt32LE(compressed.length, 18)
        header.writeUInt32LE(part.bytes.length, 22)
        header.writeUInt16LE(name.length, 26)
        files.push(header, name, compressed)
        const directory = Buffer.alloc(46)
        directory.writeUInt32LE(0x02014b50)
        directory.writeUInt16LE(20, 4)
        directory.writeUInt16LE(20, 6)
        directory.writeUInt16LE(8, 10)
        directory.writeUInt32LE(crc, 16)
        directory.writeUInt32LE(compressed.length, 20)
        directory.writeUInt32LE(part.bytes.length, 24)
        directory.writeUInt16LE(name.length, 28)
        directory.writeUInt32LE(offset, 42)
        central.push(directory, name)
        offset += header.length + name.length + compressed.length
    }
    const directory = Buffer.concat(central)
    const end = Buffer.alloc(22)
    end.writeUInt32LE(0x06054b50)
    end.writeUInt16LE(parts.length, 8)
    end.writeUInt16LE(parts.length, 10)
    end.writeUInt32LE(directory.length, 12)
    end.writeUInt32LE(offset, 16)
    return Buffer.concat([...files, directory, end])
}

/** 两节合同含普通页眉、页脚和图片，用于核对编号与原内容保存。 */
export function contractDocxParts(marker: string): ZipPart[] {
    const xml = (name: string, content: string): ZipPart => ({
        name,
        bytes: Buffer.from(content),
    })
    return [
        xml("[Content_Types].xml", `<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/><Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/></Types>`),
        xml("_rels/.rels", `<Relationships xmlns="${PACKAGE_NS}"><Relationship Id="document" Type="${REL_NS}/officeDocument" Target="word/document.xml"/></Relationships>`),
        xml("word/_rels/document.xml.rels", `<Relationships xmlns="${PACKAGE_NS}"><Relationship Id="normal-header" Type="${REL_NS}/header" Target="header1.xml"/><Relationship Id="normal-footer" Type="${REL_NS}/footer" Target="footer1.xml"/><Relationship Id="logo" Type="${REL_NS}/image" Target="media/logo.png"/></Relationships>`),
        xml("word/document.xml", `<w:document xmlns:w="${WORD_NS}" xmlns:r="${REL_NS}" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:body><w:p><w:pPr><w:sectPr><w:headerReference w:type="default" r:id="normal-header"/><w:footerReference w:type="default" r:id="normal-footer"/></w:sectPr></w:pPr><w:r><w:t>${marker} 合同正文第一节</w:t></w:r></w:p><w:p><w:r><w:drawing><wp:inline><wp:extent cx="9525" cy="9525"/><wp:docPr id="1" name="合同图片"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="1" name="logo.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="logo"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="9525" cy="9525"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p><w:p><w:r><w:t>${marker} 合同正文第二节</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="normal-header"/><w:footerReference w:type="default" r:id="normal-footer"/><w:titlePg/></w:sectPr></w:body></w:document>`),
        xml("word/header1.xml", `<w:hdr xmlns:w="${WORD_NS}"><w:p><w:r><w:t>${marker} 原页眉</w:t></w:r></w:p></w:hdr>`),
        xml("word/footer1.xml", `<w:ftr xmlns:w="${WORD_NS}"><w:p><w:r><w:t>${marker} 原页脚</w:t></w:r></w:p></w:ftr>`),
        { name: "word/media/logo.png", bytes: IMAGE },
    ]
}

/** 读取真实下载包的中央目录，同时支持存储和 DEFLATE 压缩条目。 */
function unzipParts(bytes: Buffer): Map<string, Buffer> {
    let end = bytes.length - 22
    while (end >= Math.max(0, bytes.length - 65_557) && bytes.readUInt32LE(end) !== 0x06054b50) end -= 1
    if (end < 0) throw new Error("下载文件没有 ZIP 目录")
    const parts = new Map<string, Buffer>()
    let directory = bytes.readUInt32LE(end + 16)
    for (let index = 0; index < bytes.readUInt16LE(end + 10); index += 1) {
        expect(bytes.readUInt32LE(directory)).toBe(0x02014b50)
        const method = bytes.readUInt16LE(directory + 10)
        const size = bytes.readUInt32LE(directory + 20)
        const nameSize = bytes.readUInt16LE(directory + 28)
        const local = bytes.readUInt32LE(directory + 42)
        const name = bytes.subarray(directory + 46, directory + 46 + nameSize).toString()
        const start = local + 30 + bytes.readUInt16LE(local + 26) + bytes.readUInt16LE(local + 28)
        const payload = bytes.subarray(start, start + size)
        if (method !== 0 && method !== 8) throw new Error(`不支持的 ZIP 压缩方式 ${method}`)
        parts.set(name, method === 8 ? inflateRawSync(payload) : payload)
        directory += 46 + nameSize + bytes.readUInt16LE(directory + 30) + bytes.readUInt16LE(directory + 32)
    }
    return parts
}

/** 验证真实 DOCX 的页眉引用、编号样式、正文与原图片；不代替实际 Word 排版验收。 */
export async function expectStampedContractDocx(
    page: Page,
    bytes: Buffer,
    original: ZipPart[],
    number: string,
    marker: string,
): Promise<void> {
    const parts = unzipParts(bytes)
    for (const part of original.filter((part) =>
        ["word/header1.xml", "word/footer1.xml", "word/media/logo.png"].includes(part.name),
    )) expect(parts.get(part.name), part.name).toEqual(part.bytes)
    const result = await page.evaluate(({ sources, number, word, rel, pkg }) => {
        const parse = (path: string) => {
            const document = new DOMParser().parseFromString(sources[path] ?? "", "application/xml")
            if (document.querySelector("parsererror")) throw new Error(`Word XML 无效: ${path}`)
            return document
        }
        const text = (document: Document) => Array.from(document.getElementsByTagNameNS(word, "t"))
            .map((node) => node.textContent).join("")
        const relationships = Array.from(parse("word/_rels/document.xml.rels").getElementsByTagNameNS(pkg, "Relationship"))
        const sections = Array.from(parse("word/document.xml").getElementsByTagNameNS(word, "sectPr"))
        const firstHeader = (section: Element) => {
            const reference = Array.from(section.children).find((node) =>
                node.localName === "headerReference" && node.getAttributeNS(word, "type") === "first",
            )
            const target = relationships.find((node) => node.getAttribute("Id") === reference?.getAttributeNS(rel, "id"))?.getAttribute("Target")
            return target ? parse(`word/${target}`) : null
        }
        const headers = sections.map(firstHeader)
        const paragraph = Array.from(headers[0]?.getElementsByTagNameNS(word, "p") ?? [])
            .find((node) => text(node.ownerDocument) && node.textContent?.includes(number))
        const value = (tag: string) => paragraph?.getElementsByTagNameNS(word, tag)[0]?.getAttributeNS(word, "val")
        return {
            body: text(parse("word/document.xml")),
            headers: headers.map((header) => header ? text(header) : null),
            sections: sections.length,
            firstTitlePage: sections[0]?.getElementsByTagNameNS(word, "titlePg").length,
            alignment: value("jc"),
            size: value("sz"),
            color: value("color"),
            defaultReferences: sections.map((section) => Array.from(section.children)
                .filter((node) => ["headerReference", "footerReference"].includes(node.localName) && node.getAttributeNS(word, "type") === "default")
                .map((node) => node.getAttributeNS(rel, "id"))),
        }
    }, {
        sources: Object.fromEntries(Array.from(parts).filter(([name]) => name.endsWith(".xml") || name.endsWith(".rels")).map(([name, value]) => [name, value.toString()])),
        number,
        word: WORD_NS,
        rel: REL_NS,
        pkg: PACKAGE_NS,
    })
    expect(result.body).toContain(`${marker} 合同正文第一节`)
    expect(result.body).toContain(`${marker} 合同正文第二节`)
    expect(result.sections).toBe(2)
    expect(result.headers[0]).toContain(number)
    expect(result.headers[0]).toContain(`${marker} 原页眉`)
    expect(result.headers[1]).not.toContain(number)
    expect(result.firstTitlePage).toBe(1)
    expect(result).toMatchObject({ alignment: "right", size: "20", color: "000000" })
    expect(result.defaultReferences).toEqual([
        ["normal-header", "normal-footer"],
        ["normal-header", "normal-footer"],
    ])
}
