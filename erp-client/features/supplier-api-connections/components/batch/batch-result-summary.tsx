import {
    CircleCheckIcon,
    CircleAlertIcon,
    LoaderCircleIcon,
    ArrowRightIcon,
    RefreshCwIcon,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { cn } from "@/lib/utils"
import type { BatchView } from "../../api/batch"

/** 结果反馈以实际回执为准，待核实项始终使用原操作查询。 */
export function BatchResultSummary({
    view,
    processing,
    preparing,
    unsubmittedReady,
    busy,
    canWrite,
    onRefresh,
    onRecovery,
    onReconcile,
    onReturn,
}: {
    view: BatchView
    processing: number
    preparing: boolean
    unsubmittedReady: number
    busy: boolean
    canWrite: boolean
    onRefresh: () => void
    onRecovery: () => void
    onReconcile: () => void
    onReturn: () => void
}) {
    const { counts } = view
    const running =
        processing > 0 || preparing || Boolean(view.batch.execution_pending)
    const actionable = counts.failed + counts.draft + unsubmittedReady
    const unresolved = actionable + counts.unknown
    const complete = !running && unresolved === 0
    const title = running
        ? "正在入库"
        : counts.unknown > 0
          ? "入库结果待核实"
          : unresolved > 0
            ? counts.succeeded + counts.skipped > 0
                ? "入库部分完成"
                : "入库未完成"
            : counts.succeeded > 0
              ? "入库完成"
              : "本批处理完成"
    const Icon = running
        ? LoaderCircleIcon
        : complete
          ? CircleCheckIcon
          : CircleAlertIcon
    const details = [
        { label: "处理中", count: processing },
        { label: "待核实", count: counts.unknown },
        { label: "失败", count: counts.failed },
        { label: "待补齐", count: counts.draft },
        { label: "待提交", count: unsubmittedReady },
        { label: "已完成跳过", count: counts.skipped },
        { label: "已排除", count: counts.excluded },
    ].filter(({ count }) => count > 0)
    return (
        <section aria-label="入库结果摘要" className="border-y py-6 sm:py-8">
            <div className="flex flex-wrap items-center justify-between gap-6">
                <div className="flex min-w-0 items-start gap-4">
                    <div
                        className={cn(
                            "flex size-12 shrink-0 items-center justify-center rounded-full",
                            running
                                ? "bg-info-soft text-info-soft-foreground"
                                : complete
                                  ? counts.succeeded + counts.skipped > 0
                                      ? "bg-success-soft text-success-soft-foreground"
                                      : "bg-muted text-muted-foreground"
                                  : "bg-warning-soft text-warning-soft-foreground",
                        )}
                    >
                        <Icon
                            aria-hidden
                            className={cn(
                                "size-6",
                                running && "motion-safe:animate-spin",
                            )}
                        />
                    </div>
                    <div className="min-w-0 space-y-2" role="status">
                        <h2 className="text-xl font-semibold">{title}</h2>
                        {counts.succeeded > 0 ? (
                            <p className="flex flex-wrap items-baseline gap-2">
                                <span className="num text-2xl font-semibold">
                                    {counts.succeeded}
                                </span>
                                <span className="text-sm text-muted-foreground">
                                    个 SKU 入库成功
                                </span>
                            </p>
                        ) : null}
                        {details.length > 0 ? (
                            <div className="flex flex-wrap gap-2">
                                {details.map(({ label, count }) => (
                                    <Badge
                                        key={label}
                                        variant={
                                            label === "失败"
                                                ? "destructive"
                                                : label === "待核实" ||
                                                    label === "待补齐"
                                                  ? "warning"
                                                  : "secondary"
                                        }
                                    >
                                        {label} {count}
                                    </Badge>
                                ))}
                            </div>
                        ) : null}
                        {running ? (
                            <p className="text-sm text-muted-foreground">
                                后台持续处理，结果自动更新
                            </p>
                        ) : null}
                    </div>
                </div>
                <div className="flex flex-wrap items-center gap-2">
                    <Button
                        id="supplier-catalog-batch-refresh-results"
                        variant="ghost"
                        disabled={busy}
                        onClick={onRefresh}
                    >
                        <RefreshCwIcon aria-hidden />
                        刷新结果
                    </Button>
                    {actionable > 0 && !running && canWrite ? (
                        <Button
                            id="supplier-catalog-batch-open-recovery"
                            variant={counts.unknown > 0 ? "outline" : "default"}
                            disabled={busy || !canWrite}
                            onClick={onRecovery}
                        >
                            处理待办 {actionable}
                            <ArrowRightIcon
                                aria-hidden
                                data-icon="inline-end"
                            />
                        </Button>
                    ) : null}
                    {counts.unknown > 0 && canWrite ? (
                        <Button
                            id="supplier-catalog-batch-result-reconcile"
                            disabled={busy || !canWrite}
                            onClick={onReconcile}
                        >
                            核实入库结果
                        </Button>
                    ) : null}
                    <Button
                        id="supplier-catalog-batch-result-continue"
                        variant={complete ? "default" : "outline"}
                        onClick={onReturn}
                    >
                        继续选品
                        <ArrowRightIcon aria-hidden data-icon="inline-end" />
                    </Button>
                </div>
            </div>
        </section>
    )
}
