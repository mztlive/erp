/**
 * 去掉单号前的单据种类中文前缀，避免和队列徽章重复。
 *
 * 「销售单 XS20260825170146」→「XS20260825170146」。
 */
export function stripDocumentNumberPrefix(stableNumber: string): string {
    const number = stableNumber.trim()
    const parts = number.split(/\s+/)
    if (parts.length < 2) return number
    const [prefix, ...rest] = parts
    if (/\d/.test(prefix) || !/\d/.test(rest.join(" "))) return number
    return rest.join(" ") || number
}
