import { Badge } from "@/components/ui/badge"
import type { BatchView } from "../../api/batch"

/** 核对摘要区分当前核对与历史结果，零值不占独立位置。 */
export function BatchReviewSummary({
    view,
    readyIds,
    needsRecheck,
}: {
    view: BatchView
    readyIds: string[]
    needsRecheck: boolean
}) {
    const hasPreview = view.batch.preview_version != null
    const readyIdSet = new Set(readyIds)
    const resolutions = [
        { kind: "CREATE_PRODUCT", label: "新建 SKU" },
        { kind: "CREATE_SKU", label: "新增规格" },
        { kind: "EXISTING", label: "关联已有 SKU" },
    ].map(({ kind, label }) => ({
        label,
        count: view.rows.filter(
            (row) =>
                readyIdSet.has(row.source.id) && row.resolution?.kind === kind,
        ).length,
    }))
    return (
        <section aria-label="核对摘要" className="space-y-2 border-b pb-3">
            {needsRecheck ? (
                <div
                    role="status"
                    className="flex flex-wrap items-center gap-2"
                >
                    <span className="text-body-compact font-semibold text-warning-soft-foreground">
                        待重新核对
                    </span>
                    <span className="text-body-compact text-muted-foreground">
                        {view.batch.preview_version == null
                            ? "核对后可确认入库"
                            : "草稿已恢复或调整，以下为上次核对结果"}
                    </span>
                </div>
            ) : null}
            <div className="flex flex-wrap items-center gap-x-5 gap-y-2 text-body-compact">
                {hasPreview ? (
                    <p
                        className={
                            needsRecheck
                                ? "text-muted-foreground"
                                : "font-semibold"
                        }
                    >
                        {needsRecheck ? "上次可入库" : "可入库"}{" "}
                        <span
                            className={
                                needsRecheck
                                    ? "num"
                                    : "num text-value-compact font-semibold tracking-tight"
                            }
                        >
                            {readyIds.length}
                        </span>
                    </p>
                ) : null}
                {view.counts.unknown > 0 ? (
                    <Badge variant="warning">
                        结果待核实 {view.counts.unknown}
                    </Badge>
                ) : null}
                {view.counts.draft > 0 ? (
                    <Badge variant="warning">待补齐 {view.counts.draft}</Badge>
                ) : null}
                {view.counts.failed > 0 ? (
                    <Badge variant="destructive">
                        明确失败 {view.counts.failed}
                    </Badge>
                ) : null}
                {view.counts.succeeded + view.counts.skipped > 0 ? (
                    <span className="text-muted-foreground">
                        已完成跳过 {view.counts.succeeded + view.counts.skipped}
                    </span>
                ) : null}
                {resolutions
                    .filter(({ count }) => hasPreview && count > 0)
                    .map(({ label, count }) => (
                        <span key={label} className="text-muted-foreground">
                            {label} {count}
                        </span>
                    ))}
            </div>
        </section>
    )
}
