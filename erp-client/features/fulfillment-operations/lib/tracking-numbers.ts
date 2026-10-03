/** 物流单号上限，与发货接口保持一致。 */
export const MAX_TRACKING_NUMBERS = 100
export const MAX_TRACKING_NUMBER_LENGTH = 128

/** 将逐行录入或粘贴的物流单号按原顺序去重，忽略空项。 */
export function normalizeTrackingNumbers(
    value: string | readonly string[],
): string[] {
    const entries = typeof value === "string" ? [value] : value
    return [
        ...new Set(
            entries
                .flatMap((entry) => entry.split(/[\r\n,，;；\t]+/))
                .map((entry) => entry.trim())
                .filter(Boolean),
        ),
    ]
}
