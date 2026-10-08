/** 名称字段不能使用内部身份；请求参数、业务编号和 DOM id 保留原值。 */
const INTERNAL_ID =
    /^(?:[0-9a-f]{24,}|[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}|(?:rsv|pla|sv|wi)_.+)$/i

/** 只接受可读名称，并拒绝与关联对象身份相同的旧回退值。 */
export function displayName(
    value?: string | null,
    ...objectIds: Array<string | null | undefined>
): string | undefined {
    const text = value?.trim()
    if (
        !text ||
        INTERNAL_ID.test(text) ||
        objectIds.some((id) => text === id?.trim())
    ) {
        return undefined
    }
    return text
}

/** 业务单号和原始文本按已知身份排除回退值，不按内容格式猜测内部 ID。 */
export function displayBusinessText(
    value?: string | null,
    ...objectIds: Array<string | null | undefined>
): string | undefined {
    const text = value?.trim()
    return text && !objectIds.some((id) => text === id?.trim())
        ? text
        : undefined
}
