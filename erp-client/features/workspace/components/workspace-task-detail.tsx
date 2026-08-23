"use client"

import { ChevronRightIcon } from "lucide-react"

import { ApprovalActionBar } from "@/features/approval-workflow/components/approval-action-bar"
import { RuntimeSummary } from "@/features/approval-workflow/components/runtime-summary"
import { useRecoveryOptionsQuery } from "@/features/approval-workflow/queries"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { StatusBadge } from "@/components/ui/status-badge"
import { cn } from "@/lib/utils"

import { useWorkspaceDocumentFacts } from "../hooks/use-workspace-document-facts"
import { buildDocumentHref } from "../lib/destination"
import { type DetailSection, splitDetailSections } from "../lib/detail-facts"
import { isApprovalWorkbenchTask } from "../lib/navigation-eligibility"
import { isBlockedWorkItem } from "../lib/work-item"
import type { WorkspaceWorkItem } from "../types"

/**
 * 工作台右侧详情。审批任务在页内提交决定；非审批任务只给打开单据。
 *
 * 版式分三层：金额条是决策重心，明细回答「买什么」，其余字段折叠待查。
 * 标题副行已展示的往来方与提交人不再进字段区，避免同一事实反复上屏。
 */
export function WorkspaceTaskDetail({
    item,
    canReadSensitive = true,
    onDecisionApplied,
}: {
    item: WorkspaceWorkItem
    canReadSensitive?: boolean
    onDecisionApplied?: (
        view: ApprovalCommandView,
        completedWorkItemId: string,
    ) => void
}) {
    const approvalTask = isApprovalWorkbenchTask(
        item.allowedActions,
        item.approvalProcessInstanceId,
        item.approvalNodeExecutionId,
    )
    const instanceId =
        item.approvalProcessInstanceId ?? item.approval?.instanceId
    const recoveryQuery = useRecoveryOptionsQuery(
        instanceId,
        approvalTask && isBlockedWorkItem(item),
    )
    const documentHref = buildDocumentHref(item)
    const documentFacts = useWorkspaceDocumentFacts(item)
    const facts = documentFacts.facts
    const summarySections = facts?.sections ?? item.summarySections
    const briefLines = facts?.lines ?? item.briefLines
    const briefMoreCount = facts?.moreCount ?? item.briefMoreCount
    const counterpartyName = facts?.counterparty ?? item.counterpartyName
    const impactSummary =
        facts?.impact &&
        (item.impactSummary === "不审批则卡券销售不能生效" ||
            !item.summarySections?.length)
            ? facts.impact
            : item.impactSummary
    const detailFacts = splitDetailSections(summarySections, counterpartyName)
    const [primaryAmount, ...otherAmounts] = detailFacts.amounts
    const blocked = isBlockedWorkItem(item)
    const overdue = item.dueBucket === "overdue"
    const subtitle = [
        counterpartyName,
        detailFacts.submitter ? `${detailFacts.submitter} 提交` : undefined,
        item.enteredAtLabel,
    ]
        .filter(Boolean)
        .join(" · ")
    const instance = item.approval
        ? {
              id: item.approval.instanceId,
              status: item.approval.status,
              currentRoundNo: item.approval.currentRoundNo,
              currentNodeName: item.approval.currentNodeLabel,
              currentAssigneeName: item.approval.currentAssigneeLabel,
              latestRejection: item.approval.lastRejectReason,
              latestRejectionBy: item.approval.lastRejectorLabel,
              processName: item.approval.processName,
              processVersion: item.approval.processVersion,
          }
        : undefined
    const actions = approvalTask ? (
        <ApprovalActionBar
            allowedActions={item.allowedActions}
            recoveryOptions={recoveryQuery.data?.actions ?? []}
            workItemId={item.workItemId}
            expectedTaskVersion={item.taskVersion}
            instance={instance}
            documentHref={documentHref}
            canReadSensitive={canReadSensitive}
            approveWithoutDialog
            onDecisionApplied={(view) =>
                onDecisionApplied?.(view, item.workItemId)
            }
        />
    ) : documentHref ? (
        <Button
            type="button"
            render={<a href={documentHref} aria-label="打开单据" />}
        >
            打开单据
        </Button>
    ) : null

    return (
        <section className="flex h-full min-h-0 flex-col" aria-label="当前任务">
            <div className="min-h-0 flex-1 overflow-auto p-4">
                {/* 正文限宽：铺满宽屏会让标签与取值拉开到需要横扫的距离。 */}
                <div className="flex max-w-3xl flex-col gap-5">
                    <header className="flex flex-col gap-1">
                        <div className="flex flex-wrap items-center gap-2">
                            <h2 className="text-lg font-medium">
                                {item.objectTitle}
                            </h2>
                            {blocked ? (
                                <StatusBadge label="受阻" tone="warning" />
                            ) : overdue ? (
                                <StatusBadge
                                    label="已超期"
                                    tone="destructive"
                                />
                            ) : null}
                        </div>
                        <p className="text-sm text-muted-foreground">
                            {canReadSensitive
                                ? subtitle
                                : "当前账号无权查看部分业务字段"}
                        </p>
                    </header>

                    {primaryAmount ? (
                        <div className="flex flex-wrap items-baseline gap-x-6 gap-y-1 border-y border-border/40 py-3">
                            <div>
                                <span className="num text-2xl font-semibold">
                                    {primaryAmount.value}
                                </span>
                                <span className="ml-2 text-xs text-muted-foreground">
                                    {primaryAmount.label}
                                </span>
                            </div>
                            {otherAmounts.map((amount) => (
                                <div
                                    key={amount.label}
                                    className="text-sm text-muted-foreground"
                                >
                                    <span className="num text-foreground">
                                        {amount.value}
                                    </span>
                                    <span className="ml-1.5 text-xs">
                                        {amount.label}
                                    </span>
                                </div>
                            ))}
                        </div>
                    ) : documentFacts.isPending ? (
                        <p className="text-sm text-muted-foreground">
                            正在读取单据事实…
                        </p>
                    ) : impactSummary ? (
                        <p className="text-sm">{impactSummary}</p>
                    ) : null}

                    {detailFacts.keyFields.length > 0 ? (
                        <FieldGrid sections={detailFacts.keyFields} />
                    ) : null}

                    {briefLines && briefLines.length > 0 ? (
                        <section className="flex flex-col gap-2">
                            <h3 className="text-xs font-medium text-muted-foreground">
                                明细 ·{" "}
                                {briefLines.length + (briefMoreCount ?? 0)} 行
                            </h3>
                            <ul className="flex flex-col text-sm">
                                {briefLines.map((line) => (
                                    <li
                                        key={line.title}
                                        className="flex justify-between gap-4 border-b border-border/30 py-1.5 last:border-b-0"
                                    >
                                        <span className="min-w-0 truncate">
                                            {line.title}
                                        </span>
                                        {line.quantity ? (
                                            <span className="num shrink-0 text-muted-foreground">
                                                {line.quantity}
                                            </span>
                                        ) : null}
                                    </li>
                                ))}
                            </ul>
                            {briefMoreCount ? (
                                <p className="text-xs text-muted-foreground">
                                    另有 {briefMoreCount} 行，打开单据查看
                                </p>
                            ) : null}
                        </section>
                    ) : null}

                    {detailFacts.moreFields.length > 0 ? (
                        <Collapsible>
                            <CollapsibleTrigger className="group flex items-center gap-1 text-xs text-muted-foreground transition-colors hover:text-foreground">
                                <ChevronRightIcon
                                    aria-hidden="true"
                                    className="size-3.5 transition-transform group-aria-expanded:rotate-90"
                                />
                                单据字段（{detailFacts.moreFields.length}）
                            </CollapsibleTrigger>
                            <CollapsibleContent>
                                <FieldGrid
                                    className="pt-3"
                                    sections={detailFacts.moreFields}
                                />
                            </CollapsibleContent>
                        </Collapsible>
                    ) : null}

                    {item.actionBlockers.length > 0 ? (
                        <p className="text-sm text-muted-foreground">
                            {item.actionBlockers[0]?.message}
                        </p>
                    ) : null}

                    {/* 没有运行实例时不渲染空的「审批摘要」占位卡。 */}
                    {approvalTask ? (
                        instance ? (
                            <RuntimeSummary instance={instance} />
                        ) : null
                    ) : item.impactSummary ? (
                        <p className="text-sm">{item.impactSummary}</p>
                    ) : null}
                </div>
            </div>
            {actions ? (
                <div className="shrink-0 border-t border-border/30 bg-card px-4 py-3">
                    {actions}
                </div>
            ) : null}
        </section>
    )
}

function FieldGrid({
    sections,
    className,
}: {
    sections: readonly DetailSection[]
    className?: string
}) {
    return (
        <dl
            className={cn(
                "grid grid-cols-2 gap-x-6 gap-y-3 text-sm sm:grid-cols-3",
                className,
            )}
        >
            {sections.map((section) => (
                <div key={section.label} className="min-w-0">
                    <dt className="text-xs text-muted-foreground">
                        {section.label}
                    </dt>
                    <dd
                        className={cn("truncate", section.numeric && "num")}
                        title={section.value}
                    >
                        {section.value}
                    </dd>
                </div>
            ))}
        </dl>
    )
}
