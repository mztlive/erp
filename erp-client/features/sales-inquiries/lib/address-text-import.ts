import {
    isLeafRegion,
    regionFullName,
    type RegionIndex,
} from "@/features/regions/lib/region-index"
import type { RegionNode } from "@/features/regions/api"
import { compareDecimal } from "@/lib/fixed-decimal"
import type { InquiryAllocation } from "../types"

export const ADDRESS_TEXT_LIMIT = 100_000
export type AddressImportRow = {
    id: string
    line: number
    recipient: string
    phone: string
    province: string
    city: string
    district: string
    address: string
    quantity: string
    parseError?: string
}
export const addressImportDefaults = {
    text: "",
    rows: [] as AddressImportRow[],
}

/** 保留 Excel 空单元格、引号内换行及转义引号，同时记录源文本行号。 */
function clipboardRows(text: string) {
    const result: { cells: string[]; line: number; raw: string }[] = []
    let cells: string[] = [],
        cell = "",
        quoted = false,
        line = 1,
        start = 1,
        offset = 0
    const source = text.replace(/^\uFEFF/, "")
    for (let i = 0; i < source.length; i++) {
        const char = source[i]
        if (char === '"' && (quoted || cell === "")) {
            if (quoted && source[i + 1] === '"') {
                cell += '"'
                i++
            } else quoted = !quoted
        } else if (
            !quoted &&
            (char === "\t" || char === "\r" || char === "\n")
        ) {
            cells.push(cell)
            cell = ""
            if (char !== "\t") {
                result.push({
                    cells,
                    line: start,
                    raw: source.slice(offset, i),
                })
                cells = []
                if (char === "\r" && source[i + 1] === "\n") i++
                line++
                start = line
                offset = i + 1
            }
        } else {
            cell += char
            if (char === "\n") line++
        }
    }
    if (quoted) throw new Error("文本中的双引号未闭合，请检查后重新解析。")
    cells.push(cell)
    result.push({ cells, line: start, raw: source.slice(offset) })
    return result.filter((row) => row.cells.some((value) => value.trim()))
}

function spaceCells(value: string): string[] {
    const cells: string[] = []
    const pattern = /"((?:[^"]|"")*)"|([^\s"]+)/g
    let match: RegExpExecArray | null
    let end = 0
    while ((match = pattern.exec(value))) {
        if (value.slice(end, match.index).trim())
            throw new Error("空格分隔的字段含有不完整的双引号。")
        cells.push((match[1] ?? match[2]).replace(/""/g, '"'))
        end = pattern.lastIndex
    }
    if (value.slice(end).trim())
        throw new Error("空格分隔的字段含有不完整的双引号。")
    return cells
}

/** 六列收件信息，可带第七列配送数量；空格模式中含空格的字段须用双引号包裹。 */
export function parseAddressText(text: string): AddressImportRow[] {
    if (text.length > ADDRESS_TEXT_LIMIT)
        throw new Error("文本超过 10 万字符，请分批添加。")
    const rows = clipboardRows(text)
    const result: AddressImportRow[] = []
    for (const row of rows) {
        let cells = row.cells.map((value) => value.trim())
        let parseError: string | undefined
        if (cells.length === 1) {
            try {
                cells = spaceCells(row.raw)
            } catch (error) {
                parseError = (error as Error).message
            }
        }
        const headers = [
            ["收件人", "收货人"],
            ["联系电话", "电话", "手机号"],
            ["省", "省份"],
            ["市", "城市"],
            ["区", "区县", "区/县"],
            ["详细地址"],
        ]
        if (
            result.length === 0 &&
            (cells.length === 6 || cells.length === 7) &&
            headers.every((names, i) => names.includes(cells[i])) &&
            (cells.length === 6 || cells[6] === "配送数量")
        )
            continue
        if (cells.length !== 6 && cells.length !== 7)
            parseError =
                "须为 6 或 7 列；字段含空格时请用 Tab 分隔或双引号包裹。"
        result.push({
            id: crypto.randomUUID(),
            line: row.line,
            recipient: cells[0] ?? "",
            phone: cells[1] ?? "",
            province: cells[2] ?? "",
            city: cells[3] ?? "",
            district: cells[4] ?? "",
            address: cells[5] ?? "",
            quantity: cells[6] || "1",
            parseError,
        })
    }
    if (!result.length) throw new Error("请粘贴至少一行收件信息。")
    if (result.length > 2000)
        throw new Error("单次最多解析 2,000 行，请拆分文本后重新解析。")
    return result
}

const shortName = (name: string) =>
    name.replace(
        /(?:壮族自治区|回族自治区|维吾尔自治区|特别行政区|自治区|省|市|区|县)$/,
        "",
    )
function matching(nodes: readonly RegionNode[], name: string) {
    const exact = nodes.filter((node) => node.name === name)
    return exact.length
        ? exact
        : nodes.filter((node) => shortName(node.name) === shortName(name))
}

/** 按省、市、区逐级匹配；直辖市可填“北京市 北京市 朝阳区”。 */
export function resolveAddressRegion(
    index: RegionIndex,
    province: string,
    city: string,
    district: string,
): string {
    if (!province || !city) throw new Error("省、市必须填写完整。")
    const provinces = matching(index.provinces, province)
    if (provinces.length !== 1)
        throw new Error("省份无法唯一匹配，请填写完整名称。")
    const children = index.children.get(provinces[0].code) ?? []
    let cities = matching(children, city)
    if (
        !cities.length &&
        ["北京市", "天津市", "上海市", "重庆市"].includes(provinces[0].name) &&
        shortName(city) === shortName(provinces[0].name)
    ) {
        cities = children.filter(
            (node) => node.name === "市辖区" || node.name === "县",
        )
    }
    if (!cities.length) throw new Error("城市与省份不匹配，请核对。")
    if (cities.length === 1 && isLeafRegion(index, cities[0].code)) {
        if (
            !district ||
            ["—", "-", "无", "市辖区"].includes(district) ||
            matching(cities, district).length === 1
        )
            return cities[0].code
        throw new Error(
            "当前区划目录仅到该市，区县请填“—”，街道或镇请写入详细地址。",
        )
    }
    if (!district) throw new Error("请填写区县。")
    const districts = matching(
        cities.flatMap((node) => index.children.get(node.code) ?? []),
        district,
    )
    if (districts.length !== 1 || !isLeafRegion(index, districts[0].code))
        throw new Error("区县与省市不匹配或不唯一，请核对完整名称。")
    return districts[0].code
}

export function allocationFieldErrors(value: InquiryAllocation, scale: number) {
    const fields: Partial<Record<keyof InquiryAllocation, string>> = {}
    if (!value.recipient.trim()) fields.recipient = "请填写收件人"
    if (!value.phone.trim()) fields.phone = "请填写联系电话"
    else if (/^\d+(?:\.\d+)?e[+-]?\d+$/i.test(value.phone.trim()))
        fields.phone = "电话不能使用科学计数法，请在 Excel 中按文本填写"
    if (!value.region_code) fields.region_code = "请选择完整的省、市、区"
    if (!value.address.trim()) fields.address = "请填写详细地址"
    try {
        if (compareDecimal(value.quantity, "0", scale) <= 0)
            fields.quantity = "配送数量须大于 0"
    } catch {
        fields.quantity = `请填写有效数量，最多 ${scale} 位小数`
    }
    return fields
}

export function validateAddressImportRow(
    row: AddressImportRow,
    regions: RegionIndex,
    scale: number,
) {
    const errors: string[] = row.parseError ? [row.parseError] : []
    let code = ""
    try {
        code = resolveAddressRegion(
            regions,
            row.province,
            row.city,
            row.district,
        )
    } catch (error) {
        errors.push((error as Error).message)
    }
    const allocation: InquiryAllocation = {
        allocation_id: row.id,
        recipient: row.recipient.trim(),
        phone: row.phone.trim(),
        region_code: code,
        region_name: code ? regionFullName(regions, code) : "",
        address: row.address.trim(),
        quantity: row.quantity.trim(),
    }
    const fields = allocationFieldErrors(allocation, scale)
    delete fields.region_code
    errors.push(...Object.values(fields))
    return { allocation, errors }
}
