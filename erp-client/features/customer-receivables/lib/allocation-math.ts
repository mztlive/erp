/** 分配金额纯函数：解析与格式化（仅展示，不做任何正式余额计算）。 */

export function parseAmt(v: string): number {
    const n = Number(v)
    return Number.isFinite(n) ? n : 0
}

export function money(n: number): string {
    return n.toFixed(2)
}
