import { compareDecimal, formatScaled } from "@/lib/fixed-decimal"
import { formatQuantityUnits, minimumUnits, quantityUnits } from "./quantity"
import type {
    SourcingLineInput,
    SourcingProductLine,
    SourcingSalesOrder,
    SourcingSupplierOption,
} from "./types"

/**
 * 为当前销售单生成默认供给分配：先消化现有库存，采购缺口优先供应商直发。
 *
 * @param order 当前选源销售单。
 * @returns 可写入表单的选源行。
 */
export function buildDefaultSourcingLines(
    order?: SourcingSalesOrder,
): SourcingLineInput[] {
    if (!order) return []
    const stockCapacity = new Map<string, bigint>()
    const result: SourcingLineInput[] = []
    for (const line of order.lines) {
        const stock = allocateExistingStock(line, stockCapacity)
        for (const allocation of stock.lines) result.push(allocation)
        const purchase = allocatePurchaseResidual(
            line,
            stock.remaining,
            stock.lines.length,
        )
        if (purchase) result.push(purchase)
        if (
            !result.some(
                (input) => input.salesOrderLineId === line.salesOrderLineId,
            )
        ) {
            result.push({
                rowKey: `${line.salesOrderLineId}:0`,
                salesOrderLineId: line.salesOrderLineId,
                selected: true,
                quantity: line.remainingQuantity,
                basisId: "",
                targetWarehouseId: "",
                targetWarehouseName: "",
                expectedDeliveryDate: line.deliveryDeadline,
            })
        }
    }
    return result
}

/** 按库存覆盖量分配一条销售明细；容量表在整张销售单内共用。 */
function allocateExistingStock(
    line: SourcingProductLine,
    stockCapacity: Map<string, bigint>,
): { lines: SourcingLineInput[]; remaining: bigint } {
    let remaining = quantityUnits(line.remainingQuantity)
    const lines: SourcingLineInput[] = []
    const stockOptions = line.options
        .filter((option) => option.sourceType === "EXISTING_STOCK")
        .sort(compareStockOptions)
    for (const option of stockOptions) {
        if (remaining <= BigInt(0)) break
        const sourceCapacity =
            stockCapacity.get(option.basisId) ??
            quantityUnits(
                option.sourceAvailableQuantity ?? option.maxCreateQuantity,
            )
        const allocated = minimumUnits(
            remaining,
            sourceCapacity,
            quantityUnits(option.maxCreateQuantity),
        )
        if (allocated <= BigInt(0)) continue
        lines.push(
            sourcingInput(
                line.salesOrderLineId,
                lines.length,
                option,
                allocated,
            ),
        )
        remaining -= allocated
        stockCapacity.set(option.basisId, sourceCapacity - allocated)
    }
    return { lines, remaining }
}

/** 只对库存分配后的剩余缺口推荐采购，保留未覆盖数量。 */
function allocatePurchaseResidual(
    line: SourcingProductLine,
    remaining: bigint,
    allocationIndex: number,
): SourcingLineInput | undefined {
    if (remaining <= BigInt(0)) return undefined
    const purchase = pickBestSourcingOption(
        line.options.filter((option) => option.sourceType === "PURCHASE"),
        formatScaled(remaining, 6),
    )
    if (!purchase) return undefined
    const allocated = minimumUnits(
        remaining,
        quantityUnits(purchase.maxCreateQuantity),
    )
    return sourcingInput(
        line.salesOrderLineId,
        allocationIndex,
        purchase,
        allocated,
    )
}

/** 把推荐供给与定点数量投影为表单行。 */
const sourcingInput = (
    salesOrderLineId: string,
    index: number,
    option: SourcingSupplierOption,
    quantity: bigint,
): SourcingLineInput => ({
    rowKey: `${salesOrderLineId}:${index}`,
    salesOrderLineId,
    selected: true,
    quantity: formatQuantityUnits(quantity),
    basisId: option.basisId,
    targetWarehouseId: "",
    targetWarehouseName: "",
    expectedDeliveryDate: option.expectedDeliveryDate,
})

/** 现有库存推荐按可覆盖量降序，再按仓库名称稳定排序。 */
const compareStockOptions = (
    left: SourcingSupplierOption,
    right: SourcingSupplierOption,
): number => {
    const quantity = compareDecimalSafe(
        right.maxCreateQuantity,
        left.maxCreateQuantity,
        6,
    )
    if (quantity !== 0) return quantity
    return (left.warehouseName ?? left.supplierName).localeCompare(
        right.warehouseName ?? right.supplierName,
        "zh-CN",
    )
}

/**
 * 为一条销售明细选出最优供给。
 *
 * 排序：现有库存优先；采购方案中供应商直发优先，再比较剩余数量覆盖能力、
 * 含税成本、交期、可创建量与供给方名称。
 *
 * @param options 该明细的可用供给。
 * @param remainingQuantity 销售剩余待分配数量；缺省时不比较覆盖能力。
 * @returns 最优选项；无合格供给时为空。
 */
export function pickBestSourcingOption(
    options: readonly SourcingSupplierOption[],
    remainingQuantity?: string,
): SourcingSupplierOption | undefined {
    if (options.length === 0) return undefined
    return [...options].sort((left, right) => {
        if (left.sourceType !== right.sourceType) {
            return left.sourceType === "EXISTING_STOCK" ? -1 : 1
        }
        if (left.sourceType === "PURCHASE") {
            const leftDirect =
                left.fulfillmentResponsibility === "SUPPLIER_DIRECT"
            const rightDirect =
                right.fulfillmentResponsibility === "SUPPLIER_DIRECT"
            if (leftDirect !== rightDirect) return leftDirect ? -1 : 1
        }
        const leftCovers = optionCoversRemaining(left, remainingQuantity)
        const rightCovers = optionCoversRemaining(right, remainingQuantity)
        if (leftCovers !== rightCovers) return leftCovers ? -1 : 1
        const cost = compareDecimalSafe(
            left.unitCostGross,
            right.unitCostGross,
            4,
        )
        if (cost !== 0) return cost
        const leftDate = left.expectedDeliveryDate.trim()
        const rightDate = right.expectedDeliveryDate.trim()
        if (leftDate && rightDate) {
            const dateCmp = leftDate.localeCompare(rightDate)
            if (dateCmp !== 0) return dateCmp
        } else if (leftDate) return -1
        else if (rightDate) return 1
        const quantity = compareDecimalSafe(
            right.maxCreateQuantity,
            left.maxCreateQuantity,
            6,
        )
        if (quantity !== 0) return quantity
        return left.supplierName.localeCompare(right.supplierName, "zh-CN")
    })[0]
}

/** 只重新推荐参与本次分配的商品；暂停的商品保留完整方案且不占推荐库存。 */
export function assignBestSourcingOptions(
    order: SourcingSalesOrder | undefined,
    lines: readonly SourcingLineInput[],
): SourcingLineInput[] {
    if (!order) return lines.map((line) => ({ ...line }))
    const included = new Set(
        lines
            .filter((line) => line.selected)
            .map((line) => line.salesOrderLineId),
    )
    const recommended = buildDefaultSourcingLines({
        ...order,
        lines: order.lines.filter((product) =>
            included.has(product.salesOrderLineId),
        ),
    })
    return order.lines.flatMap((product) => {
        const current = lines.filter(
            (line) => line.salesOrderLineId === product.salesOrderLineId,
        )
        const next = recommended.filter(
            (line) => line.salesOrderLineId === product.salesOrderLineId,
        )
        return included.has(product.salesOrderLineId) &&
            next.some((line) => line.basisId)
            ? next
            : current.map((line) => ({ ...line }))
    })
}

function optionCoversRemaining(
    option: SourcingSupplierOption,
    remainingQuantity: string | undefined,
): boolean {
    if (!remainingQuantity) return true
    try {
        return (
            compareDecimal(option.maxCreateQuantity, remainingQuantity, 6) >= 0
        )
    } catch {
        return false
    }
}

function compareDecimalSafe(
    left: string,
    right: string,
    maxScale: number,
): -1 | 0 | 1 {
    try {
        return compareDecimal(left, right, maxScale)
    } catch {
        return left.localeCompare(right, "en") as -1 | 0 | 1
    }
}
