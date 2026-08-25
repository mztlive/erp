import type { SellableSkuPick } from "@/features/entity-selectors/lib/sellable-sku-pick"
import { createEmptyLine } from "@/features/sales-orders/lib/sales-order-create-model"
import type {
    SalesOrderDraftLineInput,
    SalesOrderNature,
} from "@/features/sales-orders/types"

function lineFromPick(
    pick: SellableSkuPick,
    nature: SalesOrderNature,
    existing?: SalesOrderDraftLineInput,
): SalesOrderDraftLineInput {
    const base = existing ?? createEmptyLine(nature)
    return {
        ...base,
        name: pick.name,
        sku: pick.skuId,
        skuRevisionId: pick.skuRevisionId,
        unit: pick.baseUnit || base.unit,
        unitPriceGross: pick.salesVisiblePriceGross || base.unitPriceGross,
    }
}

function isEmptyLine(line: SalesOrderDraftLineInput): boolean {
    return line.sku.trim() === "" && line.name.trim() === ""
}

/** 用选中的 SKU 替换指定明细行；保留数量等已填字段。 */
export function replaceLineWithSellablePick(
    lines: readonly SalesOrderDraftLineInput[],
    rowIndex: number,
    pick: SellableSkuPick,
    nature: SalesOrderNature,
): SalesOrderDraftLineInput[] {
    return lines.map((line, index) =>
        index === rowIndex ? lineFromPick(pick, nature, line) : line,
    )
}

/**
 * 把多选 SKU 写入明细：先填空白行，再追加新行。
 * 空白行指尚未选择 SKU 且尚未填写名称的占位行。
 */
export function appendSellablePicksToLines(
    lines: readonly SalesOrderDraftLineInput[],
    picks: readonly SellableSkuPick[],
    nature: SalesOrderNature,
): SalesOrderDraftLineInput[] {
    if (picks.length === 0) return [...lines]
    const next = [...lines]
    const unused = [...picks]
    for (let index = 0; index < next.length && unused.length > 0; index += 1) {
        if (!isEmptyLine(next[index]!)) continue
        next[index] = lineFromPick(unused.shift()!, nature, next[index])
    }
    for (const pick of unused) {
        next.push(lineFromPick(pick, nature))
    }
    return next
}

/**
 * 把选中 SKU 写入明细。
 * 指定 `replaceRowIndex` 时：第一项替换该行，其余追加；否则先填空白行再追加。
 */
export function applySellablePicksToLines(
    lines: readonly SalesOrderDraftLineInput[],
    picks: readonly SellableSkuPick[],
    nature: SalesOrderNature,
    replaceRowIndex?: number,
): SalesOrderDraftLineInput[] {
    if (picks.length === 0) return [...lines]
    if (replaceRowIndex == null) {
        return appendSellablePicksToLines(lines, picks, nature)
    }
    const [first, ...rest] = picks
    if (!first) return [...lines]
    const replaced = replaceLineWithSellablePick(
        lines,
        replaceRowIndex,
        first,
        nature,
    )
    return rest.length > 0
        ? appendSellablePicksToLines(replaced, rest, nature)
        : replaced
}
