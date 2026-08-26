import {
    compareDecimal,
    formatScaled,
    multiplyFixed,
    parseDecimal,
    splitGrossByPercentRate,
    sumFixed,
} from "@/lib/fixed-decimal"
import type {
    FulfillmentResponsibility,
    PurchaseCreationBasis,
    PurchaseType,
    SupplySourceType,
} from "@/features/purchase-orders/types"

/** 一条销售明细可选的合格履约方案。 */
export type SourcingSupplierOption = Readonly<{
    sourceType: SupplySourceType
    supplierId: string
    supplierName: string
    basisId: string
    workItemId: string
    purchaseType: PurchaseType
    fulfillmentResponsibility: FulfillmentResponsibility
    paymentTermCode: string
    paymentTermLabel: string
    businessCategory?: string
    stockBalanceId?: string
    warehouseId?: string
    warehouseName?: string
    sourceAvailableQuantity?: string
    unitCostGross: string
    inputTaxRate: string
    maxCreateQuantity: string
    expectedDeliveryDate: string
}>

/** 选源工作区中的一条销售明细。 */
export type SourcingProductLine = Readonly<{
    salesOrderLineId: string
    itemName: string
    itemSku?: string
    unit: string
    salesQuantity: string
    coveredQuantity: string
    remainingQuantity: string
    /** 销售承诺的最晚交付日，采购预计交付日不得晚于此日。 */
    deliveryDeadline: string
    salesAllocationLabel: string
    options: readonly SourcingSupplierOption[]
}>

/** 一张可分配供给的销售单及其剩余明细。 */
export type SourcingSalesOrder = Readonly<{
    salesOrderId: string
    salesOrderNo: string
    customerName: string
    contractNumber?: string
    salesOwnerName?: string
    workItemId: string
    lines: readonly SourcingProductLine[]
}>

/** 建单表单中一条可编辑选源行。 */
export type SourcingLineInput = {
    rowKey: string
    salesOrderLineId: string
    selected: boolean
    quantity: string
    basisId: string
    expectedDeliveryDate: string
}

/** 确认创建前按拆分维度预览的一张采购单。 */
export type PurchaseOrderPreview = Readonly<{
    key: string
    supplierId: string
    supplierName: string
    purchaseType: PurchaseType
    fulfillmentResponsibility: FulfillmentResponsibility
    paymentTermCode: string
    paymentTermLabel: string
    workItemId: string
    basisId: string
    lines: readonly PurchaseOrderPreviewLine[]
    totals: Readonly<{ gross: string; net: string; tax: string }>
}>

/** 预览采购单中的一行。 */
export type PurchaseOrderPreviewLine = Readonly<{
    salesOrderLineId: string
    itemName: string
    itemSku?: string
    unit: string
    quantity: string
    unitCostGross: string
    inputTaxRate: string
    expectedDeliveryDate: string
    grossAmount: string
    netAmount: string
    taxAmount: string
}>

/** 确认前展示的一条现有库存分配。 */
export type StockAllocationPreviewLine = Readonly<{
    salesOrderLineId: string
    itemName: string
    warehouseName: string
    quantity: string
    unit: string
}>

/**
 * 把精确创建依据转成按销售单聚合的选源工作区。
 *
 * @param bases 当前账号可消费的创建依据。
 * @returns 按销售单号稳定排序的选源销售单。
 */
export function buildSourcingWorkspace(
    bases: readonly PurchaseCreationBasis[],
): SourcingSalesOrder[] {
    const bySalesOrder = new Map<
        string,
        {
            order: Omit<SourcingSalesOrder, "lines">
            lines: Map<string, SourcingProductLine>
        }
    >()
    for (const basis of bases) {
        if (basis.consumed) continue
        let bucket = bySalesOrder.get(basis.salesOrderId)
        if (!bucket) {
            bucket = {
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
            bySalesOrder.set(basis.salesOrderId, bucket)
        }
        for (const line of basis.lines) {
            const option: SourcingSupplierOption = {
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
            const existing = bucket.lines.get(line.salesOrderLineId)
            if (!existing) {
                bucket.lines.set(line.salesOrderLineId, {
                    salesOrderLineId: line.salesOrderLineId,
                    itemName: line.itemName,
                    itemSku: line.itemSku,
                    unit: line.unit,
                    salesQuantity: line.salesQuantity,
                    coveredQuantity: line.coveredQuantity,
                    remainingQuantity: line.remainingQuantity,
                    deliveryDeadline: line.salesDeliveryDeadline,
                    salesAllocationLabel: line.salesAllocationLabel,
                    options: [option],
                })
                continue
            }
            if (
                existing.options.some(
                    (candidate) => candidate.basisId === option.basisId,
                )
            ) {
                continue
            }
            bucket.lines.set(line.salesOrderLineId, {
                ...existing,
                options: [...existing.options, option],
            })
        }
    }
    return [...bySalesOrder.values()]
        .map((bucket) => ({
            ...bucket.order,
            lines: [...bucket.lines.values()].map((line) => ({
                ...line,
                options: [...line.options].sort((left, right) => {
                    const supplier = left.supplierName.localeCompare(
                        right.supplierName,
                        "zh-CN",
                    )
                    if (supplier !== 0) return supplier
                    const responsibility =
                        left.fulfillmentResponsibility.localeCompare(
                            right.fulfillmentResponsibility,
                            "en",
                        )
                    if (responsibility !== 0) return responsibility
                    return left.basisId.localeCompare(right.basisId, "en")
                }),
            })),
        }))
        .sort((left, right) =>
            left.salesOrderNo.localeCompare(right.salesOrderNo, "zh-CN"),
        )
}

/**
 * 为当前销售单生成默认供给分配：现有库存优先，剩余缺口再匹配采购。
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
        let remaining = quantityUnits(line.remainingQuantity)
        let allocationIndex = 0
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
            result.push(
                sourcingInput(
                    line.salesOrderLineId,
                    allocationIndex,
                    option,
                    allocated,
                ),
            )
            allocationIndex += 1
            remaining -= allocated
            stockCapacity.set(option.basisId, sourceCapacity - allocated)
        }
        if (remaining > BigInt(0)) {
            const purchase = pickBestSourcingOption(
                line.options.filter(
                    (option) => option.sourceType === "PURCHASE",
                ),
                formatScaled(remaining, 6),
            )
            if (purchase) {
                const allocated = minimumUnits(
                    remaining,
                    quantityUnits(purchase.maxCreateQuantity),
                )
                result.push(
                    sourcingInput(
                        line.salesOrderLineId,
                        allocationIndex,
                        purchase,
                        allocated,
                    ),
                )
            }
        }
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
                expectedDeliveryDate: line.deliveryDeadline,
            })
        }
    }
    return result
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
    expectedDeliveryDate: option.expectedDeliveryDate,
})

/** 将最多六位小数的数量转成六位定点整数。 */
const quantityUnits = (value: string): bigint => {
    const parsed = parseDecimal(value, { maxScale: 6 })
    return parsed.unscaled * BigInt(10) ** BigInt(6 - parsed.scale)
}

/** 将六位定点整数格式化为不带无意义尾零的数量文本。 */
const formatQuantityUnits = (value: bigint): string => {
    const fixed = formatScaled(value, 6)
    const [integer, fraction = ""] = fixed.split(".")
    const trimmed = fraction.replace(/0+$/, "")
    return trimmed ? `${integer}.${trimmed}` : integer!
}

/** 返回多个非负定点整数中的最小值。 */
const minimumUnits = (...values: bigint[]): bigint =>
    values.reduce((minimum, value) => (value < minimum ? value : minimum))

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
 * 查找一行当前选用的供给选项。
 *
 * @param line 选源明细。
 * @param basisId 当前选用的精确履约依据。
 * @returns 命中的选项；未选或已失效时为空。
 */
export function findSourcingOption(
    line: SourcingProductLine | undefined,
    basisId: string,
): SourcingSupplierOption | undefined {
    if (!line || !basisId) return undefined
    return line.options.find((option) => option.basisId === basisId)
}

/**
 * 为一条销售明细选出最优供给。
 *
 * 排序：能覆盖剩余数量优先，其次最低含税成本、最早交期、最大可创建量，
 * 再按供给方名称稳定排序。
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

/**
 * 为全部选源行填充库存优先的推荐供给和对应可分配量。
 *
 * 没有合格供给的行保持原值，不改勾选状态。
 *
 * @param order 当前选源销售单。
 * @param lines 表单选源行。
 * @returns 填充后的选源行。
 */
export function assignBestSourcingOptions(
    order: SourcingSalesOrder | undefined,
    lines: readonly SourcingLineInput[],
): SourcingLineInput[] {
    if (!order) return lines.map((line) => ({ ...line }))
    const recommended = buildDefaultSourcingLines(order)
    if (recommended.some((line) => line.basisId)) return recommended
    return lines.map((line) => {
        const product = order.lines.find(
            (candidate) => candidate.salesOrderLineId === line.salesOrderLineId,
        )
        const option = pickBestSourcingOption(
            product?.options ?? [],
            product?.remainingQuantity,
        )
        if (!option) return { ...line }
        return {
            ...line,
            selected: true,
            basisId: option.basisId,
            quantity: option.maxCreateQuantity,
            expectedDeliveryDate: option.expectedDeliveryDate,
        }
    })
}

/** 选源销售单的汇总事实，供来源区密集展示。 */
export type SourcingOrderSummary = Readonly<{
    lineCount: number
    coveredLineCount: number
    uniqueSupplierCount: number
    purchaseTypes: readonly PurchaseType[]
    fulfillmentResponsibilities: readonly FulfillmentResponsibility[]
    paymentTermLabels: readonly string[]
    businessCategories: readonly string[]
    minEstimatedGross: string
}>

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
    const recommended = buildDefaultSourcingLines(order)
    const amounts = recommended.flatMap((input) => {
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

/**
 * 勾选明细上出现过的精确履约方案，供批量指定。
 *
 * 不要求所有勾选行都具备同一方案；应用到选中行时，不支持该方案的行会跳过。
 *
 * @param order 当前选源销售单。
 * @param lines 表单选源行。
 * @returns 勾选行可选履约方案的并集；没有勾选时退回全部明细。
 */
export function commonSourcingOptionsForSelected(
    order: SourcingSalesOrder | undefined,
    lines: readonly SourcingLineInput[],
): SourcingSupplierOption[] {
    if (!order) return []
    const selected = lines.filter((line) => line.selected)
    const targets = selected.length > 0 ? selected : lines
    if (targets.length === 0) {
        return uniqueOptions(
            order.lines.flatMap((product) => [...product.options]),
        )
    }
    return uniqueOptions(
        targets.flatMap((line) => {
            const product = order.lines.find(
                (candidate) =>
                    candidate.salesOrderLineId === line.salesOrderLineId,
            )
            return product?.options ?? []
        }),
    )
}

function uniqueOptions(
    options: readonly SourcingSupplierOption[],
): SourcingSupplierOption[] {
    const seen = new Set<string>()
    const result: SourcingSupplierOption[] = []
    for (const option of options) {
        if (!option.basisId || seen.has(option.basisId)) continue
        seen.add(option.basisId)
        result.push(option)
    }
    return result
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

/**
 * 把已选定履约方案的明细按 §7.4 拆分维度预览成多张采购单。
 *
 * @param order 当前选源销售单。
 * @param lines 表单选源行。
 * @returns 按供应商、采购类型、付款条件和履约责任分组的预览单。
 */
export function buildPurchaseOrderPreviews(
    order: SourcingSalesOrder | undefined,
    lines: readonly SourcingLineInput[],
): PurchaseOrderPreview[] {
    if (!order) return []
    const groups = new Map<
        string,
        {
            preview: Omit<PurchaseOrderPreview, "lines" | "totals">
            lines: PurchaseOrderPreviewLine[]
        }
    >()
    for (const input of lines) {
        if (!input.selected || !input.basisId) continue
        const product = order.lines.find(
            (line) => line.salesOrderLineId === input.salesOrderLineId,
        )
        const option = findSourcingOption(product, input.basisId)
        if (!product || !option) continue
        if (option.sourceType === "EXISTING_STOCK") continue
        const amounts = previewLineAmounts(
            option.unitCostGross,
            option.inputTaxRate,
            input.quantity,
        )
        const key = [
            option.supplierId,
            option.purchaseType,
            option.paymentTermCode,
            option.fulfillmentResponsibility,
        ].join("|")
        const previewLine: PurchaseOrderPreviewLine = {
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
        const existing = groups.get(key)
        if (existing) {
            existing.lines.push(previewLine)
            continue
        }
        groups.set(key, {
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
            },
            lines: [previewLine],
        })
    }
    return [...groups.values()].map((group) => ({
        ...group.preview,
        lines: group.lines,
        totals: sumPreviewTotals(group.lines),
    }))
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

/**
 * 校验本次分配数量是否大于 0 且不超过该供给方案最大可分配量。
 *
 * @param quantity 用户输入数量。
 * @param maximum 该供应商最大可创建数量。
 * @returns 合法返回空；否则返回错误文案。
 */
export function sourcingQuantityError(
    quantity: string,
    maximum: string,
): string | undefined {
    try {
        const parsed = parseDecimal(quantity, { maxScale: 6 })
        if (parsed.unscaled <= BigInt(0)) {
            return "本次分配数量必须大于 0"
        }
        if (compareDecimal(quantity, maximum, 6) > 0) {
            return `本次分配数量不能超过 ${maximum}`
        }
        return undefined
    } catch {
        return "本次分配数量必须是大于 0、最多 6 位小数的数值"
    }
}
