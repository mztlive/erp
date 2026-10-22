import { sumFixed, compactFixed } from "@/lib/fixed-decimal"
import type { InquiryLine } from "../types"

export type SupplierQuoteLine = {
    lineId: string
    skuId: string
    skuCode: string
    name: string
    specification: string
    unit: string
    quantity: string
    dueAt: number
    deliveryTerms: string
    imageAssetId?: string
    issue?: string
    regions: { name: string; addresses: number; quantity: string }[]
}
export type SupplierQuoteGroup = {
    supplierId: string
    supplierName: string
    supplierNo: string
    lines: (SupplierQuoteLine & {
        supplierCodes: string[]
        statuses: string[]
    })[]
}
export type SupplierQuoteSource = {
    inquiryId: string
    inquiryNo: string
    version: number
    requirementVersion: number
    deadlineAt: number
    taskId: string
    taskVersion: number
    lines: SupplierQuoteLine[]
    groups: SupplierQuoteGroup[]
}

/** 外发字段白名单；客户、收件人、电话、详细地址及内部价格不得进入导出模型。 */
export function supplierQuoteLine(
    line: InquiryLine,
    unit: string,
    imageAssetId?: string,
): SupplierQuoteLine {
    const regions = new Map<string, InquiryLine["allocations"]>()
    for (const allocation of line.allocations) {
        const key = `${allocation.region_code}:${allocation.region_name.trim()}`
        regions.set(key, [...(regions.get(key) ?? []), allocation])
    }
    return {
        lineId: line.line_id,
        skuId: line.sku_id,
        skuCode: line.sku_code,
        name: line.product_name,
        specification: line.specification,
        unit,
        quantity: line.quantity,
        dueAt: line.delivery_due_at,
        deliveryTerms: line.delivery_terms,
        imageAssetId,
        issue: unit
            ? undefined
            : "商品图片与单位资料未匹配，请核对商品状态后重试",
        regions: [...regions.values()].map((allocations) => ({
            name: allocations[0]?.region_name.trim() || "地区未填写",
            addresses: allocations.length,
            quantity: compactFixed(
                sumFixed(
                    allocations.map((item) => item.quantity),
                    {
                        maxScale: line.quantity_scale,
                        outputScale: line.quantity_scale,
                    },
                ),
            ),
        })),
    }
}

/** 同一供应商同一需求行只出现一次；不同交期的同 SKU 需求行分别保留。 */
export function groupSupplierQuotes(
    lines: SupplierQuoteLine[],
    offerings: readonly {
        sku_id: string
        supplier_id: string
        supplier_name?: string | null
        supplier_no?: string | null
        supplier_sku_code: string
        status: string
    }[],
): SupplierQuoteGroup[] {
    const groups = new Map<string, SupplierQuoteGroup>()
    for (const line of lines) {
        for (const offering of offerings.filter(
            (item) => item.sku_id === line.skuId,
        )) {
            const name = offering.supplier_name?.trim()
            if (!name)
                throw new Error(
                    `${line.name}的供应商名称缺失，请维护供应商资料后重试`,
                )
            const group = groups.get(offering.supplier_id) ?? {
                supplierId: offering.supplier_id,
                supplierName: name,
                supplierNo: offering.supplier_no?.trim() ?? "",
                lines: [],
            }
            const existing = group.lines.find(
                (item) => item.lineId === line.lineId,
            )
            if (existing) {
                existing.supplierCodes = [
                    ...new Set([
                        ...existing.supplierCodes,
                        offering.supplier_sku_code,
                    ]),
                ].sort()
                existing.statuses = [
                    ...new Set([...existing.statuses, offering.status]),
                ].sort()
            } else {
                group.lines.push({
                    ...line,
                    supplierCodes: [offering.supplier_sku_code],
                    statuses: [offering.status],
                })
            }
            groups.set(group.supplierId, group)
        }
    }
    return [...groups.values()].sort(
        (a, b) =>
            a.supplierName.localeCompare(b.supplierName, "zh-CN") ||
            a.supplierId.localeCompare(b.supplierId),
    )
}

export function selectedSupplierQuotes(
    source: SupplierQuoteSource,
    lineIds: readonly string[],
    supplierIds: readonly string[],
): SupplierQuoteGroup[] {
    return source.groups
        .filter((group) => supplierIds.includes(group.supplierId))
        .map((group) => ({
            ...group,
            lines: group.lines.filter((line) => lineIds.includes(line.lineId)),
        }))
        .filter((group) => group.lines.length > 0)
}

export function quoteFilePart(value: string): string {
    return (
        Array.from(value, (character) =>
            character.charCodeAt(0) < 32 ? "_" : character,
        )
            .join("")
            .replace(/[\\/:*?"<>|]/g, "_")
            .trim()
            .replace(/[. ]+$/g, "")
            .slice(0, 80) || "供应商"
    )
}

/** 清洗后同名（含大小写差异）的供应商文件使用序号区分，禁止 ZIP 静默覆盖。 */
export function supplierQuoteFileNames(
    groups: readonly SupplierQuoteGroup[],
): string[] {
    const used = new Set<string>()
    return groups.map((group) => {
        const base = `${quoteFilePart(group.supplierName)}报价表`
        let name = `${base}.xlsx`
        let sequence = 2
        while (used.has(name.normalize("NFKC").toLowerCase())) {
            name = `${base}（${sequence}）.xlsx`
            sequence += 1
        }
        used.add(name.normalize("NFKC").toLowerCase())
        return name
    })
}
