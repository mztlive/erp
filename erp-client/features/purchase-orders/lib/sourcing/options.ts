import type {
    SourcingLineInput,
    SourcingProductLine,
    SourcingSalesOrder,
    SourcingSupplierOption,
} from "./types"

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
