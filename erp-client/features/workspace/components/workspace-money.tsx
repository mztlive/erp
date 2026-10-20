import { MoneyValue, type MoneyValueProps } from "@/components/business/values"

/** 兼容工作台历史金额文本；数量、日期、变更前后对照与业务说明保持原文。 */
export function WorkspaceMoney({
    value,
    ...props
}: { value: string } & Omit<MoneyValueProps, "value">) {
    const match =
        /^(?:(增加|减少)\s+)?(-?)[¥￥](-?)(\d{1,3}(?:,\d{3})+|\d+)(\.\d+)?$/.exec(
            value.trim(),
        )
    if (!match || (match[2] && match[3])) return <>{value}</>
    const decimal = `${match[2] || match[3]}${match[4].replaceAll(",", "")}${match[5] ?? ""}`
    return (
        <>
            {match[1] ? `${match[1]} ` : null}
            <MoneyValue value={decimal} {...props} />
        </>
    )
}
