import * as React from "react"

/**
 * 把 Base UI Combobox 的 onInputValueChange 转成远程搜索词。
 *
 * 选中或关闭时 Base UI 会把输入框写成 item 展示文案（reason 为 `item-press` / `none`）。
 * 不能把这次回写当查询词，否则会用「全称（简称）」去打接口：列表对不上已选项 →
 * 受控 value 被算成 null → 输入框清空再回填，循环闪烁。
 *
 * 返回 `undefined` 表示这次变化不是搜索，调用方不应更新查询。
 */
export function remoteSearchFromInputChange(
    query: string,
    reason: string,
): string | undefined {
    if (reason === "item-press") return ""
    if (
        reason === "input-change" ||
        reason === "input-clear" ||
        reason === "clear-press"
    ) {
        return query
    }
    return undefined
}

/** 列表暂时不含已选项时，沿用上一次匹配到的对象，避免受控 value 掉成 null。 */
export function useStickySelected<T>(
    items: readonly T[],
    key: string | undefined,
    keyOf: (item: T) => string,
): T | null {
    const lastRef = React.useRef<T | null>(null)
    if (!key) {
        lastRef.current = null
        return null
    }
    const fromList = items.find((item) => keyOf(item) === key) ?? null
    if (fromList) lastRef.current = fromList
    return (
        fromList ??
        (lastRef.current && keyOf(lastRef.current) === key
            ? lastRef.current
            : null)
    )
}
