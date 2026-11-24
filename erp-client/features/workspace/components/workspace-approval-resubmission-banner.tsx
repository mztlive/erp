"use client"

import { ArrowRightIcon, HistoryIcon } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { displayActorName } from "@/features/approval-workflow/display"
import { cn } from "@/lib/utils"

export type ResubmissionFieldDiff = Readonly<{
    label: string
    before: string
    after: string
    numeric?: boolean
}>

export type WorkspaceApprovalResubmissionBannerProps = Readonly<{
    roundNo?: number
    resubmissionReason?: string | null
    diffBaseline?: string | null
    diffSummary?: string | null
    diffs?: readonly ResubmissionFieldDiff[]
    previousRejection?: Readonly<{
        reason: string
        actor?: string | null
    }> | null
    className?: string
}>

/**
 * 第 2 轮及以上的重提核对条：
 * 将「重新提交说明」、上轮驳回原因与表头「前→后」变更摘要集中置顶展示，
 * 避免混入静态基础信息网格，帮助审批人快速核对“上次为何驳回 → 本次改了什么”。
 */
export function WorkspaceApprovalResubmissionBanner({
    roundNo,
    resubmissionReason,
    diffBaseline,
    diffSummary,
    diffs = [],
    previousRejection,
    className,
}: WorkspaceApprovalResubmissionBannerProps) {
    const hasReason = Boolean(resubmissionReason?.trim())
    const hasDiffs = diffs.length > 0 || Boolean(diffSummary?.trim())
    const hasPreviousRejection = Boolean(previousRejection?.reason.trim())

    if (!hasReason && !hasDiffs && !hasPreviousRejection) return null

    const previousActor = displayActorName(previousRejection?.actor)
    const roundBadgeLabel =
        roundNo && roundNo > 1 ? `第 ${roundNo} 轮重提` : "再次提交"

    return (
        <section
            aria-label="再次提交与变更说明"
            data-slot="workspace-approval-resubmission-banner"
            className={cn(
                "space-y-2 rounded-md border border-info-border/80 bg-info-soft/35 px-3.5 py-2.5 text-xs",
                className,
            )}
        >
            <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
                <span className="inline-flex items-center gap-1.5 font-medium text-info-soft-foreground">
                    <HistoryIcon
                        aria-hidden="true"
                        className="size-3.5 shrink-0"
                    />
                    <Badge variant="info" className="num">
                        {roundBadgeLabel}
                    </Badge>
                </span>
                {diffBaseline ? (
                    <span className="num text-muted-foreground">
                        对比 {diffBaseline}
                    </span>
                ) : null}
                {diffSummary ? (
                    <span className="font-medium text-foreground">
                        {diffSummary}
                    </span>
                ) : null}
            </div>

            {hasReason ? (
                <p className="text-body-compact whitespace-pre-wrap wrap-anywhere text-foreground">
                    <span className="mr-1.5 text-xs font-medium text-muted-foreground">
                        重提说明：
                    </span>
                    {resubmissionReason?.trim()}
                </p>
            ) : null}

            {diffs.length > 0 ? (
                <div className="flex flex-wrap items-center gap-1.5 pt-0.5">
                    {diffs.map((item) => (
                        <span
                            key={item.label}
                            className="inline-flex max-w-full flex-wrap items-center gap-1 rounded-sm border border-border/70 bg-card px-2 py-0.5 text-xs"
                        >
                            <span className="text-muted-foreground">
                                {item.label}
                            </span>
                            <span
                                className={cn(
                                    "text-muted-foreground line-through",
                                    item.numeric && "num",
                                )}
                            >
                                {item.before}
                            </span>
                            <ArrowRightIcon
                                aria-hidden="true"
                                className="size-3 shrink-0 text-muted-foreground"
                            />
                            <span
                                className={cn(
                                    "font-medium text-foreground",
                                    item.numeric && "num",
                                )}
                            >
                                {item.after}
                            </span>
                        </span>
                    ))}
                </div>
            ) : null}

            {hasPreviousRejection && previousRejection ? (
                <p className="border-t border-info-border/50 pt-1.5 whitespace-pre-wrap wrap-anywhere text-muted-foreground">
                    <span className="font-medium text-destructive-soft-foreground">
                        上轮驳回
                        {previousActor ? `（${previousActor}）` : ""}：
                    </span>
                    {previousRejection.reason.trim()}
                </p>
            ) : null}
        </section>
    )
}
