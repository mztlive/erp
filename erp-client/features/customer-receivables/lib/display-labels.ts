/** 往来主体名称缺失时的用户可见占位，不得回退内部主体 ID。 */
export const MISSING_COUNTERPARTY_NAME = "往来主体名称待补全"

/** 经营客户名称缺失时的用户可见占位，不得回退内部客户 ID。 */
export const MISSING_CUSTOMER_NAME = "经营客户名称待补全"

/** 销售单号缺失时的用户可见占位，不得回退内部销售单 ID。 */
export const MISSING_SALES_ORDER_NO = "销售单号待补全"

/** 应收项目类型使用业务中文，未知类型保持通用名称。 */
export const receivableEntryLabel = (entryType: string): string => {
    switch (entryType) {
        case "original":
            return "原始应收"
        case "sales_change_delta":
            return "销售变更差额"
        case "void_reduction":
            return "作废冲减"
        case "refund":
            return "退款"
        case "reversal":
            return "冲正"
        default:
            return "应收项目"
    }
}

/**
 * 返回可上屏的业务标签；空值或与内部 ID 相同的值统一改为业务占位。
 *
 * @param value 后端或上游提供的业务名称/单号。
 * @param internalId 仅用于判定的内部身份，不得作为返回值。
 * @param placeholder 缺失时的用户可见业务文案。
 */
export const businessLabelOrPlaceholder = (
    value: string | null | undefined,
    internalId: string | null | undefined,
    placeholder: string,
): string => {
    const label = value?.trim()
    const id = internalId?.trim()
    if (!label || (id && label === id)) return placeholder
    return label
}
