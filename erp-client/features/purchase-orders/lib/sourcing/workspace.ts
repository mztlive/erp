import { compareDecimal, multiplyFixed, sumFixed } from "@/lib/fixed-decimal"
import type { PurchaseCreationBasis } from "@/features/purchase-orders/types"
import { findSourcingOption } from "./options"
import { buildDefaultSourcingLines } from "./recommendation"
import type {
    SourcingLineInput,
    SourcingOrderSummary,
    SourcingProductLine,
    SourcingSalesOrder,
    SourcingSupplierOption,
} from "./types"

type WorkspaceBucket = {
    order: Omit<SourcingSalesOrder, "lines">
    lines: Map<string, SourcingProductLine>
}

type CreationBasisLine = PurchaseCreationBasis["lines"][number]

/**
 * 把精确创建依据转成按销售单聚合的选源工作区。
 *
 * @param bases 当前账号可消费的创建依据。
 * @returns 按销售单号稳定排序的选源销售单。
 */
export function buildSourcingWorkspace(
    bases: readonly PurchaseCreationBasis[],
): SourcingSalesOrder[] {
    const bySalesOrder = new Map<string, WorkspaceBucket>()
    for (const basis of bases) {
        if (basis.consumed) continue
        let bucket = bySalesOrder.get(basis.salesOrderId)
        if (!bucket) {
            bucket = workspaceBucket(basis)
            bySalesOrder.set(basis.salesOrderId, bucket)
        }
        for (const line of basis.lines) {
            appendBasisLine(bucket.lines, basis, line)
        }
    }
    return [...bySalesOrder.values()]
        .map((bucket) => ({
            ...bucket.order,
            lines: [...bucket.lines.values()].map((line) => ({
                ...line,
                options: [...line.options].sort(compareWorkspaceOptions),
            })),
        }))
        .sort((left, right) =>
            left.salesOrderNo.localeCompare(right.salesOrderNo, "zh-CN"),
        )
}

/** 首次出现的创建依据确定销售单身份，后续依据只补充可选方案。 */
function workspaceBucket(basis: PurchaseCreationBasis): WorkspaceBucket {
    return {
        order: {
            salesOrderId: basis.salesOrderId,
            salesOrderNo: basis.salesOrderNo,
            customerName: basis.customerName,
            contractNumber: basis.contractNumber,
            salesOwnerName: basis.salesOwnerName,
            workItemId: basis.workItemId,
        },
        lines: new Map(),
    }
}

/** 同一销售明细保留首个投影，并按精确依据身份追加不同方案。 */
function appendBasisLine(
    lines: Map<string, SourcingProductLine>,
    basis: PurchaseCreationBasis,
    line: CreationBasisLine,
): void {
    const option = optionFromBasis(basis, line)
    const existing = lines.get(line.salesOrderLineId)
    if (!existing) {
        lines.set(line.salesOrderLineId, productFromBasisLine(line, option))
        return
    }
    if (
        existing.options.some(
            (candidate) => candidate.basisId === option.basisId,
        )
    )
        return
    lines.set(line.salesOrderLineId, {
        ...existing,
        options: [...existing.options, option],
    })
}

/** 精确创建依据与明细成本共同组成一条可选履约方案。 */
function optionFromBasis(
    basis: PurchaseCreationBasis,
    line: CreationBasisLine,
): SourcingSupplierOption {
    return {
        sourceType: basis.sourceType,
        supplierId: basis.supplierId,
        supplierName: basis.supplierName,
        basisId: basis.basisId,
        workItemId: basis.workItemId,
        purchaseType: basis.purchaseType,
        fulfillmentResponsibility: basis.fulfillmentResponsibility,
        paymentTermCode: basis.paymentTermCode,
        paymentTermLabel: basis.paymentTermLabel,
        businessCategory: basis.businessCategory,
        stockBalanceId: basis.stockBalanceId,
        warehouseId: basis.warehouseId,
        warehouseName: basis.warehouseName,
        sourceAvailableQuantity: basis.sourceAvailableQuantity,
        unitCostGross: line.unitCostGross,
        inputTaxRate: line.inputTaxRate,
        maxCreateQuantity: line.maxCreateQuantity,
        expectedDeliveryDate: line.expectedDeliveryDate,
    }
}

/** 把销售行事实投影为选源明细，不替换服务端数量或履约期限。 */
function productFromBasisLine(
    line: CreationBasisLine,
    option: SourcingSupplierOption,
): SourcingProductLine {
    return {
        salesOrderLineId: line.salesOrderLineId,
        itemName: line.itemName,
        itemSku: line.itemSku,
        unit: line.unit,
        quantityScale: line.quantityScale,
        salesQuantity: line.salesQuantity,
        coveredQuantity: line.coveredQuantity,
        remainingQuantity: line.remainingQuantity,
        deliveryDeadline: line.salesDeliveryDeadline,
        salesAllocationLabel: line.salesAllocationLabel,
        options: [option],
    }
}

/** 工作区方案按供应商名称、履约责任和精确依据稳定排序。 */
function compareWorkspaceOptions(
    left: SourcingSupplierOption,
    right: SourcingSupplierOption,
): number {
    const supplier = left.supplierName.localeCompare(
        right.supplierName,
        "zh-CN",
    )
    if (supplier !== 0) return supplier
    const responsibility = left.fulfillmentResponsibility.localeCompare(
        right.fulfillmentResponsibility,
        "en",
    )
    if (responsibility !== 0) return responsibility
    return left.basisId.localeCompare(right.basisId, "en")
}

/**
 * 表单选源行是否已与当前销售单明细对齐。
 *
 * 从工作台带着 `salesOrderId` 进入时，选中项在创建依据到达前就已确定；
 * 未对齐时页面只展示骨架，不能把空表单当成“没有销售明细”。
 *
 * @param lines 表单当前选源行。
 * @param order 当前选中的销售单。
 * @returns 行数与稳定销售行 ID 均一致时为 true。
 */
export function sourcingFormLinesReady(
    lines: readonly SourcingLineInput[],
    order: SourcingSalesOrder,
): boolean {
    return (
        lines.length >= order.lines.length &&
        order.lines.every((product) =>
            lines.some(
                (line) => line.salesOrderLineId === product.salesOrderLineId,
            ),
        )
    )
}

/**
 * 汇总一张选源销售单的行数、供给和推荐采购含税估算。
 *
 * @param order 当前选源销售单。
 * @returns 用于来源区展示的汇总。
 */
export function summarizeSourcingOrder(
    order: SourcingSalesOrder,
): SourcingOrderSummary {
    const options = order.lines.flatMap((line) => [...line.options])
    const purchaseOptions = options.filter(
        (option) => option.sourceType === "PURCHASE",
    )
    const amounts = recommendedPurchaseAmounts(order)
    return {
        lineCount: order.lines.length,
        coveredLineCount: order.lines.filter((line) =>
            isPositiveQuantity(line.coveredQuantity),
        ).length,
        uniqueSupplierCount: uniqueStable(
            purchaseOptions.map((option) => option.supplierId).filter(Boolean),
        ).length,
        purchaseTypes: uniqueStable(
            purchaseOptions.map((option) => option.purchaseType),
        ),
        fulfillmentResponsibilities: uniqueStable(
            options.map((option) => option.fulfillmentResponsibility),
        ),
        paymentTermLabels: uniqueStable(
            purchaseOptions.map((option) => option.paymentTermLabel),
        ),
        businessCategories: uniqueStable(
            purchaseOptions
                .map((option) => option.businessCategory?.trim() ?? "")
                .filter(Boolean),
        ),
        minEstimatedGross: sumFixed(amounts, { maxScale: 2, outputScale: 2 }),
    }
}

/** 来源摘要只估算推荐采购缺口，库存分配不计入采购含税金额。 */
function recommendedPurchaseAmounts(order: SourcingSalesOrder): string[] {
    return buildDefaultSourcingLines(order).flatMap((input) => {
        const product = order.lines.find(
            (line) => line.salesOrderLineId === input.salesOrderLineId,
        )
        const option = findSourcingOption(product, input.basisId)
        if (!option || option.sourceType !== "PURCHASE") return []
        try {
            return [
                multiplyFixed(option.unitCostGross, input.quantity, {
                    leftMaxScale: 4,
                    rightMaxScale: 6,
                    outputScale: 2,
                }),
            ]
        } catch {
            return []
        }
    })
}

function isPositiveQuantity(value: string): boolean {
    try {
        return compareDecimal(value, "0", 6) > 0
    } catch {
        return false
    }
}

function uniqueStable<T>(values: readonly T[]): T[] {
    const seen = new Set<T>()
    const result: T[] = []
    for (const value of values) {
        if (seen.has(value)) continue
        seen.add(value)
        result.push(value)
    }
    return result
}
