import ExcelJS from "exceljs"
import {
    compareDecimal,
    compactFixed,
    divideFixed,
    parseDecimal,
} from "@/lib/fixed-decimal"
import type { SupplierOfferingView } from "@/features/supplier-offerings/types"
import type { ProcurementInquiry } from "../types"
import { inquiryDateInput } from "./format"
import {
    latestSupplierQuotes,
    type InternalQuote,
    type QuoteTerms,
} from "./internal-quotes"
import { SUPPLIER_QUOTE_HEADERS } from "./supplier-quote-workbook"

export type ImportedQuote = {
    source: string
    sku_id: string
    product: string
    supplier: string
    status: "新增" | "新版本" | "已导入"
    previous?: InternalQuote
    quote: {
        quote_id: string
        revision_no: number
        line_id: string
        supplier_id: string
        supplier_offering_id: string
        terms: QuoteTerms
        internal_note: string
    }
}

/** 解析前检查 ZIP 中央目录，限制展开体积；不执行公式或外部链接。 */
function checkArchive(buffer: ArrayBuffer) {
    const view = new DataView(buffer)
    let end = buffer.byteLength - 22
    const lower = Math.max(0, end - 65535)
    while (end >= lower && view.getUint32(end, true) !== 0x06054b50) end--
    if (end < lower) throw new Error("不是有效的 XLSX 文件")
    const count = view.getUint16(end + 10, true)
    let offset = view.getUint32(end + 16, true)
    if (!count || count > 4096)
        throw new Error("工作簿内容过多，请使用系统生成的报价表")
    let total = 0
    for (let index = 0; index < count; index++) {
        if (offset + 46 > end || view.getUint32(offset, true) !== 0x02014b50)
            throw new Error("工作簿压缩结构无效")
        const size = view.getUint32(offset + 24, true)
        total += size
        if (size > 32 * 1024 * 1024 || total > 100 * 1024 * 1024)
            throw new Error("工作簿展开内容过大，请减少图片后重试")
        const length = view.getUint16(offset + 28, true)
        const path = new TextDecoder().decode(
            new Uint8Array(buffer, offset + 46, length),
        )
        if (/vbaProject|externalLinks|\.\./i.test(path))
            throw new Error("报价表不能包含宏或外部链接")
        offset +=
            46 +
            length +
            view.getUint16(offset + 30, true) +
            view.getUint16(offset + 32, true)
    }
}

function cellText(cell: ExcelJS.Cell): string {
    const value = cell.value
    if (value == null) return ""
    if (value instanceof Date)
        return `${value.getUTCFullYear()}-${String(value.getUTCMonth() + 1).padStart(2, "0")}-${String(value.getUTCDate()).padStart(2, "0")}`
    if (typeof value === "object")
        throw new Error(
            `${cell.address}：请填写数值或文字，不使用公式、链接或富文本`,
        )
    return String(value).trim()
}
function decimal(value: string, scale: number, name: string, positive = false) {
    try {
        parseDecimal(value, { maxScale: scale })
        if (positive && compareDecimal(value, "0", scale) <= 0)
            throw new Error()
        return compactFixed(value)
    } catch {
        throw new Error(
            `${name}须填写${positive ? "正数" : "非负数"}，最多 ${scale} 位小数`,
        )
    }
}
function date(value: string, name: string, optional = false): string | null {
    if (!value && optional) return null
    if (
        !/^\d{4}-\d{2}-\d{2}$/.test(value) ||
        !Number.isFinite(new Date(`${value}T00:00:00Z`).getTime()) ||
        new Date(`${value}T00:00:00Z`).toISOString().slice(0, 10) !== value
    )
        throw new Error(`${name}须为有效日期 YYYY-MM-DD`)
    return value
}
function termsFor(row: ExcelJS.Row, quantityScale: number): QuoteTerms {
    const quantity = decimal(
        cellText(row.getCell(13)),
        quantityScale,
        "可供数量",
    )
    if (compareDecimal(quantity, "0", quantityScale) === 0)
        return {
            unavailable: true,
            unit_cost_gross: "0",
            input_tax_rate: "0",
            available_quantity: "0",
            expected_delivery_date: date(
                cellText(row.getCell(8)),
                "要求交付日",
            )!,
            valid_to: null,
            freight_gross: null,
        }
    const percent = decimal(cellText(row.getCell(12)), 4, "税率")
    if (compareDecimal(percent, "100", 4) > 0)
        throw new Error("税率不得超过 100%")
    return {
        unavailable: false,
        unit_cost_gross: decimal(
            cellText(row.getCell(11)),
            4,
            "含税单价",
            true,
        ),
        input_tax_rate: compactFixed(
            divideFixed(percent, "100", {
                numeratorMaxScale: 4,
                denominatorMaxScale: 0,
                outputScale: 6,
            }),
        ),
        available_quantity: quantity,
        expected_delivery_date: date(cellText(row.getCell(14)), "承诺交付日")!,
        freight_gross: decimal(
            cellText(row.getCell(15)),
            2,
            "运费（已含运费填写 0）",
        ),
        valid_to: date(cellText(row.getCell(16)), "报价失效日", true),
    }
}
function sameTerms(a: QuoteTerms, b: QuoteTerms) {
    return (
        ["unit_cost_gross", "input_tax_rate", "available_quantity"].every(
            (key) =>
                compareDecimal(
                    a[key as "unit_cost_gross"],
                    b[key as "unit_cost_gross"],
                    6,
                ) === 0,
        ) &&
        (a.freight_gross == null
            ? b.freight_gross == null
            : b.freight_gross != null &&
              compareDecimal(a.freight_gross, b.freight_gross, 2) === 0) &&
        Boolean(a.unavailable) === Boolean(b.unavailable) &&
        a.expected_delivery_date === b.expected_delivery_date &&
        a.valid_to === b.valid_to
    )
}

function parseRow(
    row: ExcelJS.Row,
    supplierId: string,
    inquiry: ProcurementInquiry,
    offerings: readonly SupplierOfferingView[],
    prior: InternalQuote[],
    units: ReadonlyMap<string, string>,
): Omit<ImportedQuote, "source"> | null {
    if (
        Array.from({ length: 7 }, (_, index) =>
            cellText(row.getCell(11 + index)),
        ).every((value) => !value)
    )
        return null
    const identity = JSON.parse(cellText(row.getCell(18))) as {
        lineId: string
        skuId: string
    }
    const line = inquiry.draft.lines.find(
        (item) =>
            item.line_id === identity.lineId && item.sku_id === identity.skuId,
    )
    if (!line) throw new Error("商品行标识与当前需求不匹配，请重新生成报价表")
    if (
        cellText(row.getCell(4)) !== line.sku_code ||
        cellText(row.getCell(6)) !== units.get(line.unit_code) ||
        compareDecimal(cellText(row.getCell(7)), line.quantity, 6) !== 0 ||
        cellText(row.getCell(8)) !== inquiryDateInput(line.delivery_due_at) ||
        cellText(row.getCell(9)) !== line.delivery_terms.trim()
    )
        throw new Error(
            "原商品、单位、需求数量或交付要求已被修改，请保留原需求列",
        )
    const candidates = offerings.filter(
        (item) =>
            item.supplier_id === supplierId &&
            item.sku_id === line.sku_id &&
            item.status === "ACTIVE",
    )
    const previous = latestSupplierQuotes(prior).find(
        (item) =>
            item.line_id === line.line_id && item.supplier_id === supplierId,
    )
    const offering =
        candidates.find((item) => item.id === previous?.supplier_offering_id) ??
        (candidates.length === 1 ? candidates[0] : undefined)
    if (!offering)
        throw new Error(
            candidates.length
                ? "存在多个供给关系，请先手工登记以确定本次使用的供给"
                : "没有当前可用供给关系，请维护供应商供给后重试",
        )
    const revision =
        previous?.supplier_offering_id === offering.id ? previous : undefined
    const terms = termsFor(row, line.quantity_scale)
    if (compareDecimal(terms.available_quantity, line.quantity, 6) > 0)
        throw new Error("可供数量不得超过本行需求数量")
    const note = cellText(row.getCell(17))
    if (terms.unavailable && !note)
        throw new Error("无法供货时请填写报价备注说明原因")
    if (note.length > 4000) throw new Error("报价备注不得超过 4000 字")
    const duplicate = prior.some(
        (item) =>
            item.line_id === line.line_id &&
            item.supplier_id === supplierId &&
            item.supplier_offering_id === offering.id &&
            item.feedback_round === inquiry.feedback_round + 1 &&
            sameTerms(item.terms, terms) &&
            item.internal_note === note,
    )
    return {
        sku_id: line.sku_id,
        product: line.product_name,
        supplier: offering.supplier_name ?? "供应商",
        status: duplicate ? "已导入" : previous ? "新版本" : "新增",
        previous,
        quote: {
            quote_id: revision?.quote_id ?? crypto.randomUUID(),
            revision_no: revision ? revision.revision_no + 1 : 1,
            line_id: line.line_id,
            supplier_id: supplierId,
            supplier_offering_id: offering.id,
            terms,
            internal_note: note,
        },
    }
}

export async function parseSupplierQuoteFiles(
    files: File[],
    inquiry: ProcurementInquiry,
    offerings: readonly SupplierOfferingView[],
    prior: InternalQuote[],
    units: ReadonlyMap<string, string>,
) {
    if (
        !files.length ||
        files.length > 20 ||
        files.reduce((sum, file) => sum + file.size, 0) > 50 * 1024 * 1024
    )
        throw new Error("每次选择 1 至 20 个 XLSX 文件，合计不超过 50 MiB")
    const rows: ImportedQuote[] = []
    const issues: string[] = []
    let skipped = 0
    const seen = new Set<string>()
    for (const file of files) {
        try {
            if (
                !file.name.toLowerCase().endsWith(".xlsx") ||
                file.size > 20 * 1024 * 1024
            )
                throw new Error("须为不超过 20 MiB 的 XLSX 文件")
            const buffer = await file.arrayBuffer()
            checkArchive(buffer)
            const workbook = new ExcelJS.Workbook()
            await workbook.xlsx.load(buffer)
            const metadata = workbook.getWorksheet("回导信息")
            if (
                !metadata ||
                cellText(metadata.getCell("A1")) !== "supplier-inquiry-quote-v2"
            )
                throw new Error("该文件不支持回导，请重新生成新版报价表后填写")
            if (
                cellText(metadata.getCell("A2")) !== inquiry.id ||
                cellText(metadata.getCell("A3")) !==
                    String(inquiry.requirement_version) ||
                cellText(metadata.getCell("A4")) !==
                    String(inquiry.feedback_round + 1)
            )
                throw new Error(
                    "报价表不属于当前询价、需求版本或反馈轮次，请重新生成",
                )
            const supplier = cellText(metadata.getCell("A5"))
            const sheet = workbook.getWorksheet("报价表")
            if (!sheet || sheet.rowCount > 204)
                throw new Error("报价表缺失或超过 200 条商品")
            SUPPLIER_QUOTE_HEADERS.forEach((header, index) => {
                if (cellText(sheet.getCell(4, index + 1)) !== header)
                    throw new Error("报价表列已变化，请使用原模板")
            })
            for (let number = 5; number <= sheet.rowCount; number++) {
                const source = `${file.name} 第 ${number} 行`
                try {
                    const value = parseRow(
                        sheet.getRow(number),
                        supplier,
                        inquiry,
                        offerings,
                        prior,
                        units,
                    )
                    if (!value) {
                        skipped++
                        continue
                    }
                    const key = `${supplier}:${value.quote.line_id}`
                    if (seen.has(key))
                        throw new Error(
                            "同一供应商商品有多份回复，请只保留一份",
                        )
                    seen.add(key)
                    rows.push({ ...value, source })
                } catch (error) {
                    issues.push(
                        `${source}：${error instanceof Error ? error.message : "格式错误"}`,
                    )
                }
            }
        } catch (error) {
            issues.push(
                `${file.name}：${error instanceof Error ? error.message : "无法读取工作簿"}`,
            )
        }
    }
    if (rows.length > 1000) issues.push("每次最多导入 1000 条回复，请减少文件")
    return { rows, issues, skipped }
}
