"use client"

import { useEffect, useState, type ReactNode } from "react"
import { usePathname, useSearchParams } from "next/navigation"
import { ArrowUpRightIcon, FileTextIcon } from "lucide-react"

import { ApprovalActionBar } from "@/features/approval-workflow/components/approval-action-bar"
import { RuntimeSummary } from "@/features/approval-workflow/components/runtime-summary"
import { useRecoveryOptionsQuery } from "@/features/approval-workflow/queries"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import {
    surfaceInsetClassName,
    taxAmountToneClass,
    WorkspaceTaskPane,
    workspaceTaskSurfaceClassName,
    workspaceTaskSurfacePadClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button, buttonVariants } from "@/components/ui/button"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { toAutomationIdSegment } from "@/lib/automation-id"
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
import { WorkspaceAcceptanceTask } from "./workspace-acceptance-task"
import { WorkspaceCardFundsTask } from "./workspace-card-funds-task"
import { WorkspaceFulfillmentTask } from "./workspace-fulfillment-task"
import { WorkspaceInvoiceTask } from "./workspace-invoice-task"
import { WorkspaceImportTask } from "./workspace-import-task"
import { WorkspaceIntegrationTask } from "./workspace-integration-task"
import { WorkspaceMasterMappingTask } from "./workspace-master-mapping-task"
import { WorkspacePaymentTask } from "./workspace-payment-task"
import { WorkspaceProcurementTask } from "./workspace-procurement-task"
import { WorkspaceSettlementTask } from "./workspace-settlement-task"
import { WorkspaceSupplierInvestigationTask } from "./workspace-supplier-investigation-task"
import { WorkspaceSupplyExceptionTask } from "./workspace-supply-exception-task"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"
import { WorkspaceTaskSurfaceBoundary } from "./workspace-task-surface-boundary"
import {
    WorkspaceDocumentPaperDialog,
    type WorkspacePaperTarget,
} from "./workspace-document-paper-dialog"

/**
 * 工作台作业面。金额、单据字段、明细全部展开，按区块分层。
 * 任务说明收在标题栏问号里，不单独占一块说明区。
 * 履约、付款、开票与客户验收在本页提交正式命令；审批通过、驳回留在底栏。
 */
type WorkspaceTaskDetailProps = Readonly<{
    item: WorkspaceWorkItem
    canReadSensitive?: boolean
    onDecisionApplied?: (
        view: ApprovalCommandView,
        completedWorkItemId: string,
    ) => void
    grantedPermissions?: readonly string[]
    onTaskCompleted?: (workItemId: string, preferredWorkItemId?: string) => void
}>

export function WorkspaceTaskDetail(props: WorkspaceTaskDetailProps) {
    return (
        <section
            data-slot="workspace-task-surface"
            className={cn(
                workspaceTaskSurfaceClassName,
                "relative flex h-full min-h-0 flex-1 flex-col",
            )}
            aria-label="当前工作台任务"
        >
            <div className="min-h-0 flex-1">
                <WorkspaceTaskSurfaceBoundary
                    workItemId={props.item.workItemId}
                >
                    <WorkspaceTaskSurface {...props} />
                </WorkspaceTaskSurfaceBoundary>
            </div>
        </section>
    )
}

function WorkspaceTaskSurface({
    item,
    canReadSensitive = true,
    onDecisionApplied,
    grantedPermissions = [],
    onTaskCompleted,
}: WorkspaceTaskDetailProps) {
    // 待供给分配：销售单生成采购 → WorkspaceProcurementTask
    if (
        item.workItemType === "PROCUREMENT_ORDER_CREATION" &&
        item.businessObjectType === "sales_order"
    ) {
        return (
            <WorkspaceProcurementTask
                item={item}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    // 供应商结算复核 → WorkspaceSettlementTask
    if (
        item.workItemType === "SUPPLIER_SETTLEMENT_REVIEW" &&
        item.businessObjectType === "supplier_settlement_statement"
    ) {
        return (
            <WorkspaceSettlementTask
                item={item}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    // 导入业务确认（历史导入批次）→ WorkspaceImportTask
    if (
        item.workItemType === "IMPORT_BUSINESS_CONFIRMATION" &&
        item.businessObjectType === "LEGACY_IMPORT_BATCH"
    ) {
        return (
            <WorkspaceImportTask
                item={item}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    // 卡券票款复核 / 卡券票款差异复核 → WorkspaceCardFundsTask
    if (
        (item.workItemType === "CARD_FUNDS_REVIEW" ||
            item.workItemType === "CARD_FUNDS_DELTA_REVIEW") &&
        item.businessObjectType === "receivable_account"
    ) {
        return (
            <WorkspaceCardFundsTask
                item={item}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    // 主数据映射异常 → WorkspaceMasterMappingTask
    if (
        item.workItemType === "BUSINESS_EXCEPTION" &&
        item.businessObjectType === "MASTER_MAPPING_TASK"
    ) {
        return (
            <WorkspaceMasterMappingTask
                item={item}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    // 集成结果未知或业务异常：接口差错 / 对账差异 → WorkspaceIntegrationTask
    if (
        (item.workItemType === "INTEGRATION_RESULT_UNKNOWN" ||
            item.workItemType === "BUSINESS_EXCEPTION") &&
        (item.businessObjectType === "integration_error_task" ||
            item.businessObjectType === "reconciliation_difference")
    ) {
        return (
            <WorkspaceIntegrationTask
                item={item}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    // 集成结果未知或业务异常：供应商履约单调查 → WorkspaceSupplierInvestigationTask
    if (
        (item.workItemType === "INTEGRATION_RESULT_UNKNOWN" ||
            item.workItemType === "BUSINESS_EXCEPTION") &&
        item.businessObjectType === "SUPPLIER_FULFILLMENT_ORDER"
    ) {
        return (
            <WorkspaceSupplierInvestigationTask
                item={item}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    // 业务异常：供应商可供货 → WorkspaceSupplyExceptionTask
    if (
        item.workItemType === "BUSINESS_EXCEPTION" &&
        item.businessObjectType === "SUPPLIER_OFFERING"
    ) {
        return (
            <WorkspaceSupplyExceptionTask
                item={item}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    // 履约处理：采购收货 / 仓发或代发 / 电子交付 / 服务履约 → WorkspaceFulfillmentTask
    if (
        item.workItemType === "FULFILLMENT_OPERATION" &&
        [
            "purchase_receipt",
            "delivery",
            "electronic_delivery",
            "service_fulfillment",
        ].includes(item.businessObjectType)
    ) {
        return (
            <WorkspaceFulfillmentTask
                item={item}
                grantedPermissions={grantedPermissions}
                onTaskCompleted={(workItemId) => onTaskCompleted?.(workItemId)}
            />
        )
    }

    // 供应商付款处理 → WorkspacePaymentTask
    if (
        item.workItemType === "SUPPLIER_PAYMENT_EXECUTION" &&
        item.businessObjectType === "payable_account"
    ) {
        return (
            <WorkspacePaymentTask
                item={item}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    // 销项开票处理 → WorkspaceInvoiceTask
    if (
        item.workItemType === "SALES_INVOICE_EXECUTION" &&
        item.businessObjectType === "receivable_account"
    ) {
        return (
            <WorkspaceInvoiceTask
                item={item}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    // 客户验收登记 → WorkspaceAcceptanceTask
    if (
        item.workItemType === "CUSTOMER_ACCEPTANCE_REGISTRATION" &&
        item.businessObjectType === "sales_order"
    ) {
        return (
            <WorkspaceAcceptanceTask
                item={item}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    // 非审批任务且没有登记专用作业面 → 停止处理并提示
    if (
        item.workItemType !== "DOCUMENT_APPROVAL" &&
        item.workItemType !== "APPROVAL_INSTANCE"
    ) {
        return (
            <WorkspaceTaskPane
                header={<WorkspaceTaskIdentityHeader item={item} />}
                aria-label="当前任务"
            >
                <Alert variant="destructive">
                    <AlertTitle>任务处理器未登记</AlertTitle>
                    <AlertDescription>
                        当前任务类型与业务对象没有签署原地处理面，系统已停止提供处理动作。请联系管理员核对任务类型、业务对象和处理器登记。
                    </AlertDescription>
                </Alert>
            </WorkspaceTaskPane>
        )
    }

    // 单据审批 / 审批实例 → WorkspaceDocumentTaskDetail（通用审批作业面）
    return (
        <WorkspaceDocumentTaskDetail
            item={item}
            canReadSensitive={canReadSensitive}
            onDecisionApplied={onDecisionApplied}
        />
    )
}

function WorkspaceDocumentTaskDetail({
    item,
    canReadSensitive,
    onDecisionApplied,
}: {
    item: WorkspaceWorkItem
    canReadSensitive: boolean
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
    const trackingTask = item.workItemType === "APPROVAL_INSTANCE"
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
    const actions = approvalTask ? (
        <ApprovalActionBar
            id={`workspace-task-detail-approval-${toAutomationIdSegment(item.workItemId)}`}
            allowedActions={item.allowedActions}
            recoveryOptions={recoveryQuery.data?.actions ?? []}
            workItemId={item.workItemId}
            expectedTaskVersion={item.taskVersion}
            instance={instance}
            canReadSensitive={canReadSensitive}
            hiddenActions={["OPEN_DOCUMENT", "VIEW"]}
            decisionContext={{
                documentLabel: item.objectTitle,
                amountLabel: primaryAmount?.value,
                currentNodeLabel: instance?.currentNodeName,
                impactSummary,
            }}
            onDecisionApplied={(view) =>
                onDecisionApplied?.(view, item.workItemId)
            }
        />
    ) : documentHref &&
      (trackingTask || item.allowedActions.includes("PROCESS")) ? (
        <Button
            id={`workspace-task-detail-open-${toAutomationIdSegment(item.workItemId)}`}
            type="button"
            data-testid={`work-item-open-document-${item.workItemId}`}
            render={
                <a
                    id={`workspace-task-detail-open-${toAutomationIdSegment(item.workItemId)}`}
                    href={documentHref}
                    aria-label={openActionLabel}
                />
            }
        >
            {openActionLabel}
        </Button>
    ) : null

    const paneFooter =
        actions || item.nextActionHint ? (
            <div
                className={cn(
                    "flex w-full min-w-0 flex-col items-stretch gap-2 sm:flex-row sm:items-center sm:gap-4",
                    actions ? "sm:justify-between" : "sm:justify-end",
                )}
            >
                {item.nextActionHint ? (
                    <p className="order-1 max-w-sm text-left text-xs text-muted-foreground sm:order-2 sm:text-right">
                        {item.nextActionHint}
                    </p>
                ) : null}
                {actions ? (
                    <div className="order-2 shrink-0 sm:order-1">{actions}</div>
                ) : null}
            </div>
        ) : undefined

    return (
        <WorkspaceTaskPane
            header={
                <WorkspaceTaskIdentityHeader
                    item={item}
                    subtitle={
                        canReadSensitive
                            ? subtitle
                            : "当前账号无权查看部分业务字段"
                    }
                >
                    {canReadPaper && currentPaperKind ? (
                        <IconActionButton
                            id={`workspace-task-detail-read-${toAutomationIdSegment(item.workItemId)}`}
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
                            id={`workspace-task-detail-open-${toAutomationIdSegment(item.workItemId)}`}
                            label={openActionLabel}
                            testId={`work-item-open-document-${item.workItemId}`}
                            href={documentHref}
                        >
                            <ArrowUpRightIcon aria-hidden="true" />
                        </IconActionButton>
                    ) : null}
                </WorkspaceTaskIdentityHeader>
            }
            footer={paneFooter}
            aria-label="当前任务"
        >
            <div className="flex w-full flex-col">
                {documentFacts.isError ? (
                    <div
                        className={cn(workspaceTaskSurfacePadClassName, "mt-4")}
                    >
                        <Alert variant="destructive">
                            <AlertTitle>单据补充信息读取失败</AlertTitle>
                            <AlertDescription className="flex flex-wrap items-center justify-between gap-3">
                                <span>
                                    {facts
                                        ? "已保留当前可见内容；部分最新信息可能暂未显示。"
                                        : "当前没有可展示的单据事实，请重试后再执行审批或财务操作。"}
                                </span>
                                <Button
                                    id={`workspace-task-detail-document-facts-retry-${toAutomationIdSegment(item.workItemId)}`}
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    onClick={() => void documentFacts.refetch()}
                                >
                                    重试
                                </Button>
                            </AlertDescription>
                        </Alert>
                    </div>
                ) : null}

                {primaryAmount ? (
                    <DetailBlock title="金额">
                        <div
                            className={cn(
                                surfaceInsetClassName,
                                "flex flex-wrap items-end gap-x-8 gap-y-2 px-4 py-4",
                            )}
                        >
                            <div className="flex flex-col gap-1">
                                <span
                                    className={cn(
                                        "num text-3xl font-semibold tracking-tight",
                                        taxAmountToneClass(primaryAmount.label),
                                    )}
                                >
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
                                    <span
                                        className={cn(
                                            "num text-lg font-medium",
                                            taxAmountToneClass(amount.label),
                                        )}
                                    >
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
                    <DetailBlock title="明细" description={`${lineCount} 行`}>
                        <ul className="flex flex-col text-sm">
                            {briefLines.map((line, lineIndex) => (
                                <li
                                    key={`${lineIndex}:${line.title}`}
                                    className="grid gap-1 border-b border-border/40 py-2.5 last:border-b-0 sm:grid-cols-[minmax(0,1fr)_auto] sm:gap-x-4"
                                >
                                    <span className="min-w-0">
                                        {line.title}
                                    </span>
                                    {line.quantity || line.dueLabel ? (
                                        <span className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-muted-foreground sm:max-w-md sm:justify-end sm:text-right">
                                            {line.quantity ? (
                                                <span className="num">
                                                    {line.quantity}
                                                </span>
                                            ) : null}
                                            {line.dueLabel ? (
                                                <span className="text-xs">
                                                    {line.dueLabel}
                                                </span>
                                            ) : null}
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
            <WorkspaceDocumentPaperDialog
                target={paper}
                open={Boolean(paper)}
                onOpenChange={(open) => {
                    if (!open) setPaper(null)
                }}
            />
        </WorkspaceTaskPane>
    )
}

function IconActionButton({
    id,
    label,
    testId,
    href,
    onClick,
    children,
}: {
    id?: string
    label: string
    testId: string
    href?: string
    onClick?: () => void
    children: ReactNode
}) {
    return (
        <Tooltip>
            <TooltipTrigger
                id={id}
                render={
                    href ? (
                        <a
                            id={id}
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
                            id={id}
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
        <section
            className={cn(
                workspaceTaskSurfacePadClassName,
                "flex flex-col gap-3 border-b border-grid py-5 last:border-b-0",
            )}
        >
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
                    id={`workspace-linked-document-preview-${toAutomationIdSegment(objectId)}`}
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
                    id={`workspace-linked-document-open-${toAutomationIdSegment(objectId)}`}
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
                    id={`workspace-linked-document-open-action-${toAutomationIdSegment(objectId)}`}
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
