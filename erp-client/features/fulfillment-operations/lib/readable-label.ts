/**
 * 履约作业面的可读文案。内部主键、UUID 形态单号不得上屏。
 */

const OPAQUE_ID =
    /^(?:[0-9a-f]{24,}|[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$/i

const PREFIXED_OPAQUE_ID =
    /^(?:DLV|FH|GRN|SF|DN|PR|ED|PO|SO|CG)-[0-9a-f]{24,}$/i

/**
 * 值是否只是内部对象 id 或把 id 拼进前缀的伪单号。
 */
export function isOpaqueId(value: string): boolean {
    const trimmed = value.trim()
    return OPAQUE_ID.test(trimmed) || PREFIXED_OPAQUE_ID.test(trimmed)
}

/**
 * 用户可读的展示文案。空值和内部 id 一律丢掉。
 */
export function displayText(value: string | null | undefined): string {
    const trimmed = value?.trim() ?? ""
    if (!trimmed || trimmed === "—" || isOpaqueId(trimmed)) return ""
    return trimmed
}

type RemainingLine = Readonly<{
    itemName: string
    remainingQuantity: string
    unitCode: string
}>

/**
 * 把待处理数量收成「品名 数量单位」。没有品名时不上屏，避免只剩一串数字。
 */
export function formatRemainingLines(lines: readonly RemainingLine[]): string {
    return lines
        .map((line) => {
            const name = displayText(line.itemName)
            const quantity = displayText(line.remainingQuantity)
            if (!name || !quantity) return ""
            return `${name} ${quantity}${displayText(line.unitCode)}`
        })
        .filter(Boolean)
        .join("；")
}

/**
 * 行上的品名。缺失时用「明细 n」，不得回退成行 id。
 */
export function lineItemTitle(
    itemName: string | undefined,
    index: number,
): string {
    return displayText(itemName) || `明细 ${index + 1}`
}
