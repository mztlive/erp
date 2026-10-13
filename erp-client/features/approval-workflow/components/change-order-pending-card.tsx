"use client"

import type { ReactNode } from "react"
import { ChevronDownIcon } from "lucide-react"

import { BusinessFailureState } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { StatusBadge } from "@/components/ui/status-badge"
import { ExecutionHistory } from "@/features/approval-workflow/components/execution-history"
import {
    displayActorName,
    displayReadableName,
    displayRound,
    isBlockedStatus,
} from "@/features/approval-workflow/display"
import { useApprovalHistoryInfiniteQuery } from "@/features/approval-workflow/queries"
import type { StatusTone } from "@/components/ui/status-badge"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import { formatDateTime } from "@/lib/datetime"

function PendingFact({ label, value }: { label: string; value: string }) {
    return (
        <div className="min-w-0">
            <dt className="text-xs text-muted-foreground">{label}</dt>
            <dd className="mt-0.5 truncate text-sm font-medium">{value}</dd>
        </div>
    )
}

/** 进行中改单卡片需要的摘要；销售与采购改单共用。 */
export type ChangeOrderPendingSummary = {
    statusLabel: string
    statusTone: StatusTone
    /** 发起时的生效版本号；未知时为 0。 */
    baseRevisionNo: number
    createdAt: string
    approval?: DocumentApprovalView
}

/**
 * 详情查阅里的进行中改单：基于哪个版本、现在在谁手上、能做什么放在一张卡里。
 *
 * 审批记录默认收起；当前节点只在摘要里出现一次，不再和记录重复铺开。
 */
export function ChangeOrderPendingCard({
    id,
    changeOrder,
    counterparty,
    running,
    actions,
    children,
}: {
    id: string
    changeOrder: ChangeOrderPendingSummary
    /** 生效前仍按原版本执行的一方，例如「客户」「供应商」。 */
    counterparty: string
    /** 审批已提交且在途；草稿或已撤回时不展示当前节点。 */
    running: boolean
    actions?: ReactNode
    /** 卡片正文附加内容，例如原因或对其他单据的影响。 */
    children?: ReactNode
}) {
    const approval = changeOrder.approval
    const instance = approval?.instance
    const history = useApprovalHistoryInfiniteQuery(
        { instanceId: instance?.id ?? "" },
        Boolean(instance?.id),
    )
    const historyItems =
        history.data?.pages.flatMap((page) => page.items) ??
        approval?.recentHistory ??
        []
    const base =
        changeOrder.baseRevisionNo > 0 ? `v${changeOrder.baseRevisionNo}` : ""
    const createdAt = formatDateTime(changeOrder.createdAt, "monthDay")
    const blocked =
        running && instance ? isBlockedStatus(instance.status) : false
    const rejectionBy = instance
        ? displayActorName(
              instance.latestRejectionByName,
              instance.latestRejectionBy,
          )
        : undefined

    return (
        <section
            aria-labelledby={`${id}-title`}
            className="space-y-4 rounded-xl border border-warning-border bg-warning-soft/30 p-4 sm:p-5"
        >
            <header className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0 space-y-1">
                    <div className="flex flex-wrap items-center gap-2">
                        <h2
                            id={`${id}-title`}
                            className="text-sm font-semibold"
                        >
                            进行中的改单
                        </h2>
                        <StatusBadge
                            tone={changeOrder.statusTone}
                            label={changeOrder.statusLabel}
                        />
                    </div>
                    <p className="text-xs leading-5 text-muted-foreground">
                        {base ? `基于 ${base} · ` : ""}
                        {createdAt} 发起。生效前{counterparty}仍按
                        {base ? ` ${base} ` : "当前版本"}执行。
                    </p>
                </div>
                {actions ? (
                    <div className="flex flex-wrap items-center gap-2">
                        {actions}
                    </div>
                ) : null}
            </header>

            {children}

            {running && instance ? (
                <dl className="grid grid-cols-1 gap-3 rounded-lg bg-background/70 px-3 py-2.5 sm:grid-cols-3">
                    <PendingFact
                        label="当前节点"
                        value={
                            displayReadableName(
                                instance.currentNodeName,
                                instance.currentNode,
                            ) ?? "审批节点未标注"
                        }
                    />
                    <PendingFact
                        label="当前审批人"
                        value={
                            displayActorName(
                                instance.currentAssigneeName,
                                instance.currentAssignee,
                            ) ?? "审批人待确认"
                        }
                    />
                    <PendingFact
                        label="轮次"
                        value={displayRound(instance.currentRoundNo)}
                    />
                </dl>
            ) : null}

            {instance?.latestRejection ? (
                <Alert variant="destructive">
                    <AlertTitle>
                        最近驳回{rejectionBy ? ` · ${rejectionBy}` : ""}
                    </AlertTitle>
                    <AlertDescription>
                        {instance.latestRejection}
                    </AlertDescription>
                </Alert>
            ) : null}
            {blocked ? (
                <Alert variant="destructive">
                    <AlertTitle>审批受阻</AlertTitle>
                    <AlertDescription>
                        {instance?.blockerMessage ??
                            "当前审批无法继续，请按系统给出的恢复方式处理。"}
                    </AlertDescription>
                </Alert>
            ) : null}

            {history.isError ? (
                <BusinessFailureState
                    id={`${id}-retry`}
                    title="审批记录加载失败"
                    error={history.error}
                    onRetry={() => void history.refetch()}
                />
            ) : null}
            {historyItems.length > 0 ? (
                <Collapsible className="border-t border-warning-border/60 pt-3">
                    <CollapsibleTrigger
                        id={`${id}-history-toggle`}
                        className="group/history -ml-2"
                        render={
                            <Button type="button" variant="ghost" size="sm" />
                        }
                    >
                        审批记录
                        <span className="num text-muted-foreground">
                            {historyItems.length}
                        </span>
                        <ChevronDownIcon
                            data-icon="inline-end"
                            className="transition-transform group-data-[panel-open]/history:rotate-180"
                        />
                    </CollapsibleTrigger>
                    <CollapsibleContent className="pt-3">
                        <ExecutionHistory
                            id={`${id}-history`}
                            presentation="timeline"
                            items={historyItems}
                            hasMore={history.hasNextPage}
                            loadingMore={history.isFetchingNextPage}
                            onLoadMore={
                                history.hasNextPage
                                    ? () => {
                                          void history.fetchNextPage()
                                      }
                                    : undefined
                            }
                        />
                    </CollapsibleContent>
                </Collapsible>
            ) : null}
        </section>
    )
}
