/**
 * W22 商品发布 · 列表页指标快捷筛选参数解析（纯函数）。
 */

/** 仅接受已登记的 metric 值，其余回落到 "all"。 */
export function parseMetric(raw: string | null): string {
    if (
        raw === "pending_confirm" ||
        raw === "failed_handoff" ||
        raw === "mall_live" ||
        raw === "paused" ||
        raw === "pending_publish"
    ) {
        return raw
    }
    return "all"
}
