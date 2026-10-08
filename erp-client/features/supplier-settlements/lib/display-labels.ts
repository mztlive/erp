/** 名称和业务单号缺失时使用业务占位，不得显示内部身份。 */
export function settlementDisplayLabel(
    value: string | null | undefined,
    internalId: string | null | undefined,
    placeholder: string,
): string {
    const label = value?.trim()
    return label && label !== internalId?.trim() ? label : placeholder
}

export function settlementActor(
    userId: string | null | undefined,
    displayName: string | null | undefined,
): { userId: string; displayName: string } | undefined {
    if (!userId) return undefined
    return {
        userId,
        displayName: settlementDisplayLabel(displayName, userId, "姓名待补全"),
    }
}
