"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { FileTextIcon, PanelRightCloseIcon, XIcon } from "lucide-react"
import {
    BusinessFailureState,
    DetailWorkspaceMeta,
    PaperDocumentViewport,
    useWorkspaceInspectorMode,
    useWorkspaceTaskPage,
    WorkspaceProcessingPane,
} from "@/components/business"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { TaskTimeline } from "@/features/document-timeline/components/task-timeline"
import { timelineDocumentTypes } from "@/features/document-timeline/types"
import type { FulfillmentQueueFilters } from "@/features/fulfillment-operations/api"
import { FULFILLMENT_ROLES } from "@/features/fulfillment-operations/lib/fulfillment-roles"
import { OPERATION_TYPE_LABEL } from "@/features/fulfillment-operations/types"
import { FulfillmentOperationsWorkspace } from "@/features/fulfillment-operations/pages/components/fulfillment-operations-workspace"
import { FulfillmentPageStates } from "@/features/fulfillment-operations/pages/components/fulfillment-page-states"
import { useFulfillmentOperationsController } from "@/features/fulfillment-operations/pages/hooks/use-fulfillment-operations-controller"
import { PurchaseOrderPaperDocument } from "@/features/purchase-orders/components/purchase-order-paper-document"
import { usePurchaseOrderCenterQuery } from "@/features/purchase-orders/hooks/queries"
import { SalesOrderPaperDocument } from "@/features/sales-orders/components/sales-order-paper-dialog"
import { useSalesOrderDetailQuery } from "@/features/sales-orders/hooks/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { displayName } from "@/lib/display-name"
import { displayText } from "@/features/fulfillment-operations/lib/readable-label"
import { hasPermission } from "@/lib/permissions"
import { buildWorkspaceDetailHref } from "@/lib/workspace-detail-href"
import { subscribeWorkspaceHotkeyAction } from "@/lib/workspace-hotkeys"
import { navigateWorkspace } from "@/lib/workspace-tabs"

import { fulfillmentTaskTitle } from "../lib/fulfillment-title"
import { stripDocumentNumberPrefix } from "../lib/stable-number"
import { workspaceFulfillmentDescriptor } from "../lib/workspace-fulfillment"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceDeliveryGate } from "./workspace-delivery-brief"
import { fulfillmentPaymentStatus } from "./workspace-fulfillment-badges"
import { WorkspaceDocumentPaperDialog } from "./workspace-document-paper-dialog"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

type WorkspaceFulfillmentTaskProps = Readonly<{
    item: WorkspaceWorkItem
    grantedPermissions: readonly string[]
    onTaskCompleted: (workItemId: string) => void
}>

/** W01 履约作业面：任务身份锁定一个强类型收货或发货操作。 */
export function WorkspaceFulfillmentTask({
    item,
    grantedPermissions,
    onTaskCompleted,
}: WorkspaceFulfillmentTaskProps) {
    const inspectorMode = useWorkspaceInspectorMode()
    const fullPage = useWorkspaceTaskPage()
    const surfaceRef = React.useRef<HTMLDivElement>(null)
    const [wide, setWide] = React.useState(false)
    const [reference, setReference] = React.useState<{
        workItemId: string
        mode: "rail" | "dialog" | "inline"
    } | null>(null)
    React.useEffect(() => {
        const surface = surfaceRef.current
        if (!surface) return
        const target = surface.closest<HTMLElement>("#main-content") ?? surface
        const media = window.matchMedia("(min-width: 960px)")
        const sync = () => setWide(media.matches && target.clientWidth >= 960)
        sync()
        media.addEventListener("change", sync)
        const observer =
            typeof ResizeObserver !== "undefined"
                ? new ResizeObserver(sync)
                : null
        observer?.observe(target)
        return () => {
            media.removeEventListener("change", sync)
            observer?.disconnect()
        }
    }, [])

    const descriptor = workspaceFulfillmentDescriptor(item)
    const operationType = descriptor?.operationTypes[0]
    const filters = React.useMemo<FulfillmentQueueFilters>(
        () => ({
            role: descriptor?.role ?? "sales_order",
            operationTypes: operationType ? [operationType] : [],
            operationId: item.businessObjectId,
            taskSourceVersion: item.subjectVersion,
            currentOperationId: item.businessObjectId,
        }),
        [
            descriptor?.role,
            item.businessObjectId,
            item.subjectVersion,
            operationType,
        ],
    )
    const controller = useFulfillmentOperationsController({
        roleValue: filters.role,
        filters,
        lane: null,
        autoNextExplicit: "0",
        stateMode: "local",
        grantedPermissions,
        permissionsReady: true,
        executionAuthorized: item.allowedActions.includes("PROCESS"),
        onOperationCompleted: (operationId) => {
            if (operationId === item.businessObjectId) {
                onTaskCompleted(item.workItemId)
            }
        },
    })
    const operation = controller.operation
    const id = `workspace-fulfillment-${toAutomationIdSegment(item.workItemId)}`
    const fields = item.summarySections ?? []
    const field = (label: string) =>
        fields.find((entry) => entry.label === label)
    const purchaseSource = field("来源采购单") ?? field("关联采购单")
    const purchaseId =
        operation?.source.purchaseOrderId || purchaseSource?.objectId
    const salesId =
        operation?.source.salesOrderId || field("来源销售单")?.objectId
    const purchaseNo = displayText(
        operation?.source.purchaseNo || purchaseSource?.value,
        purchaseId,
    )
    const salesNo = displayText(
        operation?.source.salesOrderNo || field("来源销售单")?.value,
        salesId,
    )
    const purchase =
        purchaseId && hasPermission(grantedPermissions, "purchase_order:detail")
            ? {
                  kind: "purchase_order" as const,
                  objectId: purchaseId,
                  title: purchaseNo,
              }
            : null
    const sales =
        salesId && hasPermission(grantedPermissions, "sales_order:detail")
            ? {
                  kind: "sales_order" as const,
                  objectId: salesId,
                  title: salesNo,
              }
            : null
    const prefersPurchase =
        operationType === "RECEIPT" || descriptor?.role === "procurement"
    const target = prefersPurchase ? (purchase ?? sales) : (sales ?? purchase)
    const showRail = wide && !inspectorMode
    const referenceOpen = reference?.workItemId === item.workItemId
    const railOpen = referenceOpen && showRail && reference.mode === "rail"
    const inlineOpen = referenceOpen && !showRail && reference.mode === "inline"
    const dialogOpen = referenceOpen && !showRail && reference.mode === "dialog"
    const hasTarget = Boolean(target)

    React.useEffect(() => {
        if (inspectorMode || !hasTarget) return
        return subscribeWorkspaceHotkeyAction((action) => {
            if (action !== "toggle-rail") return
            setReference((current) =>
                current?.workItemId === item.workItemId
                    ? null
                    : {
                          workItemId: item.workItemId,
                          mode: showRail ? "rail" : "dialog",
                      },
            )
        })
    }, [hasTarget, inspectorMode, item.workItemId, showRail])

    const typeLabel = operationType ? OPERATION_TYPE_LABEL[operationType] : ""
    const title =
        displayName(operation?.source.customerLabel) ||
        displayName(item.counterpartyName) ||
        typeLabel ||
        fulfillmentTaskTitle(item)
    const stableNo = displayText(
        stripDocumentNumberPrefix(item.stableNumber),
        item.businessObjectId,
    )
    const batchNo = displayText(field("履约批次")?.value, item.businessObjectId)
    const numbers = [
        ...new Set([
            batchNo !== "未登记" && batchNo ? batchNo : stableNo,
            prefersPurchase ? purchaseNo || salesNo : salesNo || purchaseNo,
        ]),
    ].filter((value) => value && value !== title && value !== typeLabel)
    const blockers = item.actionBlockers
        .filter(({ action }) => action === "PROCESS" || action === "POST")
        .map(({ message }) => message)
    const impact = field("供给影响提示")?.value
    const commitment = field("最早客户交付日") ?? field("最早预计交付日")
    const owner = field("当前采购负责人")?.value
    const amount =
        item.amountSummary ??
        (target?.kind === "purchase_order"
            ? field("来源采购单金额")
            : target?.kind === "sales_order"
              ? field("来源销售单金额")
              : undefined)
    const toggleActive = railOpen || inlineOpen

    return (
        <div ref={surfaceRef} className="flex min-h-0 min-w-0 flex-1 flex-col">
            <WorkspaceProcessingPane
                archetype="action"
                className="@container/document"
                idPrefix={id}
                contextRail={
                    target && showRail
                        ? {
                              collapsible: true,
                              resizable: true,
                              allowOnFullWidth: true,
                              defaultWidthPercent: 38,
                              minWidthPercent: 26,
                              maxWidthPercent: 52,
                              storageKey:
                                  "workspace-fulfillment-reference-width",
                              collapsed: !railOpen,
                              onCollapsedChange: (collapsed) =>
                                  setReference(
                                      collapsed
                                          ? null
                                          : {
                                                workItemId: item.workItemId,
                                                mode: "rail",
                                            },
                                  ),
                              label: "对照来源单据",
                              content: railOpen ? (
                                  <FulfillmentReferenceRail
                                      id={id}
                                      kind={target.kind}
                                      objectId={target.objectId}
                                      title={target.title}
                                      onClose={() => setReference(null)}
                                  />
                              ) : null,
                          }
                        : undefined
                }
                header={
                    <WorkspaceTaskIdentityHeader
                        item={item}
                        compact={!fullPage}
                        title={title}
                        subtitle={
                            <DetailWorkspaceMeta
                                items={[
                                    typeLabel !== title && typeLabel,
                                    ...numbers.map((number, index) => (
                                        <span
                                            key={number}
                                            className="num select-all"
                                            data-workspace-document-number={
                                                index === 0 ? number : undefined
                                            }
                                        >
                                            {number}
                                        </span>
                                    )),
                                    item.dueAt && item.dueAtLabel ? (
                                        <span key="deadline" className="num">
                                            截止 {item.dueAtLabel}
                                        </span>
                                    ) : null,
                                ]}
                            />
                        }
                    >
                        {target ? (
                            <Button
                                id={`${id}-source-toggle`}
                                type="button"
                                variant={toggleActive ? "secondary" : "ghost"}
                                size="sm"
                                aria-pressed={
                                    showRail || inlineOpen
                                        ? toggleActive
                                        : undefined
                                }
                                aria-haspopup={
                                    showRail || inlineOpen
                                        ? undefined
                                        : "dialog"
                                }
                                onClick={(event) => {
                                    if (toggleActive) {
                                        setReference(null)
                                        return
                                    }
                                    const inSheet = Boolean(
                                        event.currentTarget.closest(
                                            '[data-business-component="task-action-sheet"]',
                                        ),
                                    )
                                    setReference({
                                        workItemId: item.workItemId,
                                        mode: showRail
                                            ? "rail"
                                            : inSheet
                                              ? "inline"
                                              : "dialog",
                                    })
                                }}
                            >
                                {toggleActive ? (
                                    <PanelRightCloseIcon
                                        aria-hidden="true"
                                        data-icon="inline-start"
                                    />
                                ) : (
                                    <FileTextIcon
                                        aria-hidden="true"
                                        data-icon="inline-start"
                                    />
                                )}
                                {toggleActive ? "收起来源单据" : "对照来源单据"}
                            </Button>
                        ) : null}
                        {fullPage &&
                        showRail &&
                        target &&
                        timelineDocumentTypes.has(item.businessObjectType) ? (
                            <TaskTimeline
                                key={item.workItemId}
                                taskId={item.workItemId}
                                approval={Boolean(item.approval)}
                                instance={false}
                                title={title}
                                document={{
                                    type: item.businessObjectType,
                                    id: item.businessObjectId,
                                    number: item.stableNumber,
                                }}
                            />
                        ) : null}
                    </WorkspaceTaskIdentityHeader>
                }
                aria-label="当前履约任务"
            >
                {inlineOpen && target ? (
                    <div className="border-b border-border/60 pb-3">
                        <FulfillmentReferenceRail
                            id={id}
                            kind={target.kind}
                            objectId={target.objectId}
                            title={target.title}
                            onClose={() => setReference(null)}
                        />
                    </div>
                ) : null}
                {!descriptor ? (
                    <Alert variant="destructive">
                        <AlertTitle>待办关联的单据信息异常</AlertTitle>
                        <AlertDescription>
                            当前待办与关联业务单据不匹配，请刷新页面后重试，或联系管理员处理。
                        </AlertDescription>
                    </Alert>
                ) : controller.queueQuery.isPending ||
                  controller.queueQuery.isError ? (
                    <FulfillmentPageStates
                        status={
                            controller.queueQuery.isPending
                                ? "pending"
                                : "error"
                        }
                        standalone
                        embedded
                        headerDescription="履约处理"
                        error={controller.queueQuery.error}
                        onRetry={() => void controller.queueQuery.refetch()}
                    />
                ) : (
                    <div className="@container/fulfillment-task flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto overscroll-contain">
                        <FulfillmentOperationsWorkspace
                            controller={controller}
                            headerDescription="当前任务"
                            operationTypes={[operationType!]}
                            roleLabel={
                                controller.context?.roleLabel ??
                                FULFILLMENT_ROLES[descriptor.role].label
                            }
                            embedded
                            singleOperation
                            taskContext={{
                                id,
                                blockers,
                                impact,
                                commitment,
                                owner,
                                headerNumbers: numbers,
                                paymentStatus: fulfillmentPaymentStatus(item),
                                amount,
                            }}
                            onBack={() => undefined}
                        />
                    </div>
                )}
                {target && !showRail ? (
                    <WorkspaceDocumentPaperDialog
                        target={target}
                        open={dialogOpen}
                        onOpenChange={(open) => {
                            if (!open) setReference(null)
                        }}
                        closeButtonId={`${id}-source-dialog-close`}
                    />
                ) : null}
            </WorkspaceProcessingPane>
        </div>
    )
}

function FulfillmentReferenceRail({
    id,
    kind,
    objectId,
    title,
    onClose,
}: {
    id: string
    kind: "purchase_order" | "sales_order"
    objectId: string
    title: string
    onClose: () => void
}) {
    const purchase = usePurchaseOrderCenterQuery(
        kind === "purchase_order" ? objectId : "",
    )
    const sales = useSalesOrderDetailQuery(
        kind === "sales_order" ? objectId : "",
    )
    const query = kind === "purchase_order" ? purchase : sales

    return (
        <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col bg-card">
            <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border/60 py-2.5 pr-1">
                <div className="min-w-0">
                    <h2 className="truncate text-body-compact font-semibold text-foreground">
                        {kind === "purchase_order"
                            ? "来源采购单对照"
                            : "来源销售单对照"}
                    </h2>
                    {title ? (
                        <p className="num truncate text-xs text-muted-foreground select-all">
                            {title}
                        </p>
                    ) : null}
                </div>
                <Button
                    id={`${id}-source-rail-close`}
                    type="button"
                    size="icon-sm"
                    variant="ghost"
                    aria-label="收起来源单据对照栏"
                    onClick={onClose}
                >
                    <XIcon aria-hidden="true" />
                </Button>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain py-3">
                {query.isPending ? (
                    <div className="flex min-h-48 items-center justify-center py-10">
                        <p className="flex items-center gap-2 text-xs text-muted-foreground">
                            <Spinner aria-hidden="true" /> 正在读取来源单据…
                        </p>
                    </div>
                ) : query.isError ? (
                    <BusinessFailureState
                        id={`${id}-source-error`}
                        title="来源单据读取失败"
                        error={query.error}
                        onRetry={() => void query.refetch()}
                    />
                ) : query.data ? (
                    <PaperDocumentViewport
                        id={`${id}-source-paper`}
                        fitMode="width"
                        fitKey={objectId}
                        className="bg-transparent p-0"
                        role="region"
                        aria-label="来源单据纸面"
                        tabIndex={0}
                    >
                        {kind === "purchase_order" && purchase.data ? (
                            <PurchaseOrderPaperDocument order={purchase.data} />
                        ) : sales.data ? (
                            <SalesOrderPaperDocument order={sales.data} />
                        ) : null}
                    </PaperDocumentViewport>
                ) : (
                    <p className="py-10 text-center text-xs text-muted-foreground">
                        来源单据暂不可用
                    </p>
                )}
            </div>
        </div>
    )
}

/**
 * 履约直达入口：取代原有 max-w-5xl 弹窗套娃，直接跳转至履约主作业面。
 */
export function WorkspaceFulfillmentProcessAction(
    props: WorkspaceFulfillmentTaskProps,
) {
    const router = useRouter()
    const operationType = workspaceFulfillmentDescriptor(props.item)
        ?.operationTypes[0]
    const isDelivery =
        operationType === "SUPPLIER_DIRECT" ||
        operationType === "WAREHOUSE_SHIP"
    if (!props.item.allowedActions.includes("PROCESS")) return null
    const actionLabel = isDelivery
        ? "登记发货"
        : operationType === "RECEIPT"
          ? "登记入库"
          : operationType === "ELECTRONIC"
            ? "登记电子交付"
            : operationType === "SERVICE"
              ? "登记服务完成"
              : "处理履约"
    const href = buildWorkspaceDetailHref("tasks", props.item.workItemId)
    return (
        <>
            {isDelivery ? (
                <WorkspaceDeliveryGate
                    key={props.item.workItemId}
                    item={props.item}
                />
            ) : null}
            <Button
                id={`workspace-fulfillment-process-${toAutomationIdSegment(props.item.workItemId)}`}
                type="button"
                onClick={() => {
                    if (!navigateWorkspace(href)) {
                        router.push(href)
                    }
                }}
            >
                {actionLabel}
            </Button>
        </>
    )
}
