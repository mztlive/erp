"use client"

import { useEffect, useMemo, useRef, useState, type ReactNode } from "react"
import { useMutation } from "@tanstack/react-query"
import { usePathname, useSearchParams } from "next/navigation"
import { ArrowUpRightIcon, FileTextIcon, PaperclipIcon } from "lucide-react"

import {
    ApprovalRouteStepper,
    BusinessFailureState,
    DetailWorkspaceMeta,
    DetailWorkspaceSection,
    IconActionButton,
    detailWorkspaceStyles,
    taxAmountToneClass,
    useWorkspaceTaskPage,
    type MaterialFileItem,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { LoadingButton } from "@/components/ui/loading-button"
import { fetchApprovalMaterialPreview } from "@/features/approval-workflow/api/materials"
import { ApprovalActionBar } from "@/features/approval-workflow/components/approval-action-bar"
import { ApprovalMaterialFiles } from "@/features/approval-workflow/components/approval-material-files"
import { ApprovalSubmittedSummary } from "@/features/approval-workflow/components/approval-submitted-summary"
import {
    FinancialApprovalRenderer,
    type FinancialApprovalRendererHandle,
    buildFinancialDynamicRejectPhrases,
    isFinancialApprovalDocumentType,
    isReversalDocumentType,
} from "@/features/approval-workflow/components/financial-approval-renderer"
import type { ReworkRejectionContext } from "@/features/approval-workflow/components/rework-rejection-echo"
import {
    StructuredDiffApprovalRenderer,
    type StructuredDiffRendererHandle,
    buildStructuredDiffDynamicRejectPhrases,
    isStructuredDiffDocumentType,
} from "@/features/approval-workflow/components/structured-diff-approval"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    approvalSalesTaxPoint,
    displayUnixDate,
    displayUnixSeconds,
} from "@/features/approval-workflow/display"
import {
    useApprovalMaterials,
    useDownloadApprovalMaterial,
} from "@/features/approval-workflow/hooks/use-approval-materials"
import {
    useApprovalHistoryInfiniteQuery,
    useRecoveryOptionsQuery,
} from "@/features/approval-workflow/queries"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { timelineDocumentTypes } from "@/features/document-timeline/types"
import { downloadFileAsset } from "@/features/file-assets/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { displayName } from "@/lib/display-name"
import { compareDecimal } from "@/lib/fixed-decimal"
import { useProductUnitNames } from "@/hooks/use-product-unit-names"
import { hasPermission } from "@/lib/permissions"
import { useRouter } from "@/lib/navigation"
import { cn } from "@/lib/utils"
import { subscribeWorkspaceHotkeyAction } from "@/lib/workspace-hotkeys"
import { navigateWorkspace } from "@/lib/workspace-tabs"
import { workspaceApprovalListFacts } from "../api/task-materials"
import {
    workspaceDocumentName,
    workspaceOpenActionLabel,
    workspaceReadActionLabel,
} from "../api/work-item-meta"
import { useInvoiceApprovalReview } from "../hooks/use-invoice-review"
import { useWorkspaceDetailDocumentTitle } from "../hooks/use-workspace-detail-title"
import { useWorkspaceDocumentFacts } from "../hooks/use-workspace-document-facts"
import { useWorkspaceSourceSalesOrder } from "../hooks/use-workspace-source-sales-order"
import { buildDocumentHref } from "../lib/destination"
import { type DetailSection, splitDetailSections } from "../lib/detail-facts"
import { buildHandlerHref } from "../lib/handler-destination"
import { isApprovalWorkbenchTask } from "../lib/navigation-eligibility"
import { buildWorkspaceReworkTriage } from "../lib/rework-triage"
import { workspacePaperKind } from "../lib/paper-kind"
import {
    linkedDocumentHref,
    linkedDocumentPaperKind,
    withSourceSalesOrder,
} from "../lib/source-sales-order"
import { stripDocumentNumberPrefix } from "../lib/stable-number"
import { isBlockedWorkItem } from "../lib/work-item"
import { workspaceFulfillmentDescriptor } from "../lib/workspace-fulfillment"
import type { WorkspaceWorkItem } from "../types"
import {
    WorkspaceApprovalContextRail,
    type ApprovalContextRailTab,
} from "./workspace-approval-context-rail"
import { WorkspaceApprovalResubmissionBanner } from "./workspace-approval-resubmission-banner"
import { WorkspaceApprovalReworkActions } from "./workspace-approval-rework"
import { WorkspaceDeliveryBrief } from "./workspace-delivery-brief"
import { WorkspaceDocumentBadge } from "./workspace-document-badge"
import {
    WorkspaceDocumentPaperDialog,
    type WorkspacePaperTarget,
} from "./workspace-document-paper-dialog"
import { WorkspaceFulfillmentProcessAction } from "./workspace-fulfillment-task"
import {
    WorkspaceInvoiceApproval,
    type WorkspaceInvoiceApprovalHandle,
} from "./workspace-invoice-approval"
import { buildInvoiceDynamicRejectPhrases } from "./workspace-invoice-approval-model"
import { WorkspaceMoney } from "./workspace-money"
import { WorkspaceProcessingPane } from "./workspace-processing-layout"
import {
    WorkspacePurchaseApproval,
    purchaseApprovalRiskReasons,
    type WorkspacePurchaseApprovalHandle,
} from "./workspace-purchase-approval"
import {
    buildPurchaseDynamicRejectPhrases,
    parsePurchaseExtraSections,
} from "./workspace-purchase-comparison-model"
import { WorkspacePurchasePayment } from "./workspace-purchase-payment"
import { buildPurchaseSourceComparisonModel } from "./workspace-purchase-source-comparison-model"
import type { PurchaseSourceComparisonTarget } from "./workspace-purchase-source-comparison"
import {
    WorkspaceReceiptApproval,
    type WorkspaceReceiptApprovalHandle,
    buildReceiptDynamicRejectPhrases,
} from "./workspace-receipt-approval"
import {
    WorkspaceRejectionNotice,
    useWorkspaceRejectionInfo,
} from "./workspace-rejection-notice"
import { WorkspaceReworkCommandCard } from "./workspace-rework-command-card"
import {
    WorkspaceSalesApproval,
    type WorkspaceSalesApprovalHandle,
    assessSalesApprovalRisk,
} from "./workspace-sales-approval"
import { buildSalesDynamicRejectPhrases } from "../lib/sales-approval-model"
import { WorkspaceTaskHeaderActions } from "./workspace-task-context"
import { WorkspaceCompactTaskHeader } from "./workspace-task-identity-header"
import { WorkspaceTaskResponsibilityActions } from "./workspace-task-responsibility-actions"

const APPROVAL_RAIL_WIDTH_STORAGE_KEY = "erp.workspace.approvalRailWidth"
const APPROVAL_RAIL_COLLAPSED_STORAGE_KEY =
    "erp.workspace.approvalRailCollapsed"

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

const HIGH_RISK_REFUND_THRESHOLD = "100000"

type WorkspaceDetailField = DetailSection & {
    content?: ReactNode
    fullWidth?: boolean
}

function extractUnsignedDecimal(label?: string | null): string | null {
    if (!label) return null
    const cleaned = label.replace(/[¥￥,\s元]/g, "").replace(/^-/, "")
    return /^\d+(?:\.\d+)?$/.test(cleaned) ? cleaned : null
}

export function WorkspaceDocumentTaskDetail({
    item,
    reworkTask,
    canReadSensitive,
    onDecisionApplied,
    onTaskCompleted,
    grantedPermissions = [],
    inspectorMode = false,
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
    /** 列表分栏检查器模式：隐藏独立 2 栏右轨，采用精简单栏排版。 */
    inspectorMode?: boolean
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
            ? instanceId && canReadSubmission
            : currentPaperKind && item.businessObjectId.trim(),
    )
    const salesApproval =
        approvalDocument &&
        ["sales_order", "voucher_sales_order"].includes(item.businessObjectType)
    const purchaseApproval =
        approvalDocument &&
        item.businessObjectType === "purchase_order" &&
        Boolean(instanceId) &&
        canReadSubmission
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
    const structuredDiffDocType =
        approvalDocument &&
        isStructuredDiffDocumentType(item.businessObjectType)
            ? item.businessObjectType
            : null
    const financialDocType =
        approvalDocument &&
        isFinancialApprovalDocumentType(item.businessObjectType)
            ? item.businessObjectType
            : null
    const specializedApproval =
        salesApproval ||
        purchaseApproval ||
        receiptApproval ||
        item.businessObjectType === "sales_invoice_request"
    const readActionLabel = approvalDocument
        ? "查看提交资料"
        : item.workItemType === "FULFILLMENT_OPERATION"
          ? `查看来源${item.businessObjectType === "delivery" ? "销售" : "采购"}单`
          : workspaceReadActionLabel(item.businessObjectType)
    const openActionLabel = workspaceOpenActionLabel(
        item.workItemType,
        item.businessObjectType,
    )
    const router = useRouter()
    const profileQuery = useAccountProfileQuery()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const returnTo = `${pathname}${searchParams.toString() ? `?${searchParams}` : ""}`
    const [paper, setPaper] = useState<WorkspacePaperTarget | null>(null)
    const [railTab, setRailTab] = useState<ApprovalContextRailTab>("trail")
    const [railCollapsed, setRailCollapsedState] = useState(false)
    const [activeMaterialFileId, setActiveMaterialFileId] = useState<
        string | undefined
    >(undefined)
    const [sourceComparisonTarget, setSourceComparisonTarget] =
        useState<PurchaseSourceComparisonTarget>()
    const [reworkAbandonOpen, setReworkAbandonOpen] = useState(false)
    const [reworkInlineEditOpen, setReworkInlineEditOpen] = useState(false)
    const purchaseApprovalHandleRef =
        useRef<WorkspacePurchaseApprovalHandle | null>(null)
    const invoiceApprovalHandleRef =
        useRef<WorkspaceInvoiceApprovalHandle | null>(null)
    const salesApprovalHandleRef = useRef<WorkspaceSalesApprovalHandle | null>(
        null,
    )
    const receiptApprovalHandleRef =
        useRef<WorkspaceReceiptApprovalHandle | null>(null)
    const financialRendererHandleRef =
        useRef<FinancialApprovalRendererHandle | null>(null)
    const structuredDiffHandleRef = useRef<StructuredDiffRendererHandle | null>(
        null,
    )
    const rejectionInfo = useWorkspaceRejectionInfo(item)

    const setRailCollapsed = (
        next: boolean | ((previous: boolean) => boolean),
    ) => {
        setRailCollapsedState((previous) => {
            const resolved = typeof next === "function" ? next(previous) : next
            try {
                window.localStorage.setItem(
                    APPROVAL_RAIL_COLLAPSED_STORAGE_KEY,
                    resolved ? "1" : "0",
                )
            } catch {
                // 隐私模式下忽略本地存储写入失败
            }
            return resolved
        })
    }

    useEffect(() => {
        try {
            if (
                window.localStorage.getItem(
                    APPROVAL_RAIL_COLLAPSED_STORAGE_KEY,
                ) === "1"
            ) {
                setRailCollapsedState(true)
            }
        } catch {
            // 忽略本地存储读取异常
        }
    }, [])

    useEffect(() => {
        setPaper(null)
        setActiveMaterialFileId(undefined)
        setSourceComparisonTarget(undefined)
        setReworkAbandonOpen(false)
        setReworkInlineEditOpen(false)
        setRailTab((current) =>
            current === "source-sales" &&
            item.businessObjectType !== "purchase_order"
                ? "trail"
                : current,
        )
    }, [item.businessObjectType, item.workItemId])

    const documentFacts = useWorkspaceDocumentFacts(item)
    const sourceSales = useWorkspaceSourceSalesOrder(item)
    const shouldReadSubmission =
        canReadSubmission && Boolean(instanceId) && approvalDocument
    const materials = useApprovalMaterials(
        instanceId ?? "",
        shouldReadSubmission,
        item,
    )
    const submission = canReadSubmission ? materials.data : undefined
    const hasSourceSalesLines = Boolean(
        purchaseApproval &&
        submission?.source_sales_orders?.some(
            (order) => order.revision.commercial_lines.length > 0,
        ),
    )
    const { names: sourceSalesUnitNames } = useProductUnitNames(
        [],
        hasSourceSalesLines,
    )
    const purchaseSourceComparisonModel = useMemo(
        () =>
            purchaseApproval && submission
                ? buildPurchaseSourceComparisonModel({
                      orders: submission.source_sales_orders ?? [],
                      snapshots: submission.display.source_sales,
                      purchaseLines: submission.purchase_lines,
                      purchaseFulfillment: submission.purchase_fulfillment,
                      purchaseExtraSections:
                          submission.display.source.extra_sections,
                      extraCostSummary: submission.extra_cost_summary,
                      unitNames: sourceSalesUnitNames,
                  })
                : undefined,
        [purchaseApproval, sourceSalesUnitNames, submission],
    )
    const salesSubmission = submission?.sales_order
    const useStructuredDiffRenderer = Boolean(
        structuredDiffDocType && submission,
    )
    const useFinancialRenderer = Boolean(financialDocType && submission)
    const useInlineGenericSummary =
        approvalDocument &&
        !specializedApproval &&
        !useStructuredDiffRenderer &&
        !useFinancialRenderer &&
        Boolean(submission) &&
        !salesSubmission

    const invoiceReviewQuery = useInvoiceApprovalReview(
        invoiceApproval && instanceId ? instanceId : "",
    )
    const refreshInvoiceOnReturnRef = useRef(() => {})
    refreshInvoiceOnReturnRef.current = () => {
        void invoiceReviewQuery.refetch()
        void materials.refetch()
    }
    useEffect(() => {
        if (!rework || !invoiceApproval || !instanceId) return
        const handleReturn = () => {
            if (document.visibilityState === "hidden") return
            refreshInvoiceOnReturnRef.current()
        }
        window.addEventListener("focus", handleReturn)
        document.addEventListener("visibilitychange", handleReturn)
        return () => {
            window.removeEventListener("focus", handleReturn)
            document.removeEventListener("visibilitychange", handleReturn)
        }
    }, [instanceId, invoiceApproval, rework])

    const downloadSubmittedMaterial = useDownloadApprovalMaterial(
        instanceId ?? "",
    )
    const currentBasisRevisionId =
        submission?.current_sales_basis?.source.revision_id
    const downloadCurrentBasisMaterial = useDownloadApprovalMaterial(
        instanceId ?? "",
        currentBasisRevisionId,
    )

    const dockedMaterialFiles = useMemo<readonly MaterialFileItem[]>(() => {
        if (!instanceId || !submission) return []
        const seen = new Set<string>()
        const items: MaterialFileItem[] = []
        for (const file of submission.attachments) {
            if (seen.has(file.file_asset_id)) continue
            seen.add(file.file_asset_id)
            items.push({
                id: file.file_asset_id,
                name: file.file_name,
                mimeType: file.content_type,
                sizeBytes: file.byte_size,
                loadPreview: () =>
                    fetchApprovalMaterialPreview(
                        instanceId,
                        file.file_asset_id,
                    ),
                onDownload: () => downloadSubmittedMaterial.mutateAsync(file),
            })
        }
        const currentBasis = submission.current_sales_basis
        if (currentBasis) {
            const revId = currentBasis.source.revision_id
            for (const file of currentBasis.attachments) {
                if (seen.has(file.file_asset_id)) continue
                seen.add(file.file_asset_id)
                items.push({
                    id: file.file_asset_id,
                    name: file.file_name,
                    mimeType: file.content_type,
                    sizeBytes: file.byte_size,
                    loadPreview: () =>
                        fetchApprovalMaterialPreview(
                            instanceId,
                            file.file_asset_id,
                            revId,
                        ),
                    onDownload: () =>
                        downloadCurrentBasisMaterial.mutateAsync(file),
                })
            }
        }
        return items
    }, [
        downloadCurrentBasisMaterial,
        downloadSubmittedMaterial,
        instanceId,
        submission,
    ])

    const showDecisionRail = Boolean(
        fullPage && !inspectorMode && approvalDocument && instanceId,
    )
    const railIdPrefix = `workspace-approval-rail-${toAutomationIdSegment(item.workItemId)}`

    useEffect(() => {
        if (!showDecisionRail) return
        return subscribeWorkspaceHotkeyAction((action) => {
            if (action === "toggle-rail") {
                setRailCollapsed((prev) => !prev)
                return
            }
            if (action === "materials") {
                if (railCollapsed || railTab !== "materials") {
                    setRailCollapsed(false)
                    setRailTab("materials")
                    window.requestAnimationFrame(() => {
                        document
                            .getElementById(`${railIdPrefix}-tab-materials`)
                            ?.focus()
                    })
                } else {
                    setRailCollapsed(true)
                }
                return
            }
            if (action === "history") {
                if (railCollapsed || railTab !== "trail") {
                    setRailCollapsed(false)
                    setRailTab("trail")
                    window.requestAnimationFrame(() => {
                        document
                            .getElementById(`${railIdPrefix}-tab-trail`)
                            ?.focus()
                    })
                } else {
                    setRailCollapsed(true)
                }
            }
        })
    }, [railCollapsed, railIdPrefix, railTab, showDecisionRail])

    const handleSelectMaterialFile = (fileId: string) => {
        setActiveMaterialFileId(fileId)
        setRailTab("materials")
        setRailCollapsed(false)
    }

    const handleOpenMaterials = () => {
        setRailTab("materials")
        setRailCollapsed(false)
    }

    const handleCompareSourceSales = (
        target: PurchaseSourceComparisonTarget,
    ) => {
        setSourceComparisonTarget(target)
        setRailTab("source-sales")
        setRailCollapsed(false)
        window.requestAnimationFrame(() => {
            const tab = document.getElementById(
                `${railIdPrefix}-tab-source-sales`,
            )
            tab?.focus({ preventScroll: true })
            tab?.scrollIntoView({ block: "start", inline: "nearest" })
        })
    }

    const routeStepperEnabled = Boolean(
        approvalDocument &&
        instanceId &&
        canReadSubmission &&
        !showDecisionRail,
    )
    const compactStepperHistoryQuery = useApprovalHistoryInfiniteQuery(
        { instanceId: instanceId ?? "", order: "desc" },
        routeStepperEnabled,
    )
    const compactStepperRoute = compactStepperHistoryQuery.data?.pages[0]?.route

    const submittedFacts =
        (rework || purchaseApproval) && submission
            ? workspaceApprovalListFacts({
                  ...submission,
                  ...submission.display,
                  source_sales: submission.display.source_sales ?? [],
              })
            : undefined
    const facts = rework
        ? submittedFacts
        : (submittedFacts ?? documentFacts.facts)
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
              salesSubmission?.customer_name ?? submittedFacts?.counterparty,
              item.businessObjectId,
          )
        : (facts?.counterparty ?? item.counterpartyName)
    const submittedDocumentNumber = displayName(
        submission?.document_no,
        item.businessObjectId,
        item.workItemId,
        instanceId,
    )
    const documentNumber =
        submittedDocumentNumber ??
        (rework ? undefined : stripDocumentNumberPrefix(item.stableNumber))
    const headerTitle = rework
        ? counterpartyName ||
          workspaceDocumentName(item.businessObjectType) ||
          "原单资料"
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
    const [primaryAmount, ...otherAmounts] = salesSubmission
        ? [
              { label: "含税金额", value: `¥${salesSubmission.gross_amount}` },
              { label: "不含税金额", value: `¥${salesSubmission.net_amount}` },
              { label: "税额", value: `¥${salesSubmission.tax_amount}` },
          ]
        : detailFacts.amounts
    const submitter =
        displayName(
            submission?.display.source.submitter_name,
            salesSubmission?.submitted_by,
        ) ?? detailFacts.submitter
    const applicantField = submitter
        ? [{ label: "申请人", value: submitter }]
        : []
    const documentFields: readonly WorkspaceDetailField[] = salesSubmission
        ? [
              {
                  label: "客户",
                  value:
                      displayName(salesSubmission.customer_name) ??
                      displayName(submission?.display.source.customer) ??
                      "未记录客户名称",
              },
              {
                  label: "结算主体",
                  value:
                      displayName(salesSubmission.settlement_party_name) ?? "—",
              },
              {
                  label: "业务性质",
                  value:
                      salesSubmission.business_type === "VOUCHER"
                          ? "卡券"
                          : "实物与服务",
              },
              {
                  label: "项目",
                  value: displayName(salesSubmission.project_name) ?? "—",
              },
              {
                  label: "合同",
                  value: displayName(salesSubmission.contract_no) ?? "无合同",
              },
              {
                  label: "付款条件",
                  value: displayName(salesSubmission.payment_term_name) ?? "—",
              },
              { label: "开票要求", value: salesSubmission.invoice_type || "—" },
              {
                  label: "约定税点",
                  value: approvalSalesTaxPoint(salesSubmission.tax_point),
                  numeric: true,
              },
              {
                  label: "收款到期日",
                  value: salesSubmission.receivable_due_date || "—",
              },
              ...(salesSubmission.business_type === "VOUCHER"
                  ? [
                        {
                            label: "卡券履约期限",
                            value: displayUnixDate(
                                salesSubmission.voucher_expiry_at,
                            ),
                            numeric: true,
                        },
                    ]
                  : []),
              ...applicantField,
              {
                  label: "提交时间",
                  value:
                      displayUnixSeconds(salesSubmission.submitted_at)?.label ??
                      "—",
                  numeric: true,
              },
          ]
        : [
              ...applicantField,
              ...detailFacts.keyFields,
              ...detailFacts.moreFields,
          ]
    const visibleDocumentFields: readonly WorkspaceDetailField[] =
        documentFields.filter(
            ({ label }) =>
                !(
                    item.businessObjectType === "sales_invoice_request" &&
                    label === "申请金额"
                ) &&
                !(invoiceApproval && exactInvoiceFields.has(label)) &&
                !(
                    purchaseApproval &&
                    (label === "供应商" || label === "提交号")
                ) &&
                !(purchaseApproval && submission && label === "来源销售单"),
        )
    const informationFields: readonly WorkspaceDetailField[] =
        visibleDocumentFields
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
    const deliveryBrief =
        item.workItemType === "FULFILLMENT_OPERATION" &&
        item.businessObjectType === "delivery" &&
        item.deliveryLines !== undefined
    const blockers = useMemo(
        () => [
            ...new Set(
                item.actionBlockers
                    .map(({ message }) => message.trim())
                    .filter(Boolean),
            ),
        ],
        [item.actionBlockers],
    )

    const purchaseRiskReasons = useMemo(
        () =>
            purchaseApproval
                ? purchaseApprovalRiskReasons(
                      submission?.purchase_lines,
                      purchaseSourceComparisonModel,
                  )
                : [],
        [
            purchaseApproval,
            purchaseSourceComparisonModel,
            submission?.purchase_lines,
        ],
    )

    const highRiskReasons = useMemo(() => {
        const reasons = [...purchaseRiskReasons]
        if (isReversalDocumentType(item.businessObjectType)) {
            reasons.push(
                "红字冲正单据：审批通过后将冲回原收付款记录并同步调整往来余额。",
            )
        }
        if (salesApproval && salesSubmission) {
            reasons.push(...assessSalesApprovalRisk(salesSubmission).reasons)
        }
        if (invoiceApproval && invoiceReviewQuery.data) {
            const shortage = invoiceReviewQuery.data.amounts.shortage
            if (compareDecimal(shortage, "0", 2) > 0) {
                reasons.push(`当前开票申请超出可开票额度 ¥${shortage}。`)
            }
            for (const warning of invoiceReviewQuery.data.warnings) {
                if (warning.trim()) reasons.push(warning.trim())
            }
        }
        if (
            item.businessObjectType === "customer_refund" ||
            item.businessObjectType === "supplier_refund"
        ) {
            const extraRefundAmount =
                submission?.display.source.extra_sections.find((section) =>
                    [
                        "本次退款金额",
                        "退款金额",
                        "含税金额",
                        "申请金额",
                        "金额",
                    ].includes(section.label.trim()),
                )?.value
            const amountValue = extractUnsignedDecimal(
                item.amountSummary?.value ??
                    submission?.display.source.amount_label ??
                    extraRefundAmount ??
                    primaryAmount?.value,
            )
            if (
                amountValue &&
                compareDecimal(amountValue, HIGH_RISK_REFUND_THRESHOLD, 2) >= 0
            ) {
                reasons.push("大额退款复核：单笔退款金额已达到高危复核阈值。")
            }
        }
        if (blockers.length > 0) {
            reasons.push(...blockers)
        }
        return [...new Set(reasons)]
    }, [
        blockers,
        invoiceApproval,
        invoiceReviewQuery.data,
        item.amountSummary?.value,
        item.businessObjectType,
        primaryAmount?.value,
        purchaseRiskReasons,
        salesApproval,
        salesSubmission,
        submission?.display.source.amount_label,
        submission?.display.source.extra_sections,
    ])

    const dynamicRejectPhrases = useMemo(() => {
        if (salesApproval && salesSubmission) {
            return buildSalesDynamicRejectPhrases(
                salesSubmission,
                submission?.attachments.length,
            )
        }
        if (purchaseApproval && submission?.purchase_lines) {
            return buildPurchaseDynamicRejectPhrases(
                submission.purchase_lines,
                submission.attachments.length,
                purchaseSourceComparisonModel,
            )
        }
        if (invoiceApproval && invoiceReviewQuery.data) {
            return buildInvoiceDynamicRejectPhrases(
                invoiceReviewQuery.data,
                submission ? dockedMaterialFiles.length : undefined,
            )
        }
        if (receiptApproval && submission?.display.source) {
            return buildReceiptDynamicRejectPhrases(
                submission.display.source,
                dockedMaterialFiles.length,
            )
        }
        if (financialDocType && submission?.display.source) {
            return buildFinancialDynamicRejectPhrases(
                financialDocType,
                submission.display.source,
                dockedMaterialFiles.length,
            )
        }
        if (structuredDiffDocType && submission?.display.source) {
            return buildStructuredDiffDynamicRejectPhrases(
                structuredDiffDocType,
                submission.display.source,
            )
        }
        return []
    }, [
        dockedMaterialFiles.length,
        financialDocType,
        invoiceApproval,
        invoiceReviewQuery.data,
        purchaseApproval,
        purchaseSourceComparisonModel,
        receiptApproval,
        salesApproval,
        salesSubmission,
        structuredDiffDocType,
        submission,
    ])

    const reworkTarget = reworkTask ?? item
    const reworkDocType =
        reworkTarget.routeContext?.documentType ??
        reworkTarget.businessObjectType
    const effectivePermissions =
        profileQuery.data?.permissions ?? grantedPermissions
    const supportsReworkInlineEdit = Boolean(
        [
            "customer_receipt",
            "customer_refund",
            "receipt_reversal",
            "supplier_refund",
            "payment_reversal",
            "stock_adjustment",
        ].includes(reworkDocType) ||
        ((reworkDocType === "sales_change_order" ||
            reworkDocType === "purchase_change_order") &&
            Boolean(reworkTarget.rootBusinessObjectId?.trim())) ||
        (reworkDocType === "sales_invoice_request" &&
            hasPermission(
                effectivePermissions,
                "sales_invoice_request:submit",
            ) &&
            (hasPermission(
                effectivePermissions,
                "sales_invoice_request:detail",
            ) ||
                hasPermission(
                    effectivePermissions,
                    "sales_invoice_request:list",
                ))),
    )
    const reworkDestination = buildHandlerHref({
        ...reworkTarget,
        approvalInstanceId: reworkTarget.approvalProcessInstanceId,
    })
    const reworkHref = (() => {
        if (!reworkDestination) return null
        const [destPath, destQuery] = reworkDestination.split("?", 2)
        if (!destPath) return null
        const params = new URLSearchParams(destQuery)
        params.set("returnTo", returnTo)
        return `${destPath}?${params.toString()}`
    })()
    const openReworkDestination = () => {
        if (!reworkHref) return
        if (!navigateWorkspace(reworkHref)) {
            router.push(reworkHref)
        }
    }
    const reworkCanProcess =
        reworkTarget.status === "OPEN" &&
        reworkTarget.allowedActions.includes("PROCESS")
    const reworkCanAbandon =
        reworkTarget.status === "OPEN" &&
        reworkTarget.allowedActions.includes("ABANDON_REWORK")
    const reworkTriage = rework
        ? buildWorkspaceReworkTriage({
              item,
              reworkTask,
              submission,
              invoiceReview: invoiceReviewQuery.data,
              invoiceReviewLoading: invoiceReviewQuery.isPending,
              invoiceReviewError: invoiceReviewQuery.isError,
              attachmentCount: dockedMaterialFiles.length,
              canProcess: reworkCanProcess,
              canAbandon: reworkCanAbandon,
              supportsInlineEdit: supportsReworkInlineEdit,
              hasDocumentHref: Boolean(reworkHref),
              canOpenMaterials: Boolean(
                  showDecisionRail ||
                  item.businessObjectType === "sales_order" ||
                  item.businessObjectType === "voucher_sales_order",
              ),
              refreshing:
                  materials.isFetching ||
                  (invoiceApproval && invoiceReviewQuery.isFetching) ||
                  rejectionInfo.isFetching,
              onRequestAbandon: reworkCanAbandon
                  ? () => setReworkAbandonOpen(true)
                  : undefined,
              onOpenEdit: reworkCanProcess
                  ? () => {
                        if (supportsReworkInlineEdit) {
                            setReworkInlineEditOpen(true)
                        } else {
                            openReworkDestination()
                        }
                    }
                  : undefined,
              onOpenDocument: reworkHref ? openReworkDestination : undefined,
              onOpenMaterials: showDecisionRail
                  ? () => {
                        setRailTab("materials")
                        setRailCollapsed(false)
                    }
                  : item.businessObjectType === "sales_order" ||
                      item.businessObjectType === "voucher_sales_order"
                    ? () => salesApprovalHandleRef.current?.openMaterials()
                    : undefined,
              onRefresh: () => {
                  void materials.refetch()
                  if (invoiceApproval) void invoiceReviewQuery.refetch()
                  rejectionInfo.onRetry()
              },
              onLocateInvoiceRisk: (target) =>
                  invoiceApprovalHandleRef.current?.locateRisk(target),
              onFilterPurchaseRisks: () =>
                  purchaseApprovalHandleRef.current?.filterToRisks(),
              onFilterSalesRisks: () =>
                  salesApprovalHandleRef.current?.filterToRisks(),
              onLocateReceiptRisk: (riskId) =>
                  receiptApprovalHandleRef.current?.locateRisk(riskId),
              onLocateFinancialLines: () =>
                  financialRendererHandleRef.current?.locateLines(),
              onLocateDiffChanges: () =>
                  structuredDiffHandleRef.current?.locateChanges(),
          })
        : null
    const rejectionContext: ReworkRejectionContext | undefined = rework
        ? {
              rejector: rejectionInfo.rejector,
              node: rejectionInfo.node,
              rejectedAtLabel: rejectionInfo.rejectedAt?.label,
              rejectedAtDateTime: rejectionInfo.rejectedAt?.dateTime,
              reason: rejectionInfo.reason,
          }
        : undefined
    const reworkBlocked = Boolean(rework && reworkTriage?.stance === "blocked")

    const actions = reworkTask ? (
        <WorkspaceApprovalReworkActions
            item={reworkTask}
            rejectionContext={rejectionContext}
            reworkBlocked={reworkBlocked}
            abandonVariant={reworkBlocked ? "default" : "outline"}
            abandonOpen={reworkAbandonOpen}
            onAbandonOpenChange={setReworkAbandonOpen}
            inlineEditOpen={reworkInlineEditOpen}
            onInlineEditOpenChange={setReworkInlineEditOpen}
            onTaskCompleted={onTaskCompleted}
        />
    ) : approvalTask && !trackingTask ? (
        <ApprovalActionBar
            presentation={inspectorMode ? "sheet" : "workspace"}
            id={`workspace-task-detail-approval-${toAutomationIdSegment(item.workItemId)}`}
            documentType={item.businessObjectType}
            documentId={item.businessObjectId}
            allowedActions={item.allowedActions}
            recoveryOptions={recoveryQuery.data?.actions ?? []}
            workItemId={item.workItemId}
            expectedTaskVersion={item.taskVersion}
            instance={instance}
            canReadSensitive={canReadSensitive}
            hiddenActions={["OPEN_DOCUMENT", "VIEW"]}
            highRiskReasons={highRiskReasons}
            dynamicRejectPhrases={dynamicRejectPhrases}
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

    const timelineTarget = timelineDocumentTypes.has(item.businessObjectType)
        ? {
              kind: (item.workItemType === "APPROVAL_INSTANCE"
                  ? "approval"
                  : "task") as "approval" | "task",
              id: item.workItemId,
          }
        : undefined

    const usesSalesWorkbench = Boolean(
        salesApproval &&
        instanceId &&
        canReadSubmission &&
        (materials.isPending || salesSubmission),
    )
    const usesPurchaseWorkbench = Boolean(purchaseApproval && instanceId)
    const genericSubmissionPending = Boolean(
        shouldReadSubmission && !specializedApproval && materials.isPending,
    )
    const genericResubmission = useMemo(() => {
        if (
            !submission ||
            !(
                useFinancialRenderer ||
                useStructuredDiffRenderer ||
                useInlineGenericSummary
            )
        ) {
            return null
        }
        const parsed = parsePurchaseExtraSections(
            submission.display.source.extra_sections,
            item,
        ).resubmission
        if (!parsed) return null
        const isResubmitRound =
            (parsed.roundNo ?? 0) >= 2 ||
            Boolean(parsed.resubmissionReason || parsed.diffSummary)
        if (!isResubmitRound) return null
        return useFinancialRenderer ? parsed : { ...parsed, diffs: [] }
    }, [
        item,
        submission,
        useFinancialRenderer,
        useInlineGenericSummary,
        useStructuredDiffRenderer,
    ])
    const genericResubmissionBanner = genericResubmission ? (
        <WorkspaceApprovalResubmissionBanner
            roundNo={genericResubmission.roundNo}
            resubmissionReason={genericResubmission.resubmissionReason}
            diffBaseline={genericResubmission.diffBaseline}
            diffSummary={genericResubmission.diffSummary}
            diffs={genericResubmission.diffs}
            previousRejection={genericResubmission.previousRejection}
        />
    ) : null

    const hideCustomSummaryHeader =
        receiptApproval ||
        Boolean(invoiceApproval && instanceId) ||
        deliveryBrief ||
        genericSubmissionPending ||
        useInlineGenericSummary ||
        useStructuredDiffRenderer ||
        useFinancialRenderer ||
        usesSalesWorkbench ||
        usesPurchaseWorkbench

    const showRejectionAlertBanner = rework
        ? canReadSensitive
        : item.approval?.status === "REJECTED"
    const showRouteStepper = Boolean(
        routeStepperEnabled && compactStepperRoute?.nodes.length,
    )
    const headerActions = (
        <WorkspaceTaskHeaderActions item={headerItem}>
            {showDecisionRail && approvalDocument && submission ? (
                <Button
                    id={`workspace-task-detail-materials-shortcut-${toAutomationIdSegment(item.workItemId)}`}
                    type="button"
                    variant="ghost"
                    size="xs"
                    aria-pressed={!railCollapsed && railTab === "materials"}
                    className={cn(
                        detailWorkspaceStyles.quietButton,
                        dockedMaterialFiles.length === 0 &&
                            "text-warning-soft-foreground hover:text-warning-soft-foreground",
                        !railCollapsed &&
                            railTab === "materials" &&
                            "bg-muted/80 text-foreground",
                    )}
                    onClick={() => {
                        if (!railCollapsed && railTab === "materials") {
                            setRailCollapsed(true)
                        } else {
                            handleOpenMaterials()
                        }
                    }}
                >
                    <PaperclipIcon aria-hidden="true" />
                    <span className="num">
                        {dockedMaterialFiles.length > 0
                            ? `附件 (${dockedMaterialFiles.length})`
                            : "无附件"}
                    </span>
                </Button>
            ) : null}
            <WorkspaceTaskResponsibilityActions
                item={item}
                onCompleted={onTaskCompleted}
            />
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
                                        objectId: item.businessObjectId,
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
                    label={canReadPaper ? openActionLabel : readActionLabel}
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
    )

    const isInvestigationFallback =
        !approvalDocument &&
        (item.workItemType === "INTEGRATION_RESULT_UNKNOWN" ||
            item.workItemType === "BUSINESS_EXCEPTION")

    return (
        <WorkspaceProcessingPane
            archetype={
                approvalDocument
                    ? showDecisionRail
                        ? "decision"
                        : "informational"
                    : isInvestigationFallback
                      ? "investigation"
                      : "action"
            }
            idPrefix={`workspace-document-${toAutomationIdSegment(item.workItemId)}`}
            className="@container/document"
            contextRail={
                showDecisionRail && instanceId
                    ? {
                          collapsible: true,
                          resizable: true,
                          defaultWidthPercent: 28,
                          minWidthPercent: 24,
                          maxWidthPercent: 52,
                          storageKey: APPROVAL_RAIL_WIDTH_STORAGE_KEY,
                          preferredMinWidthPercent:
                              railTab === "materials"
                                  ? 42
                                  : railTab === "source-sales"
                                    ? 38
                                    : undefined,
                          collapsed: railCollapsed,
                          onCollapsedChange: setRailCollapsed,
                          label: "审批进度、轨迹与附件对照",
                          content: (
                              <WorkspaceApprovalContextRail
                                  instanceId={instanceId}
                                  instance={instance}
                                  timelineTarget={timelineTarget}
                                  currentDocument={{
                                      type: item.businessObjectType,
                                      id: item.businessObjectId,
                                  }}
                                  materialFiles={dockedMaterialFiles}
                                  materialsLoading={materials.isPending}
                                  sourceComparison={
                                      purchaseApproval && submission
                                          ? {
                                                orders:
                                                    submission.source_sales_orders ??
                                                    [],
                                                snapshots:
                                                    submission.display
                                                        .source_sales,
                                                purchaseLines:
                                                    submission.purchase_lines,
                                                purchaseFulfillment:
                                                    submission.purchase_fulfillment,
                                                purchaseExtraSections:
                                                    submission.display.source
                                                        .extra_sections,
                                                extraCostSummary:
                                                    submission.extra_cost_summary,
                                                precomputedModel:
                                                    purchaseSourceComparisonModel,
                                                target: sourceComparisonTarget,
                                                onTargetChange:
                                                    setSourceComparisonTarget,
                                                onPreview: (revisionId) =>
                                                    setPaper({
                                                        kind: "approval_snapshot",
                                                        objectId: instanceId,
                                                        materialsView:
                                                            "source-sales",
                                                        sourceRevisionId:
                                                            revisionId,
                                                    }),
                                                onLocatePurchaseLine: (
                                                    purchaseLineId,
                                                ) =>
                                                    purchaseApprovalHandleRef.current?.locateLine(
                                                        purchaseLineId,
                                                    ),
                                            }
                                          : undefined
                                  }
                                  isRework={rework}
                                  activeTab={railTab}
                                  onActiveTabChange={setRailTab}
                                  activeFileId={activeMaterialFileId}
                                  onActiveFileChange={setActiveMaterialFileId}
                                  idPrefix={`workspace-approval-rail-${toAutomationIdSegment(item.workItemId)}`}
                              />
                          ),
                      }
                    : undefined
            }
            header={
                inspectorMode && showRouteStepper ? null : (
                    <WorkspaceCompactTaskHeader
                        title={canReadSensitive ? headerTitle : "业务详情"}
                        primaryStatus={{
                            label:
                                rework && item.status === "OPEN"
                                    ? item.workItemTypeLabel
                                    : item.statusLabel,
                            tone: item.statusTone,
                        }}
                        badges={<WorkspaceDocumentBadge item={item} />}
                        documentNumber={documentNumber}
                        meta={
                            <DetailWorkspaceMeta
                                items={[
                                    rework && submission ? (
                                        <span
                                            key="number"
                                            className="num select-all"
                                            data-workspace-document-number={
                                                documentNumber || undefined
                                            }
                                        >
                                            单号：{documentNumber || "未提供"}
                                        </span>
                                    ) : !rework && documentNumber ? (
                                        <span
                                            key="number"
                                            className="num select-all"
                                            data-workspace-document-number={
                                                documentNumber
                                            }
                                        >
                                            {documentNumber}
                                        </span>
                                    ) : null,
                                    submission
                                        ? `提交版本 ${submission.subject_version}`
                                        : null,
                                    fullPage && item.dueAt
                                        ? `截止：${item.dueAtLabel}`
                                        : null,
                                ]}
                            />
                        }
                        actions={headerActions}
                    />
                )
            }
            headerSupplement={
                showRouteStepper && compactStepperRoute ? (
                    <ApprovalRouteStepper
                        route={compactStepperRoute}
                        instanceStatus={instance?.status}
                        latestRejection={instance?.latestRejection}
                        latestRejectionByName={instance?.latestRejectionByName}
                        hideRejectionBanner={
                            showRejectionAlertBanner ||
                            usesSalesWorkbench ||
                            usesPurchaseWorkbench ||
                            invoiceApproval ||
                            receiptApproval ||
                            Boolean(genericResubmission)
                        }
                        trailingActions={
                            inspectorMode ? headerActions : undefined
                        }
                        idPrefix={`workspace-compact-stepper-${toAutomationIdSegment(item.workItemId)}`}
                    />
                ) : null
            }
            alertBanner={
                rework && canReadSensitive && reworkTriage ? (
                    <WorkspaceReworkCommandCard
                        idPrefix={`workspace-rework-command-${toAutomationIdSegment(item.workItemId)}`}
                        rejection={rejectionInfo}
                        {...reworkTriage}
                    />
                ) : !rework && showRejectionAlertBanner ? (
                    <WorkspaceRejectionNotice
                        item={item}
                        actionable={Boolean(
                            reworkTask?.allowedActions.includes("PROCESS"),
                        )}
                    />
                ) : null
            }
            footer={actions}
            aria-label="当前任务"
        >
            {!rework && blockers.length > 0 ? (
                <div className="pt-4">
                    <Alert variant="warning">
                        <AlertTitle>当前无法继续办理</AlertTitle>
                        <AlertDescription>
                            {blockers.length === 1 ? (
                                blockers[0]
                            ) : (
                                <ul className="list-disc space-y-1 pl-4">
                                    {blockers.map((message) => (
                                        <li key={message}>{message}</li>
                                    ))}
                                </ul>
                            )}
                        </AlertDescription>
                    </Alert>
                </div>
            ) : null}
            <div className="@container/delivery-brief flex w-full min-w-0 flex-col">
                {rework && (!instanceId || !canReadSubmission) ? (
                    <DetailBlock>
                        <p
                            className="text-xs text-muted-foreground"
                            role="status"
                        >
                            {canReadSubmission
                                ? "未找到本次驳回关联的提交资料，请刷新任务后重试。"
                                : "当前账号无权查看本次提交资料。"}
                        </p>
                    </DetailBlock>
                ) : shouldReadSubmission &&
                  !specializedApproval &&
                  materials.isPending ? (
                    <DetailBlock>
                        <p
                            className="text-xs text-muted-foreground"
                            role="status"
                        >
                            正在读取本次提交资料…
                        </p>
                    </DetailBlock>
                ) : shouldReadSubmission &&
                  !specializedApproval &&
                  materials.isError ? (
                    <BusinessFailureState
                        id={`workspace-rework-materials-${toAutomationIdSegment(item.workItemId)}`}
                        title="提交资料读取失败"
                        error={materials.error}
                        onRetry={() => void materials.refetch()}
                        className="my-4"
                    />
                ) : null}
                {!salesSubmission &&
                !purchaseApproval &&
                !useInlineGenericSummary &&
                !useStructuredDiffRenderer &&
                !useFinancialRenderer &&
                item.documentSummaryResolved &&
                !item.summarySections?.length &&
                !item.briefLines?.length ? (
                    <p
                        className="py-4 text-xs text-muted-foreground"
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
                        className="my-4"
                    />
                ) : null}

                {documentFacts.isError ? (
                    <BusinessFailureState
                        id={`workspace-materials-${toAutomationIdSegment(item.workItemId)}`}
                        error={documentFacts.error}
                        onRetry={() => void documentFacts.refetch()}
                        className="my-4"
                    />
                ) : null}

                {receiptApproval && instanceId ? (
                    <WorkspaceReceiptApproval
                        key={instanceId}
                        item={item}
                        instanceId={instanceId}
                        onSelectFile={
                            showDecisionRail
                                ? handleSelectMaterialFile
                                : undefined
                        }
                        onOpenMaterials={
                            showDecisionRail ? handleOpenMaterials : undefined
                        }
                        activeFileId={activeMaterialFileId}
                        hideEmbeddedMaterials={showDecisionRail}
                        hideTopBanners={rework}
                        approvalHandleRef={receiptApprovalHandleRef}
                    />
                ) : null}
                {deliveryBrief ? <WorkspaceDeliveryBrief item={item} /> : null}
                {!hideCustomSummaryHeader && primaryAmount ? (
                    <DetailBlock compact>
                        <div className="flex flex-wrap items-start gap-x-8 gap-y-2.5 py-0.5">
                            <div className="flex flex-col gap-0.5">
                                <span className="text-xs text-muted-foreground">
                                    {primaryAmount.label}
                                </span>
                                <span className="num text-value-summary font-semibold tracking-tight text-foreground">
                                    <WorkspaceMoney
                                        value={primaryAmount.value}
                                    />
                                </span>
                            </div>
                            {otherAmounts.map((amount) => (
                                <div
                                    key={amount.label}
                                    className="flex flex-col gap-0.5 border-l border-border/60 pl-6"
                                >
                                    <span className="text-xs text-muted-foreground">
                                        {amount.label}
                                    </span>
                                    <span
                                        className={cn(
                                            "num text-value-compact font-semibold tracking-tight",
                                            taxAmountToneClass(amount.label),
                                        )}
                                    >
                                        <WorkspaceMoney value={amount.value} />
                                    </span>
                                </div>
                            ))}
                        </div>
                    </DetailBlock>
                ) : !hideCustomSummaryHeader && documentFacts.isPending ? (
                    <DetailBlock title="金额">
                        <p className="text-xs text-muted-foreground">
                            正在加载单据详情…
                        </p>
                    </DetailBlock>
                ) : null}

                {useStructuredDiffRenderer &&
                structuredDiffDocType &&
                submission &&
                instanceId ? (
                    <>
                        {!rework && genericResubmissionBanner ? (
                            <div className="pt-3">
                                {genericResubmissionBanner}
                            </div>
                        ) : null}
                        <StructuredDiffApprovalRenderer
                            documentType={structuredDiffDocType}
                            source={submission.display.source}
                            submitterName={submitter}
                            impactSummary={submission.display.impact_summary}
                            documentNo={documentNumber}
                            counterpartyLabel={counterpartyName}
                            hideRiskStrip={rework}
                            rendererHandleRef={structuredDiffHandleRef}
                        />
                        {!showDecisionRail ? (
                            <DetailBlock
                                title="合同、凭证及附件"
                                description={
                                    submission.attachments.length
                                        ? `共 ${submission.attachments.length} 个文件`
                                        : undefined
                                }
                            >
                                <ApprovalMaterialFiles
                                    instanceId={instanceId}
                                    attachments={submission.attachments}
                                    idPrefix={`workspace-diff-material-${toAutomationIdSegment(item.workItemId)}`}
                                    emptyMessage="本次提交未附带合同、凭证或其他附件。"
                                />
                            </DetailBlock>
                        ) : null}
                    </>
                ) : useFinancialRenderer &&
                  financialDocType &&
                  submission &&
                  instanceId ? (
                    <>
                        <FinancialApprovalRenderer
                            documentType={financialDocType}
                            source={submission.display.source}
                            submitterName={submitter}
                            impactSummary={submission.display.impact_summary}
                            documentNo={documentNumber}
                            counterpartyLabel={counterpartyName}
                            instanceId={instanceId}
                            resubmission={
                                rework ? null : genericResubmissionBanner
                            }
                            hideRiskStrip={rework}
                            rendererHandleRef={financialRendererHandleRef}
                        />
                        {!showDecisionRail ? (
                            <DetailBlock
                                title="合同、凭证及附件"
                                description={
                                    submission.attachments.length
                                        ? `共 ${submission.attachments.length} 个文件`
                                        : undefined
                                }
                            >
                                <ApprovalMaterialFiles
                                    instanceId={instanceId}
                                    attachments={submission.attachments}
                                    idPrefix={`workspace-financial-material-${toAutomationIdSegment(item.workItemId)}`}
                                    emptyMessage="本次提交未附带合同、凭证或其他附件。"
                                />
                            </DetailBlock>
                        ) : null}
                    </>
                ) : useInlineGenericSummary && submission && instanceId ? (
                    <>
                        {!rework && genericResubmissionBanner ? (
                            <div className="pt-3">
                                {genericResubmissionBanner}
                            </div>
                        ) : null}
                        <DetailBlock>
                            <div className="space-y-6">
                                <ApprovalSubmittedSummary
                                    source={submission.display.source}
                                    submitterName={submitter}
                                    impactSummary={
                                        submission.display.impact_summary
                                    }
                                />
                            </div>
                        </DetailBlock>
                        {!showDecisionRail ? (
                            <DetailBlock
                                title="合同、凭证及附件"
                                description={
                                    submission.attachments.length
                                        ? `共 ${submission.attachments.length} 个文件`
                                        : undefined
                                }
                            >
                                <ApprovalMaterialFiles
                                    instanceId={instanceId}
                                    attachments={submission.attachments}
                                    idPrefix={`workspace-inline-material-${toAutomationIdSegment(item.workItemId)}`}
                                    emptyMessage="本次提交未附带合同、凭证或其他附件。"
                                />
                            </DetailBlock>
                        ) : null}
                    </>
                ) : (
                    <>
                        {!receiptApproval &&
                        !invoiceApproval &&
                        !deliveryBrief &&
                        !genericSubmissionPending &&
                        !usesSalesWorkbench &&
                        !usesPurchaseWorkbench &&
                        informationFields.length > 0 ? (
                            <DetailBlock
                                title={
                                    purchaseApproval
                                        ? "采购单信息"
                                        : salesSubmission
                                          ? "销售单信息"
                                          : undefined
                                }
                                width={
                                    purchaseApproval || salesSubmission
                                        ? "full"
                                        : "form"
                                }
                            >
                                <FieldGrid
                                    sections={informationFields}
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
                        onSelectFile={
                            showDecisionRail
                                ? handleSelectMaterialFile
                                : undefined
                        }
                        onOpenMaterials={
                            showDecisionRail ? handleOpenMaterials : undefined
                        }
                        activeFileId={activeMaterialFileId}
                        hideEmbeddedMaterials={showDecisionRail}
                        hideTopBanners={rework}
                        approvalHandleRef={invoiceApprovalHandleRef}
                    />
                ) : null}

                {salesApproval && instanceId && canReadSubmission ? (
                    <WorkspaceSalesApproval
                        item={item}
                        key={instanceId}
                        instanceId={instanceId}
                        onSelectFile={
                            showDecisionRail
                                ? handleSelectMaterialFile
                                : undefined
                        }
                        onOpenMaterials={
                            showDecisionRail ? handleOpenMaterials : undefined
                        }
                        activeFileId={activeMaterialFileId}
                        hideEmbeddedMaterials={showDecisionRail}
                        hideTopBanners={rework}
                        approvalHandleRef={salesApprovalHandleRef}
                    />
                ) : null}

                {purchaseApproval && instanceId ? (
                    <WorkspacePurchaseApproval
                        key={instanceId}
                        instanceId={instanceId}
                        item={item}
                        materials={materials}
                        sourceComparison={purchaseSourceComparisonModel}
                        riskReasons={purchaseRiskReasons}
                        onPreview={setPaper}
                        onSelectFile={
                            showDecisionRail
                                ? handleSelectMaterialFile
                                : undefined
                        }
                        onOpenMaterials={
                            showDecisionRail ? handleOpenMaterials : undefined
                        }
                        onCompareSource={
                            showDecisionRail && submission
                                ? handleCompareSourceSales
                                : undefined
                        }
                        approvalHandleRef={purchaseApprovalHandleRef}
                        activeFileId={activeMaterialFileId}
                        hideEmbeddedMaterials={showDecisionRail}
                        hideTopBanners={rework}
                    />
                ) : null}

                {!invoiceApproval &&
                !genericSubmissionPending &&
                !useInlineGenericSummary &&
                !useStructuredDiffRenderer &&
                !useFinancialRenderer &&
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
                        <ul className="flex flex-col text-body-compact">
                            {briefLines.map((line, lineIndex) => (
                                <li
                                    key={`${lineIndex}:${line.title}`}
                                    className="grid gap-1 border-b border-border/40 py-2.5 last:border-b-0 @[480px]/document:grid-cols-[minmax(0,1fr)_auto] @[480px]/document:gap-x-4"
                                >
                                    <span className="min-w-0">
                                        {line.title}
                                    </span>
                                    {line.quantity || line.dueLabel ? (
                                        <span className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-muted-foreground @[480px]/document:max-w-md @[480px]/document:justify-end @[480px]/document:text-right">
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
                        {/* 销售审批摘要停用「预览完整销售单」入口。 */}
                        {approvalDocument && !salesApproval && instanceId ? (
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
                                        ? `此处展示前 ${briefLines.length} 行，共 ${lineCount} 行。`
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
            </div>
            <WorkspaceDocumentPaperDialog
                reference={item}
                target={paper}
                open={Boolean(paper)}
                onOpenChange={(open) => {
                    if (!open) setPaper(null)
                }}
            />
        </WorkspaceProcessingPane>
    )
}

/** 作业面内的带标题区块。分区线用来拉开层次，内容默认全部展开。 */
function DetailBlock({
    title,
    description,
    children,
    compact = false,
    width = "full",
}: {
    title?: string
    compact?: boolean
    width?: "form" | "full"
    description?: string
    children: ReactNode
}) {
    return (
        <DetailWorkspaceSection
            title={title}
            width={width}
            subtitle={description}
            className={compact ? "py-4 first:pt-4 last:pb-4" : undefined}
        >
            {children}
        </DetailWorkspaceSection>
    )
}

/** 单据字段两列键值表。关联单据可预览或跳转。 */
function FieldGrid({
    sections,
    returnTo,
    onPreview,
}: {
    sections: readonly WorkspaceDetailField[]
    returnTo: string
    onPreview: (section: DetailSection) => void
}) {
    if (sections.length === 0) return null
    return (
        <DescriptionList
            columns="one"
            className="gap-y-2 @min-[640px]/document:grid-cols-2"
        >
            {sections.map((section) => (
                <DescriptionItem
                    key={section.label}
                    className={cn(
                        "grid grid-cols-[6rem_minmax(0,1fr)] items-baseline gap-3 space-y-0",
                        section.fullWidth && "col-span-full",
                    )}
                >
                    <DescriptionTerm className="font-normal">
                        {section.label}
                    </DescriptionTerm>
                    <DescriptionDetails
                        className={cn(section.numeric && "num")}
                        title={section.value || undefined}
                    >
                        {section.content ??
                            (section.objectId ? (
                                <LinkedDocumentValue
                                    section={section}
                                    returnTo={returnTo}
                                    onPreview={onPreview}
                                />
                            ) : (
                                section.value
                            ))}
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
