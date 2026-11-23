import {
    regionFullName,
    type RegionIndex,
} from "@/features/regions/lib/region-index"
import { resolveAddressRegion } from "@/features/regions/lib/address-region"
import { clipboardRows } from "@/lib/clipboard-rows"
import { compareDecimal } from "@/lib/fixed-decimal"
import type { InquiryAllocation } from "../types"

export { resolveAddressRegion } from "@/features/regions/lib/address-region"

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
