"use client"

import { useEffect, useState, type ReactNode } from "react"
import { ArrowUpRightIcon, FileTextIcon } from "lucide-react"

import { ApprovalActionBar } from "@/features/approval-workflow/components/approval-action-bar"
import { RuntimeSummary } from "@/features/approval-workflow/components/runtime-summary"
import { useRecoveryOptionsQuery } from "@/features/approval-workflow/queries"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { surfaceInsetClassName } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button, buttonVariants } from "@/components/ui/button"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { cn } from "@/lib/utils"

import {
    workspaceOpenActionLabel,
    workspaceReadActionLabel,
} from "../api/work-item-meta"
import { useWorkspaceDocumentFacts } from "../hooks/use-workspace-document-facts"
import { buildDocumentHref } from "../lib/destination"
import { type DetailSection, splitDetailSections } from "../lib/detail-facts"
import { isApprovalWorkbenchTask } from "../lib/navigation-eligibility"
import { workspacePaperKind } from "../lib/paper-kind"
import { isBlockedWorkItem } from "../lib/work-item"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceDocumentBadge } from "./workspace-document-badge"
import { WorkspaceDocumentPaperDialog } from "./workspace-document-paper-dialog"

/**
 * 工作台作业面。金额、单据字段、明细全部展开，按区块分层。
 * 查看/打开在页头图标；通过、驳回留在底栏。
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
    const canReadPaper = Boolean(
        workspacePaperKind(item.businessObjectType) &&
        item.businessObjectId.trim(),
    )
    const readActionLabel = workspaceReadActionLabel(item.businessObjectType)
    const openActionLabel = workspaceOpenActionLabel(
        item.workItemType,
        item.businessObjectType,
    )
    const [paperOpen, setPaperOpen] = useState(false)
    useEffect(() => {
        setPaperOpen(false)
    }, [item.workItemId])
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
    const documentFields = [...detailFacts.keyFields, ...detailFacts.moreFields]
    const lineCount = (briefLines?.length ?? 0) + (briefMoreCount ?? 0)
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
    const documentActions =
        canReadPaper || documentHref ? (
            <div className="flex shrink-0 items-center gap-1">
                {canReadPaper ? (
                    <IconActionButton
                        label={readActionLabel}
                        testId={`work-item-read-document-${item.workItemId}`}
                        onClick={() => setPaperOpen(true)}
                    >
                        <FileTextIcon aria-hidden="true" />
                    </IconActionButton>
                ) : null}
                {documentHref ? (
                    <IconActionButton
                        label={openActionLabel}
                        testId={`work-item-open-document-${item.workItemId}`}
                        href={documentHref}
                    >
                        <ArrowUpRightIcon aria-hidden="true" />
                    </IconActionButton>
                ) : null}
            </div>
        ) : null
    const actions = approvalTask ? (
        <ApprovalActionBar
            allowedActions={item.allowedActions}
            recoveryOptions={recoveryQuery.data?.actions ?? []}
            workItemId={item.workItemId}
            expectedTaskVersion={item.taskVersion}
            instance={instance}
            canReadSensitive={canReadSensitive}
            approveWithoutDialog
            hiddenActions={["OPEN_DOCUMENT", "VIEW"]}
            onDecisionApplied={(view) =>
                onDecisionApplied?.(view, item.workItemId)
            }
        />
    ) : null

    return (
        <section className="flex h-full min-h-0 flex-col" aria-label="当前任务">
            <div className="min-h-0 flex-1 overflow-auto">
                <div className="flex w-full flex-col">
                    <header className="flex items-start justify-between gap-3 border-b border-grid py-5">
                        <div className="flex min-w-0 flex-col gap-2">
                            <div className="flex flex-wrap items-center gap-2">
                                <WorkspaceDocumentBadge item={item} />
                                {blocked ? (
                                    <StatusBadge label="受阻" tone="warning" />
                                ) : overdue ? (
                                    <StatusBadge
                                        label="已超期"
                                        tone="destructive"
                                    />
                                ) : null}
                            </div>
                            <h2 className="text-xl font-semibold tracking-tight">
                                {item.objectTitle}
                            </h2>
                            <p className="text-sm text-muted-foreground">
                                {canReadSensitive
                                    ? subtitle
                                    : "当前账号无权查看部分业务字段"}
                            </p>
                        </div>
                        {documentActions}
                    </header>

                    {primaryAmount ? (
                        <DetailBlock title="金额">
                            <div
                                className={cn(
                                    surfaceInsetClassName,
                                    "flex flex-wrap items-end gap-x-8 gap-y-2 px-4 py-4",
                                )}
                            >
                                <div className="flex flex-col gap-1">
                                    <span className="num text-3xl font-semibold tracking-tight">
                                        {primaryAmount.value}
                                    </span>
                                    <span className="text-xs text-muted-foreground">
                                        {primaryAmount.label}
                                    </span>
                                </div>
                                {otherAmounts.map((amount) => (
                                    <div
                                        key={amount.label}
                                        className="flex flex-col gap-1"
                                    >
                                        <span className="num text-lg font-medium">
                                            {amount.value}
                                        </span>
                                        <span className="text-xs text-muted-foreground">
                                            {amount.label}
                                        </span>
                                    </div>
                                ))}
                            </div>
                        </DetailBlock>
                    ) : documentFacts.isPending ? (
                        <DetailBlock title="金额">
                            <p className="text-sm text-muted-foreground">
                                正在读取单据事实…
                            </p>
                        </DetailBlock>
                    ) : impactSummary ? (
                        <DetailBlock title="说明">
                            <p className="text-sm">{impactSummary}</p>
                        </DetailBlock>
                    ) : null}

                    {documentFields.length > 0 ? (
                        <DetailBlock title="单据信息">
                            <FieldGrid sections={documentFields} />
                        </DetailBlock>
                    ) : null}

                    {briefLines && briefLines.length > 0 ? (
                        <DetailBlock
                            title="明细"
                            description={`${lineCount} 行`}
                        >
                            <ul className="flex flex-col text-sm">
                                {briefLines.map((line, lineIndex) => (
                                    <li
                                        key={`${lineIndex}:${line.title}`}
                                        className="flex justify-between gap-4 border-b border-border/40 py-2.5 last:border-b-0"
                                    >
                                        <span className="min-w-0">
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
                                    {canReadPaper
                                        ? `另有 ${briefMoreCount} 行，${readActionLabel}可看全部明细`
                                        : `另有 ${briefMoreCount} 行，${openActionLabel}查看`}
                                </p>
                            ) : null}
                        </DetailBlock>
                    ) : null}

                    {item.actionBlockers.length > 0 ? (
                        <DetailBlock title="处理受阻">
                            <Alert variant="warning">
                                <AlertTitle>当前无法继续</AlertTitle>
                                <AlertDescription>
                                    {item.actionBlockers[0]?.message}
                                </AlertDescription>
                            </Alert>
                        </DetailBlock>
                    ) : null}

                    {approvalTask ? (
                        instance ? (
                            <DetailBlock title="审批">
                                <RuntimeSummary instance={instance} compact />
                            </DetailBlock>
                        ) : null
                    ) : item.impactSummary && primaryAmount ? (
                        <DetailBlock title="说明">
                            <p className="text-sm">{item.impactSummary}</p>
                        </DetailBlock>
                    ) : null}
                </div>
            </div>
            {actions || item.nextActionHint ? (
                <div
                    className={cn(
                        "flex shrink-0 items-center gap-4 border-t border-border/40 py-3",
                        actions ? "justify-between" : "justify-end",
                    )}
                >
                    {actions}
                    {item.nextActionHint ? (
                        <p className="max-w-sm text-right text-xs text-muted-foreground">
                            {item.nextActionHint}
                        </p>
                    ) : null}
                </div>
            ) : null}
            {canReadPaper ? (
                <WorkspaceDocumentPaperDialog
                    item={item}
                    open={paperOpen}
                    onOpenChange={setPaperOpen}
                />
            ) : null}
        </section>
    )
}

function IconActionButton({
    label,
    testId,
    href,
    onClick,
    children,
}: {
    label: string
    testId: string
    href?: string
    onClick?: () => void
    children: ReactNode
}) {
    return (
        <Tooltip>
            <TooltipTrigger
                render={
                    href ? (
                        <a
                            href={href}
                            aria-label={label}
                            data-testid={testId}
                            className={buttonVariants({
                                variant: "ghost",
                                size: "icon-sm",
                            })}
                        />
                    ) : (
                        <Button
                            type="button"
                            variant="ghost"
                            size="icon-sm"
                            aria-label={label}
                            data-testid={testId}
                            onClick={onClick}
                        />
                    )
                }
            >
                {children}
            </TooltipTrigger>
            <TooltipContent>{label}</TooltipContent>
        </Tooltip>
    )
}

/** 作业面内的带标题区块。分区线用来拉开层次，内容默认全部展开。 */
function DetailBlock({
    title,
    description,
    children,
}: {
    title: string
    description?: string
    children: ReactNode
}) {
    return (
        <section className="flex flex-col gap-3 border-b border-grid py-5 last:border-b-0">
            <header className="flex items-baseline gap-2">
                <h3 className="text-sm font-medium">{title}</h3>
                {description ? (
                    <p className="text-xs text-muted-foreground">
                        {description}
                    </p>
                ) : null}
            </header>
            {children}
        </section>
    )
}

/** 单据字段两列键值表。 */
function FieldGrid({
    sections,
    className,
}: {
    sections: readonly DetailSection[]
    className?: string
}) {
    return (
        <DescriptionList columns="two" className={className}>
            {sections.map((section) => (
                <DescriptionItem key={section.label}>
                    <DescriptionTerm>{section.label}</DescriptionTerm>
                    <DescriptionDetails
                        className={cn(section.numeric && "num")}
                        title={section.value}
                    >
                        {section.value}
                    </DescriptionDetails>
                </DescriptionItem>
            ))}
        </DescriptionList>
    )
}
