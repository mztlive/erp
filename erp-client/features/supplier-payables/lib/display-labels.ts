/** 供应商名称缺失时的用户可见占位，不得回退内部供应商 ID。 */
export const MISSING_SUPPLIER_NAME = "供应商名称待补全"

/** 采购单号缺失时的用户可见占位，不得回退内部采购单 ID。 */
export const MISSING_PURCHASE_ORDER_NO = "采购单号待补全"

/** 结算单号缺失时的用户可见占位，不得回退内部结算单 ID。 */
export const MISSING_SETTLEMENT_NO = "结算单号待补全"

/** 来源单号缺失时的用户可见占位。 */
export const MISSING_SOURCE_DOCUMENT_NO = "来源单号待补全"

/**
 * 返回可上屏的业务标签；空值或与内部 ID 相同的值统一改为业务占位。
 *
 * @param value 后端或上游提供的业务名称/单号。
 * @param internalId 仅用于判定的内部身份，不得作为返回值。
 * @param placeholder 缺失时的用户可见业务文案。
 */
export function businessLabelOrPlaceholder(
    value: string | null | undefined,
    internalId: string | null | undefined,
    placeholder: string,
): string {
    const label = value?.trim()
    const id = internalId?.trim()
    if (!label || (id && label === id)) return placeholder
    return label
}
