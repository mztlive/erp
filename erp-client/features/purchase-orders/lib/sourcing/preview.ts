import {
    multiplyFixed,
    splitGrossByPercentRate,
    sumFixed,
} from "@/lib/fixed-decimal"
import { findSourcingOption } from "./options"
import type {
    PurchaseOrderPreview,
    PurchaseOrderPreviewLine,
    SourcingLineInput,
    SourcingProductLine,
    SourcingSalesOrder,
    SourcingSupplierOption,
    StockAllocationPreviewLine,
} from "./types"

type PreviewGroup = {
    preview: Omit<PurchaseOrderPreview, "lines" | "totals">
    lines: PurchaseOrderPreviewLine[]
}

/**
 * 把已选定履约方案的明细按 §7.4 拆分维度预览成多张采购单。
 *
 * @param order 当前选源销售单。
 * @param lines 表单选源行。
 * @returns 按供应商、采购类型、付款条件、履约责任和目标仓分组的预览单。
 */
export function buildPurchaseOrderPreviews(
    order: SourcingSalesOrder | undefined,
    lines: readonly SourcingLineInput[],
): PurchaseOrderPreview[] {
    if (!order) return []
    const groups = new Map<string, PreviewGroup>()
    for (const input of lines) {
        if (!input.selected || !input.basisId) continue
        const product = order.lines.find(
            (line) => line.salesOrderLineId === input.salesOrderLineId,
        )
        const option = findSourcingOption(product, input.basisId)
        if (!product || !option) continue
        if (option.sourceType === "EXISTING_STOCK") continue
        const previewLine = purchasePreviewLine(product, option, input)
        const key = [
            option.supplierId,
            option.purchaseType,
            option.paymentTermCode,
            option.fulfillmentResponsibility,
            input.targetWarehouseId,
        ].join("|")
        const existing = groups.get(key)
        if (existing) {
            existing.lines.push(previewLine)
            continue
        }
        groups.set(key, purchasePreviewGroup(key, option, input, previewLine))
    }
    return [...groups.values()].map((group) => ({
        ...group.preview,
        lines: group.lines,
        totals: sumPreviewTotals(group.lines),
    }))
}

/** 预览金额按行先舍入；表头只汇总已舍入结果。 */
function purchasePreviewLine(
    product: SourcingProductLine,
    option: SourcingSupplierOption,
    input: SourcingLineInput,
): PurchaseOrderPreviewLine {
    const amounts = previewLineAmounts(
        option.unitCostGross,
        option.inputTaxRate,
        input.quantity,
    )
    return {
        salesOrderLineId: product.salesOrderLineId,
        itemName: product.itemName,
        itemSku: product.itemSku,
        unit: product.unit,
        quantity: input.quantity.trim(),
        unitCostGross: option.unitCostGross,
        inputTaxRate: option.inputTaxRate,
        expectedDeliveryDate: input.expectedDeliveryDate,
        grossAmount: amounts.gross,
        netAmount: amounts.net,
        taxAmount: amounts.tax,
    }
}

/** 新分组保留首行方案身份，只有入仓采购显示目标仓资料。 */
function purchasePreviewGroup(
    key: string,
    option: SourcingSupplierOption,
    input: SourcingLineInput,
    line: PurchaseOrderPreviewLine,
): PreviewGroup {
    return {
        preview: {
            key,
            supplierId: option.supplierId,
            supplierName: option.supplierName,
            purchaseType: option.purchaseType,
            fulfillmentResponsibility: option.fulfillmentResponsibility,
            paymentTermCode: option.paymentTermCode,
            paymentTermLabel: option.paymentTermLabel,
            workItemId: option.workItemId,
            basisId: option.basisId,
            targetWarehouseId:
                option.fulfillmentResponsibility === "WAREHOUSE"
                    ? input.targetWarehouseId || undefined
                    : undefined,
            targetWarehouseName:
                option.fulfillmentResponsibility === "WAREHOUSE"
                    ? input.targetWarehouseName || undefined
                    : undefined,
        },
        lines: [line],
    }
}

/** 把已选现有库存方案投影为确认清单。 */
export function buildStockAllocationPreviews(
    order: SourcingSalesOrder | undefined,
    lines: readonly SourcingLineInput[],
): StockAllocationPreviewLine[] {
    if (!order) return []
    return lines.flatMap((input) => {
        if (!input.selected || !input.basisId) return []
        const product = order.lines.find(
            (line) => line.salesOrderLineId === input.salesOrderLineId,
        )
        const option = findSourcingOption(product, input.basisId)
        if (!product || option?.sourceType !== "EXISTING_STOCK") return []
        return [
            {
                salesOrderLineId: product.salesOrderLineId,
                itemName: product.itemName,
                warehouseName:
                    option.warehouseName ?? option.supplierName ?? "公司仓库",
                quantity: input.quantity.trim(),
                unit: product.unit,
            },
        ]
    })
}

/**
 * 按含税成本和进项税率预估一行金额。
 *
 * @param unitCostGross 含税成本。
 * @param inputTaxRate 进项税率（小数，如 `0.13`）。
 * @param quantity 本次分配数量。
 * @returns 行含税、不含税和税额；非法数值时返回零。
 */
export function previewLineAmounts(
    unitCostGross: string,
    inputTaxRate: string,
    quantity: string,
): { gross: string; net: string; tax: string } {
    try {
        const gross = multiplyFixed(unitCostGross, quantity, {
            leftMaxScale: 4,
            rightMaxScale: 6,
            outputScale: 2,
        })
        const taxRatePercent = multiplyFixed(inputTaxRate, "100", {
            leftMaxScale: 6,
            rightMaxScale: 0,
            outputScale: 2,
        })
        return splitGrossByPercentRate(gross, taxRatePercent)
    } catch {
        return { gross: "0.00", net: "0.00", tax: "0.00" }
    }
}

/**
 * 汇总预览行已舍入金额。
 *
 * @param lines 预览明细。
 * @returns 表头含税、不含税和税额。
 */
export function sumPreviewTotals(lines: readonly PurchaseOrderPreviewLine[]): {
    gross: string
    net: string
    tax: string
} {
    return {
        gross: sumFixed(
            lines.map((line) => line.grossAmount),
            { maxScale: 2, outputScale: 2 },
        ),
        net: sumFixed(
            lines.map((line) => line.netAmount),
            { maxScale: 2, outputScale: 2 },
        ),
        tax: sumFixed(
            lines.map((line) => line.taxAmount),
            { maxScale: 2, outputScale: 2 },
        ),
    }
}
