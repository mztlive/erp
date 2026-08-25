"use client"

import { useEffect, useState, type ReactNode } from "react"
import { usePathname, useSearchParams } from "next/navigation"
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
import { useWorkspaceSourceSalesOrder } from "../hooks/use-workspace-source-sales-order"
import { buildDocumentHref } from "../lib/destination"
import { type DetailSection, splitDetailSections } from "../lib/detail-facts"
import { isApprovalWorkbenchTask } from "../lib/navigation-eligibility"
import { workspacePaperKind } from "../lib/paper-kind"
import {
    linkedDocumentHref,
    linkedDocumentPaperKind,
    withSourceSalesOrder,
} from "../lib/source-sales-order"
import { isBlockedWorkItem } from "../lib/work-item"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceDocumentBadge } from "./workspace-document-badge"
import {
    WorkspaceDocumentPaperDialog,
    type WorkspacePaperTarget,
} from "./workspace-document-paper-dialog"

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
    const currentPaperKind = workspacePaperKind(item.businessObjectType)
    const canReadPaper = Boolean(
        currentPaperKind && item.businessObjectId.trim(),
    )
    const readActionLabel = workspaceReadActionLabel(item.businessObjectType)
    const openActionLabel = workspaceOpenActionLabel(
        item.workItemType,
        item.businessObjectType,
    )
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const returnTo = `${pathname}${searchParams.toString() ? `?${searchParams}` : ""}`
    const [paper, setPaper] = useState<WorkspacePaperTarget | null>(null)
    useEffect(() => {
        setPaper(null)
    }, [item.workItemId])
    const documentFacts = useWorkspaceDocumentFacts(item)
    const sourceSales = useWorkspaceSourceSalesOrder(item)
    const facts = documentFacts.facts
    const summarySections = withSourceSalesOrder(
        facts?.sections ?? item.summarySections ?? [],
        sourceSales.source,
    )
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
        sourceSales.source ? `来源 ${sourceSales.source.orderNo}` : undefined,
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
        canReadPaper || (approvalTask && documentHref) ? (
            <div className="flex shrink-0 items-center gap-1">
                {canReadPaper && currentPaperKind ? (
                    <IconActionButton
                        label={readActionLabel}
                        testId={`work-item-read-document-${item.workItemId}`}
                        onClick={() =>
                            setPaper({
                                kind: currentPaperKind,
                                objectId: item.businessObjectId,
                                title: item.stableNumber,
                            })
                        }
                    >
                        <FileTextIcon aria-hidden="true" />
                    </IconActionButton>
                ) : null}
                {approvalTask && documentHref ? (
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
    ) : documentHref ? (
        <Button
            type="button"
            data-testid={`work-item-open-document-${item.workItemId}`}
            render={<a href={documentHref} aria-label={openActionLabel} />}
        >
            {openActionLabel}
        </Button>
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
                            <FieldGrid
                                sections={documentFields}
                                returnTo={returnTo}
                                onPreview={(section) => {
                                    const kind = linkedDocumentPaperKind(
                                        section.label,
                                    )
                                    if (!kind || !section.objectId) return
                                    setPaper({
                                        kind,
                                        objectId: section.objectId,
                                        title: `${section.label} ${section.value}`,
                                    })
                                }}
                            />
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
            <WorkspaceDocumentPaperDialog
                target={paper}
                open={Boolean(paper)}
                onOpenChange={(open) => {
                    if (!open) setPaper(null)
                }}
            />
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

/** 单据字段两列键值表。关联单据可预览或跳转。 */
function FieldGrid({
    sections,
    returnTo,
    onPreview,
    className,
}: {
    sections: readonly DetailSection[]
    returnTo: string
    onPreview: (section: DetailSection) => void
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
                        {section.objectId ? (
                            <LinkedDocumentValue
                                section={section}
                                returnTo={returnTo}
                                onPreview={onPreview}
                            />
                        ) : (
                            section.value
                        )}
                    </DescriptionDetails>
                </DescriptionItem>
            ))}
        </DescriptionList>
    )
}

function LinkedDocumentValue({
    section,
    returnTo,
    onPreview,
}: {
    section: DetailSection
    returnTo: string
    onPreview: (section: DetailSection) => void
}) {
    const objectId = section.objectId?.trim()
    if (!objectId) return section.value
    const href = linkedDocumentHref(section.label, objectId, returnTo)
    const canPreview = Boolean(linkedDocumentPaperKind(section.label))
    if (!href && !canPreview) return section.value
    return (
        <span className="inline-flex min-w-0 items-center gap-1">
            {canPreview ? (
                <button
                    type="button"
                    className="num min-w-0 truncate text-left text-primary underline-offset-2 hover:underline"
                    aria-label={`预览${section.label} ${section.value}`}
                    data-testid={`source-document-preview-${objectId}`}
                    onClick={() => onPreview(section)}
                >
                    {section.value}
                </button>
            ) : href ? (
                <a
                    href={href}
                    className="num min-w-0 truncate text-primary underline-offset-2 hover:underline"
                    aria-label={`打开${section.label} ${section.value}`}
                >
                    {section.value}
                </a>
            ) : (
                section.value
            )}
            {href ? (
                <IconActionButton
                    label={`打开${section.label}`}
                    testId={`source-document-open-${objectId}`}
                    href={href}
                >
                    <ArrowUpRightIcon aria-hidden="true" />
                </IconActionButton>
            ) : null}
        </span>
    )
}
