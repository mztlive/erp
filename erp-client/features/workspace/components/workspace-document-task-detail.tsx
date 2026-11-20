"use client"

import { BusinessFailureState } from "@/components/business"
import { useEffect, useState, type ReactNode } from "react"
import { useMutation } from "@tanstack/react-query"
import { downloadFileAsset } from "@/features/file-assets/api"
import { usePathname, useSearchParams } from "next/navigation"
import { ArrowUpRightIcon, FileTextIcon } from "lucide-react"
import { ApprovalActionBar } from "@/features/approval-workflow/components/approval-action-bar"
import { WorkspacePurchasePayment } from "./workspace-purchase-payment"
import { WorkspaceApprovalReworkActions } from "./workspace-approval-rework"
import { WorkspaceRejectionNotice } from "./workspace-rejection-notice"
import { workspaceFulfillmentDescriptor } from "../lib/workspace-fulfillment"
import { WorkspaceDeliveryBrief } from "./workspace-delivery-brief"
import { WorkspaceInvoiceApproval } from "./workspace-invoice-approval"
import { WorkspaceReceiptApproval } from "./workspace-receipt-approval"
import { WorkspaceMoney } from "./workspace-money"
import { WorkspacePurchaseApprovalStatus } from "./workspace-purchase-approval-status"
import { WorkspacePurchaseApproval } from "./workspace-purchase-approval"
import { WorkspaceSalesApproval } from "./workspace-sales-approval"
import { WorkspaceApprovalProgress } from "./workspace-approval-progress"
import { WorkspaceApprovalRouteSummary } from "./workspace-approval-route-summary"
import { useWorkspaceTaskPage } from "@/components/business/workspace-task-pane"
import { useApprovalMaterials } from "@/features/approval-workflow/hooks/use-approval-materials"
import { useRecoveryOptionsQuery } from "@/features/approval-workflow/queries"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import {
    IconActionButton,
    taxAmountToneClass,
    WorkspaceTaskPane,
    workspaceTaskSurfacePadClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { displayName } from "@/lib/display-name"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"
import { StatusBadge } from "@/components/ui/status-badge"
import { WorkspaceDocumentBadge } from "./workspace-document-badge"
import { WorkspaceTaskHeaderActions } from "./workspace-task-context"
import { stripDocumentNumberPrefix } from "../lib/stable-number"
import {
    workspaceDocumentName,
    workspaceOpenActionLabel,
    workspaceReadActionLabel,
} from "../api/work-item-meta"
import { workspaceApprovalListFacts } from "../api/task-materials"
import { useWorkspaceDocumentFacts } from "../hooks/use-workspace-document-facts"
import { useWorkspaceDetailDocumentTitle } from "../hooks/use-workspace-detail-title"
import { useWorkspaceSourceSalesOrder } from "../hooks/use-workspace-source-sales-order"
import { buildDocumentHref } from "../lib/destination"
import { type DetailSection, splitDetailSections } from "../lib/detail-facts"
import {
    isApprovalWorkbenchTask,
    isPurchaseApprovalTask,
} from "../lib/navigation-eligibility"
import { workspacePaperKind } from "../lib/paper-kind"
import {
    linkedDocumentHref,
    linkedDocumentPaperKind,
    withSourceSalesOrder,
} from "../lib/source-sales-order"
import { isBlockedWorkItem } from "../lib/work-item"
import type { WorkspaceWorkItem } from "../types"
import {
    WorkspaceFulfillmentProcessAction,
    WorkspaceFulfillmentReassignAction,
} from "./workspace-fulfillment-task"
import {
    WorkspaceDocumentPaperDialog,
    type WorkspacePaperTarget,
} from "./workspace-document-paper-dialog"

const exactInvoiceFields = new Set([
    "开票抬头",
    "税号",
    "开票内容",
    "申请事由",
    "购方名称",
    "购方纳税人识别号",
    "销方名称",
    "销方纳税人识别号",
])

export function WorkspaceDocumentTaskDetail({
    item,
    reworkTask,
    canReadSensitive,
    onDecisionApplied,
    onTaskCompleted,
    grantedPermissions = [],
}: {
    item: WorkspaceWorkItem
    reworkTask?: WorkspaceWorkItem
    canReadSensitive: boolean
    grantedPermissions?: readonly string[]
    onTaskCompleted?: (workItemId: string) => void
    onDecisionApplied?: (
        view: ApprovalCommandView,
        completedWorkItemId: string,
    ) => void
}) {
    const fullPage = useWorkspaceTaskPage()
    const rework = item.workItemType === "APPROVAL_REWORK"
    const canReadSubmission =
        canReadSensitive &&
        (!rework || hasPermission(grantedPermissions, "approval_instance:read"))
    const approvalTask = isApprovalWorkbenchTask(
        item.allowedActions,
        item.approvalProcessInstanceId,
        item.approvalNodeExecutionId,
    )
    const trackingTask = item.workItemType === "APPROVAL_INSTANCE"
    const approvalDocument =
        approvalTask ||
        trackingTask ||
        item.workItemType === "DOCUMENT_APPROVAL" ||
        rework
    const instanceId =
        item.approvalProcessInstanceId ?? item.approval?.instanceId
    const recoveryQuery = useRecoveryOptionsQuery(
        instanceId,
        approvalTask && isBlockedWorkItem(item),
    )
    const documentHref = buildDocumentHref(item)
    const currentPaperKind = workspacePaperKind(item.businessObjectType)
    const canReadPaper = Boolean(
        approvalDocument
            ? instanceId && (!rework || canReadSubmission)
            : currentPaperKind && item.businessObjectId.trim(),
    )
    const salesApproval =
        approvalDocument &&
        ["sales_order", "voucher_sales_order"].includes(item.businessObjectType)
    const purchaseApproval =
        isPurchaseApprovalTask(item) && (!rework || canReadSubmission)
    const receiptApproval =
        approvalDocument &&
        item.businessObjectType === "customer_receipt" &&
        Boolean(instanceId) &&
        canReadSubmission
    const invoiceApproval =
        approvalDocument &&
        item.businessObjectType === "sales_invoice_request" &&
        Boolean(instanceId) &&
        canReadSubmission
    const readActionLabel = approvalDocument
        ? salesApproval
            ? "预览完整销售单"
            : "查看提交资料"
        : item.workItemType === "FULFILLMENT_OPERATION"
          ? `查看来源${item.businessObjectType === "delivery" ? "销售" : "采购"}单`
          : workspaceReadActionLabel(item.businessObjectType)
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
    const materials = useApprovalMaterials(
        instanceId ?? "",
        (salesApproval || rework) && canReadSubmission,
        item,
    )
    const submission = canReadSubmission ? materials.data : undefined
    const salesSubmission = submission?.sales_order
    const reworkFacts =
        rework && submission
            ? workspaceApprovalListFacts({
                  ...submission,
                  ...submission.display,
                  source_sales: submission.display.source_sales ?? [],
              })
            : undefined
    const facts = rework ? reworkFacts : documentFacts.facts
    const sections =
        facts?.sections ?? (rework ? [] : (item.summarySections ?? []))
    const summarySections = approvalDocument
        ? sections.map((section) => ({
              ...section,
              value:
                  rework && section.objectId?.trim() === section.value.trim()
                      ? "未记录名称"
                      : section.value,
              objectId: undefined,
          }))
        : withSourceSalesOrder(sections, sourceSales.source)
    const briefLines = facts?.lines ?? (rework ? undefined : item.briefLines)
    const briefMoreCount =
        facts?.moreCount ?? (rework ? undefined : item.briefMoreCount)
    const counterpartyName = rework
        ? displayName(
              salesSubmission?.customer_name ?? reworkFacts?.counterparty,
              item.businessObjectId,
          )
        : (facts?.counterparty ?? item.counterpartyName)
    const documentNumber = rework
        ? displayName(
              reworkFacts?.documentNumber,
              item.businessObjectId,
              item.workItemId,
              instanceId,
          )
        : stripDocumentNumberPrefix(item.stableNumber)
    const headerTitle = rework
        ? counterpartyName ||
          workspaceDocumentName(item.businessObjectType) ||
          "原单资料"
        : purchaseApproval
          ? "采购单审批"
          : counterpartyName || item.objectTitle
    const headerItem = rework
        ? {
              ...item,
              counterpartyName,
              objectTitle: headerTitle,
              stableNumber: documentNumber ?? "",
          }
        : item
    useWorkspaceDetailDocumentTitle(fullPage && rework ? headerItem : undefined)
    const impactSummary =
        facts?.impact &&
        (item.impactSummary === "不审批则卡券销售不能生效" ||
            !item.summarySections?.length)
            ? facts.impact
            : item.impactSummary
    const detailFacts = splitDetailSections(summarySections, counterpartyName)
    const [primaryAmount, ...otherAmounts] = detailFacts.amounts
    const documentFields = [
        ...(detailFacts.submitter
            ? [{ label: "申请人", value: detailFacts.submitter }]
            : []),
        ...(salesSubmission
            ? [
                  {
                      label: "开票要求",
                      value: salesSubmission.invoice_type || "—",
                  },
                  {
                      label: "收款到期日",
                      value: salesSubmission.receivable_due_date || "—",
                  },
              ]
            : []),
        ...detailFacts.keyFields,
        ...detailFacts.moreFields,
    ]
    const visibleDocumentFields = documentFields.filter(
        ({ label }) =>
            !(
                item.businessObjectType === "sales_invoice_request" &&
                label === "申请金额"
            ) && !(invoiceApproval && exactInvoiceFields.has(label)),
    )
    const purchasePaymentTerm = documentFields.find(
        (field) => field.label === "付款条件",
    )?.value
    const lineCount = (briefLines?.length ?? 0) + (briefMoreCount ?? 0)
    const instance = item.approval
        ? {
              id: item.approval.instanceId,
              status: item.approval.status,
              currentRoundNo: item.approval.currentRoundNo,
              currentNodeName: item.approval.currentNodeLabel,
              currentAssigneeName: item.approval.currentAssigneeLabel,
              latestRejection: item.approval.lastRejectReason,
              latestRejectionByName: item.approval.lastRejectorLabel,
              processName: item.approval.processName,
              processVersion: item.approval.processVersion,
          }
        : undefined
    const specializedApproval =
        salesApproval ||
        purchaseApproval ||
        receiptApproval ||
        item.businessObjectType === "sales_invoice_request"
    const showCurrentAssignee = rework
        ? item.status === "OPEN"
        : salesApproval &&
          (reworkTask ||
              ["RUNNING", "BLOCKED"].includes(instance?.status ?? ""))
    const deliveryBrief =
        item.workItemType === "FULFILLMENT_OPERATION" &&
        item.businessObjectType === "delivery" &&
        item.deliveryLines !== undefined
    const actions = reworkTask ? (
        <WorkspaceApprovalReworkActions
            item={reworkTask}
            onTaskCompleted={onTaskCompleted}
        />
    ) : approvalTask && !trackingTask ? (
        <ApprovalActionBar
            presentation="workspace"
            showInlineOpinion={!purchaseApproval}
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
    ) : item.workItemType === "FULFILLMENT_OPERATION" ? (
        <WorkspaceFulfillmentProcessAction
            item={item}
            grantedPermissions={grantedPermissions}
            onTaskCompleted={(id) => onTaskCompleted?.(id)}
        />
    ) : null
    const canOpenDocument = Boolean(
        !approvalDocument &&
        documentHref &&
        (approvalTask ||
            trackingTask ||
            item.allowedActions.includes("PROCESS") ||
            item.allowedActions.includes("VIEW") ||
            item.allowedActions.includes("OPEN_DOCUMENT")),
    )

    const purchaseInformation = purchaseApproval ? (
        <FieldGrid
            inline
            className="border-t border-grid/60 pt-3"
            sections={documentFields.filter(
                (field) =>
                    field.label !== "供应商" &&
                    field.label !== "来源销售单" &&
                    field.label !== "提交号" &&
                    !(primaryAmount && field.label === "付款条件"),
            )}
            returnTo={returnTo}
            onPreview={(section) => {
                const kind = linkedDocumentPaperKind(section.label)
                if (!kind || !section.objectId) return
                setPaper({
                    kind,
                    objectId: section.objectId,
                    title: `${section.label} ${section.value}`,
                })
            }}
        />
    ) : null

    return (
        <WorkspaceTaskPane
            className={cn(
                "@container/document",
                purchaseApproval && "purchase-approval-surface",
            )}
            header={
                <>
                    <div className="min-w-0 space-y-2">
                        {(!purchaseApproval || rework) && (
                            <div className="flex flex-wrap items-center gap-2">
                                {rework ? (
                                    <WorkspaceDocumentBadge item={item} />
                                ) : (
                                    <span className="rounded-md bg-muted px-2 py-1 text-xs text-muted-foreground">
                                        {item.workItemTypeLabel}
                                    </span>
                                )}
                                <StatusBadge
                                    label={
                                        rework && item.status === "OPEN"
                                            ? item.workItemTypeLabel
                                            : item.statusLabel
                                    }
                                    tone={item.statusTone}
                                />
                                {salesApproval &&
                                instance?.currentNodeName &&
                                (instance.status === "RUNNING" ||
                                    instance.status === "BLOCKED") ? (
                                    <Badge
                                        variant="warning"
                                        aria-label={`当前审批：${instance.currentNodeName}`}
                                    >
                                        {instance.currentNodeName}
                                    </Badge>
                                ) : null}
                            </div>
                        )}
                        <h2 className="break-words text-xl font-medium tracking-tight">
                            {canReadSensitive ? headerTitle : "业务详情"}
                        </h2>
                        {rework && submission ? (
                            <div className="flex flex-wrap gap-x-6 gap-y-1 text-sm text-muted-foreground">
                                <p className="min-w-0 break-all">
                                    单号：
                                    <span className="num text-foreground select-all">
                                        {documentNumber || "未提供"}
                                    </span>
                                </p>
                                <p>提交版本 {submission.subject_version}</p>
                            </div>
                        ) : !rework ? (
                            <p className="break-all text-sm text-muted-foreground">
                                {documentNumber}
                            </p>
                        ) : null}
                        {fullPage && showCurrentAssignee ? (
                            <div className="flex flex-wrap gap-x-6 gap-y-1 text-xs text-muted-foreground">
                                <p>
                                    {reworkTask
                                        ? "当前处理人："
                                        : "当前审批人："}
                                    <span className="text-foreground">
                                        {reworkTask?.ownerUserLabel ||
                                            instance?.currentAssigneeName ||
                                            item.ownerUserLabel ||
                                            "—"}
                                    </span>
                                </p>
                                {item.dueAt ? (
                                    <p>截止时间：{item.dueAtLabel}</p>
                                ) : null}
                            </div>
                        ) : null}
                    </div>
                    <WorkspaceTaskHeaderActions item={headerItem}>
                        {item.workItemType === "FULFILLMENT_OPERATION" ? (
                            <WorkspaceFulfillmentReassignAction
                                item={item}
                                onReassigned={() =>
                                    onTaskCompleted?.(item.workItemId)
                                }
                            />
                        ) : null}
                        {canReadPaper &&
                        !(
                            approvalDocument &&
                            item.businessObjectType === "customer_receipt"
                        ) &&
                        !purchaseApproval &&
                        !salesApproval &&
                        (approvalDocument || currentPaperKind) ? (
                            <IconActionButton
                                id={`workspace-task-detail-read-${toAutomationIdSegment(item.workItemId)}`}
                                label={readActionLabel}
                                testId={`work-item-read-document-${item.workItemId}`}
                                onClick={() =>
                                    setPaper(
                                        approvalDocument && instanceId
                                            ? {
                                                  kind: "approval_snapshot",
                                                  objectId: instanceId,
                                                  title: documentNumber,
                                              }
                                            : currentPaperKind
                                              ? {
                                                    kind: currentPaperKind,
                                                    objectId:
                                                        item.businessObjectId,
                                                    title: item.stableNumber,
                                                }
                                              : null,
                                    )
                                }
                            >
                                <FileTextIcon aria-hidden="true" />
                            </IconActionButton>
                        ) : null}
                        {canOpenDocument && documentHref ? (
                            <IconActionButton
                                id={`workspace-task-detail-open-${toAutomationIdSegment(item.workItemId)}`}
                                label={
                                    canReadPaper
                                        ? openActionLabel
                                        : readActionLabel
                                }
                                testId={`work-item-open-document-${item.workItemId}`}
                                href={documentHref}
                            >
                                {canReadPaper ? (
                                    <ArrowUpRightIcon aria-hidden="true" />
                                ) : (
                                    <FileTextIcon aria-hidden="true" />
                                )}
                            </IconActionButton>
                        ) : null}
                    </WorkspaceTaskHeaderActions>
                </>
            }
            headerSupplement={
                salesApproval && fullPage && instance ? (
                    <WorkspaceApprovalRouteSummary instance={instance} />
                ) : null
            }
            footer={actions}
            aria-label="当前任务"
        >
            {(rework
                ? canReadSubmission && Boolean(instanceId)
                : item.approval?.status === "REJECTED") && (
                <WorkspaceRejectionNotice
                    item={item}
                    actionable={Boolean(
                        reworkTask?.allowedActions.includes("PROCESS"),
                    )}
                />
            )}
            <div className="@container/delivery-brief flex w-full min-w-0 flex-col">
                {rework && (!instanceId || !canReadSubmission) ? (
                    <DetailBlock>
                        <p
                            className="text-sm text-muted-foreground"
                            role="status"
                        >
                            {canReadSubmission
                                ? "未找到本次驳回关联的提交资料，请刷新任务后重试。"
                                : "当前账号无权查看本次提交资料。"}
                        </p>
                    </DetailBlock>
                ) : rework && !specializedApproval && materials.isPending ? (
                    <DetailBlock>
                        <p
                            className="text-sm text-muted-foreground"
                            role="status"
                        >
                            正在读取本次提交资料…
                        </p>
                    </DetailBlock>
                ) : rework && !specializedApproval && materials.isError ? (
                    <BusinessFailureState
                        id={`workspace-rework-materials-${toAutomationIdSegment(item.workItemId)}`}
                        title="原单资料读取失败"
                        error={materials.error}
                        onRetry={() => void materials.refetch()}
                        className={workspaceTaskSurfacePadClassName}
                    />
                ) : null}
                {item.documentSummaryResolved &&
                !item.summarySections?.length &&
                !item.briefLines?.length ? (
                    <p
                        className="px-5 py-4 text-sm text-muted-foreground"
                        role="status"
                    >
                        此次提交的单据摘要暂不可用。
                    </p>
                ) : null}

                {sourceSales.error ? (
                    <BusinessFailureState
                        id={`workspace-source-materials-${toAutomationIdSegment(item.workItemId)}`}
                        error={sourceSales.error}
                        onRetry={() => void sourceSales.refetch()}
                        className={workspaceTaskSurfacePadClassName}
                    />
                ) : null}

                {documentFacts.isError ? (
                    <BusinessFailureState
                        id={`workspace-materials-${toAutomationIdSegment(item.workItemId)}`}
                        error={documentFacts.error}
                        onRetry={() => void documentFacts.refetch()}
                        className={workspaceTaskSurfacePadClassName}
                    />
                ) : null}

                {receiptApproval && instanceId ? (
                    <WorkspaceReceiptApproval
                        key={instanceId}
                        item={item}
                        instanceId={instanceId}
                    />
                ) : null}
                {deliveryBrief ? <WorkspaceDeliveryBrief item={item} /> : null}
                {!receiptApproval && !deliveryBrief && primaryAmount ? (
                    <DetailBlock>
                        <div className="flex flex-wrap items-end gap-x-8 gap-y-2">
                            <div className="flex flex-col gap-1">
                                <span
                                    className={cn(
                                        "num text-3xl font-semibold tracking-tight",
                                        "text-foreground",
                                    )}
                                >
                                    <WorkspaceMoney
                                        value={primaryAmount.value}
                                    />
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
                                        <WorkspaceMoney value={amount.value} />
                                    </span>
                                    <span className="text-xs text-muted-foreground">
                                        {amount.label}
                                    </span>
                                </div>
                            ))}
                            {purchaseApproval && lineCount > 0 && (
                                <div className="flex flex-col gap-1">
                                    <span className="num text-lg font-medium">
                                        {lineCount} 项
                                    </span>
                                    <span className="text-xs text-muted-foreground">
                                        采购明细
                                    </span>
                                </div>
                            )}
                            {purchaseApproval && purchasePaymentTerm && (
                                <div className="flex flex-col gap-1">
                                    <span className="text-sm font-medium">
                                        {purchasePaymentTerm}
                                    </span>
                                    <span className="text-xs text-muted-foreground">
                                        付款条件
                                    </span>
                                </div>
                            )}
                            {purchaseApproval && (
                                <WorkspacePurchaseApprovalStatus
                                    item={item}
                                    instance={instance}
                                />
                            )}
                        </div>
                        {purchaseInformation}
                    </DetailBlock>
                ) : !receiptApproval &&
                  !deliveryBrief &&
                  documentFacts.isPending ? (
                    <DetailBlock title="金额">
                        <p className="text-sm text-muted-foreground">
                            正在加载单据详情…
                        </p>
                    </DetailBlock>
                ) : null}

                {purchaseApproval && !primaryAmount && (
                    <DetailBlock>
                        <WorkspacePurchaseApprovalStatus
                            item={item}
                            instance={instance}
                        />
                        {purchaseInformation}
                    </DetailBlock>
                )}
                {purchaseApproval && instanceId ? (
                    <WorkspacePurchaseApproval
                        item={item}
                        key={instanceId}
                        instanceId={instanceId}
                        onPreview={setPaper}
                    />
                ) : (
                    <>
                        {!receiptApproval &&
                        !deliveryBrief &&
                        visibleDocumentFields.length > 0 ? (
                            <DetailBlock>
                                <FieldGrid
                                    sections={visibleDocumentFields}
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
                    </>
                )}

                {invoiceApproval && instanceId ? (
                    <WorkspaceInvoiceApproval
                        item={item}
                        key={instanceId}
                        instanceId={instanceId}
                        onPreview={setPaper}
                    />
                ) : null}

                {salesApproval && instanceId && canReadSubmission ? (
                    <WorkspaceSalesApproval
                        item={item}
                        key={instanceId}
                        instanceId={instanceId}
                        onPreview={setPaper}
                    />
                ) : null}

                {!invoiceApproval &&
                !(salesApproval && instanceId && canReadSubmission) &&
                !deliveryBrief &&
                !receiptApproval &&
                !purchaseApproval &&
                briefLines &&
                briefLines.length > 0 ? (
                    <DetailBlock
                        title={approvalDocument ? "提交资料摘要" : "明细"}
                        description={`${lineCount} 行`}
                    >
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
                        {approvalDocument && instanceId ? (
                            <Button
                                id={`workspace-task-detail-preview-submission-${toAutomationIdSegment(item.workItemId)}`}
                                type="button"
                                variant="outline"
                                size="sm"
                                className="mt-3 self-start"
                                onClick={() =>
                                    setPaper({
                                        kind: "approval_snapshot",
                                        objectId: instanceId,
                                        title: item.stableNumber,
                                    })
                                }
                            >
                                <FileTextIcon aria-hidden="true" />
                                {readActionLabel}
                            </Button>
                        ) : null}
                        {briefMoreCount ? (
                            <p className="text-xs text-muted-foreground">
                                {approvalDocument
                                    ? salesApproval
                                        ? `此处展示前 ${briefLines.length} 行，共 ${lineCount} 行。预览完整销售单可查看本次提交的全部明细。`
                                        : `本摘要未包含另外 ${briefMoreCount} 行，请查看提交资料及附件。`
                                    : canReadPaper
                                      ? `另有 ${briefMoreCount} 行，${readActionLabel}可看全部明细`
                                      : `另有 ${briefMoreCount} 行，${openActionLabel}查看`}
                            </p>
                        ) : null}
                    </DetailBlock>
                ) : null}

                {workspaceFulfillmentDescriptor(item)?.role ===
                "procurement" ? (
                    <DetailBlock compact>
                        <WorkspacePurchasePayment
                            key={item.workItemId}
                            workItemId={item.workItemId}
                        />
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

                {!fullPage && !purchaseApproval && approvalTask && instance ? (
                    <DetailBlock compact={purchaseApproval || receiptApproval}>
                        <WorkspaceApprovalProgress
                            instance={instance}
                            compact={purchaseApproval || receiptApproval}
                        />
                    </DetailBlock>
                ) : null}
            </div>
            <WorkspaceDocumentPaperDialog
                reference={item}
                target={paper}
                open={Boolean(paper)}
                onOpenChange={(open) => {
                    if (!open) setPaper(null)
                }}
            />
        </WorkspaceTaskPane>
    )
}

/** 作业面内的带标题区块。分区线用来拉开层次，内容默认全部展开。 */
function DetailBlock({
    title,
    description,
    children,
    compact = false,
}: {
    title?: string
    compact?: boolean
    description?: string
    children: ReactNode
}) {
    return (
        <section
            className={cn(
                workspaceTaskSurfacePadClassName,
                "flex flex-col border-b border-grid last:border-b-0",
                compact ? "gap-2 py-3" : "gap-3 py-4",
            )}
        >
            {title ? (
                <header className="flex items-baseline gap-2">
                    <h3 className="text-sm font-medium">{title}</h3>
                    {description ? (
                        <p className="text-xs text-muted-foreground">
                            {description}
                        </p>
                    ) : null}
                </header>
            ) : null}
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
    inline = false,
}: {
    sections: readonly DetailSection[]
    returnTo: string
    onPreview: (section: DetailSection) => void
    className?: string
    inline?: boolean
}) {
    if (sections.length === 0) return null
    return (
        <DescriptionList
            columns="one"
            className={cn(
                inline
                    ? "flex flex-wrap gap-x-8 gap-y-2"
                    : "gap-y-2 @min-[640px]/document:grid-cols-2",
                className,
            )}
        >
            {sections.map((section) => (
                <DescriptionItem
                    key={section.label}
                    className={cn(
                        "items-baseline gap-3 space-y-0",
                        inline
                            ? "flex min-w-0"
                            : "grid grid-cols-[6rem_minmax(0,1fr)]",
                    )}
                >
                    <DescriptionTerm className="font-normal">
                        {section.label}
                    </DescriptionTerm>
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
    if (section.label === "履约凭证")
        return <FulfillmentEvidenceLink assetId={objectId} />
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

/** 凭证通过现有受控文件接口下载，禁止把内部 ID 或静态未授权地址交给页面。 */
function FulfillmentEvidenceLink({ assetId }: { assetId: string }) {
    const download = useMutation({
        mutationFn: () => downloadFileAsset(assetId, "履约凭证"),
    })
    return (
        <span className="inline-flex flex-col gap-1">
            <LoadingButton
                id={`workspace-fulfillment-evidence-${toAutomationIdSegment(assetId)}`}
                type="button"
                variant="link"
                className="h-auto justify-start p-0 text-left text-primary underline-offset-2 hover:underline"
                disabled={download.isPending}
                loading={download.isPending}
                onClick={() => download.mutate()}
            >
                {download.isPending ? "正在读取…" : "下载凭证"}
            </LoadingButton>
            {download.isError ? (
                <span role="alert" className="text-xs text-destructive">
                    凭证读取失败，请重试
                </span>
            ) : null}
        </span>
    )
}
