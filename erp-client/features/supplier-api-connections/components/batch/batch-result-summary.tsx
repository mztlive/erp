import {
    CircleCheckIcon,
    CircleAlertIcon,
    LoaderCircleIcon,
    ArrowRightIcon,
    RefreshCwIcon,
} from "lucide-react"
import { BusinessStatusBadge } from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { cn } from "@/lib/utils"
import type { BatchView } from "../../api/batch"

/** 结果反馈以实际回执为准，待核实项始终使用原操作查询。 */
export function BatchResultSummary({
    view,
    processing,
    preparing,
    unsubmittedReady,
    busy,
    refreshing = false,
    recovering = false,
    reconciling = false,
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
    refreshing?: boolean
    recovering?: boolean
    reconciling?: boolean
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
        { label: "处理中", count: processing, tone: "info" as const },
        { label: "待核实", count: counts.unknown, tone: "warning" as const },
        { label: "失败", count: counts.failed, tone: "destructive" as const },
        { label: "待补齐", count: counts.draft, tone: "warning" as const },
        { label: "待提交", count: unsubmittedReady, tone: "neutral" as const },
        {
            label: "已完成跳过",
            count: counts.skipped,
            tone: "neutral" as const,
        },
        { label: "已排除", count: counts.excluded, tone: "neutral" as const },
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
                        <h2 className="text-section-title font-semibold tracking-tight">
                            {title}
                        </h2>
                        {counts.succeeded > 0 ? (
                            <p className="flex flex-wrap items-baseline gap-2">
                                <span className="num text-value-section font-semibold tracking-tight">
                                    {counts.succeeded}
                                </span>
                                <span className="text-body-compact text-muted-foreground">
                                    个 SKU 入库成功
                                </span>
                            </p>
                        ) : null}
                        {details.length > 0 ? (
                            <div className="flex flex-wrap gap-2">
                                {details.map(({ label, count, tone }) => (
                                    <BusinessStatusBadge
                                        key={label}
                                        className="num"
                                        tone={tone}
                                        label={`${label} ${count}`}
                                    />
                                ))}
                            </div>
                        ) : null}
                        {running ? (
                            <p className="text-xs text-muted-foreground">
                                后台持续处理，结果自动更新
                            </p>
                        ) : null}
                    </div>
                </div>
                <div className="flex flex-wrap items-center gap-2">
                    <LoadingButton
                        id="supplier-catalog-batch-refresh-results"
                        variant="ghost"
                        loading={refreshing}
                        disabled={busy}
                        onClick={onRefresh}
                    >
                        <RefreshCwIcon aria-hidden />
                        刷新结果
                    </LoadingButton>
                    {actionable > 0 && !running && canWrite ? (
                        <LoadingButton
                            id="supplier-catalog-batch-open-recovery"
                            variant={counts.unknown > 0 ? "outline" : "default"}
                            loading={recovering}
                            disabled={busy || !canWrite}
                            onClick={onRecovery}
                        >
                            处理待办 <span className="num">{actionable}</span>
                            <ArrowRightIcon
                                aria-hidden
                                data-icon="inline-end"
                            />
                        </LoadingButton>
                    ) : null}
                    {counts.unknown > 0 && canWrite ? (
                        <LoadingButton
                            id="supplier-catalog-batch-result-reconcile"
                            loading={reconciling}
                            disabled={busy || !canWrite}
                            onClick={onReconcile}
                        >
                            核实入库结果
                        </LoadingButton>
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
