/**
 * 履约作业面的名称与业务编号分别按各自契约显示。
 */

import { displayBusinessText, displayName } from "@/lib/display-name"

/**
 * 业务单号、编码与数值只排除已知内部身份，保留合法的数字或 UUID 形态编码。
 */
export function displayText(
    value: string | null | undefined,
    ...objectIds: Array<string | null | undefined>
): string {
    const text = displayBusinessText(value, ...objectIds)
    return text && text !== "—" ? text : ""
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
            const name = displayName(line.itemName)
            const quantity = displayText(line.remainingQuantity)
            if (!name || name === "—" || !quantity) return ""
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
    const name = displayName(itemName)
    return name && name !== "—" ? name : `明细 ${index + 1}`
}
