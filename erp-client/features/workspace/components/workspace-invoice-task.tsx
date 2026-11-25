"use client"

import { useEffect, useRef, useState } from "react"
import Link from "next/link"
import {
    ArrowUpRightIcon,
    CopyIcon,
    FileTextIcon,
    PanelRightCloseIcon,
    XIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    DecisionKpiBar,
    DetailWorkspaceMeta,
    MoneyValue,
    PaperDocumentViewport,
    RateValue,
    WorkspaceFactGrid,
    WorkspaceFactGroup,
    WorkspaceFactItem,
    WorkspaceProcessingPane,
    detailWorkspaceStyles,
    useWorkspaceInspectorMode,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button, buttonVariants } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toast } from "@/components/ui/toast"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useCustomerAccountsDetailQuery } from "@/features/customer-receivables/hooks/queries"
import type { InvoiceRequest } from "@/features/invoice-requests/api"
import { resolveInvoiceRequestLines } from "@/features/invoice-requests/components/request-details"
import { useInvoiceRequests } from "@/features/invoice-requests/hooks/queries"
import {
    canonicalInvoiceDecimal,
    INVOICE_QUANTITY_SCALE,
    INVOICE_UNIT_PRICE_SCALE,
    invoiceLineAmount,
    invoiceLinesTotal,
    isPositiveInvoiceDecimal,
} from "@/features/invoice-requests/line-amounts"
import { SalesOrderPaperDocument } from "@/features/sales-orders/components/sales-order-paper-dialog"
import { SalesOrderPaperPreviewDialog } from "@/features/sales-orders/components/sales-order-paper-preview-dialog"
import { useSalesOrderDetailQuery } from "@/features/sales-orders/hooks/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    compareDecimal,
    divideFixed,
    formatFixedDisplay,
    minFixed,
    multiplyFixed,
    normalizeFixed,
    subtractFixed,
} from "@/lib/fixed-decimal"
import { hasPermission } from "@/lib/permissions"
import { isResourceAuthorizationPending } from "@/lib/resource-authorization"
import { cn } from "@/lib/utils"
import { subscribeWorkspaceHotkeyAction } from "@/lib/workspace-hotkeys"
import { sourceSalesOrderHref } from "../lib/source-sales-order"
import { stripDocumentNumberPrefix } from "../lib/stable-number"
import { workspaceInvoiceDescriptor } from "../lib/workspace-invoice"
import type { WorkspaceWorkItem } from "../types"
import {
    useWorkspaceInvoiceRegistration,
    WorkspaceInvoiceUpload,
} from "./workspace-invoice-upload"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

type WorkspaceInvoiceTaskProps = Readonly<{
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}>

const currencyScale = { maxScale: 2, outputScale: 2 }
const wrapMoney =
    "[&_[data-slot=money-value-number]]:wrap-anywhere [&_[data-slot=money-value-number]]:whitespace-normal"

function invoiceAmount(value: string): string | null {
    try {
        return normalizeFixed(value, currencyScale)
    } catch {
        return null
    }
}

function remainingApproved(amount: string, invoiced: string): string | null {
    const approved = invoiceAmount(amount)
    const registered = invoiceAmount(invoiced)
    return approved !== null && registered !== null
        ? subtractFixed(approved, registered, currencyScale)
        : null
}

function registeredRatio(approved: string, registered: string): string | null {
    try {
        if (compareDecimal(approved, "0", 2) <= 0) return null
        const capped = minFixed(registered, approved, currencyScale)
        return divideFixed(capped, approved, {
            numeratorMaxScale: 2,
            denominatorMaxScale: 2,
            outputScale: 4,
        })
    } catch {
        return null
    }
}

export function WorkspaceInvoiceTask(props: WorkspaceInvoiceTaskProps) {
    return <InvoiceExecutionWorkbench key={props.item.workItemId} {...props} />
}

function InvoiceExecutionWorkbench({
    item,
    onTaskCompleted,
}: WorkspaceInvoiceTaskProps) {
    const profileQuery = useAccountProfileQuery()
    const permissions = profileQuery.isError
        ? undefined
        : profileQuery.data?.permissions
    const canReadRequest = hasPermission(
        permissions,
        "sales_invoice_request:list",
    )
    const canOpenRequest = hasPermission(
        permissions,
        "sales_invoice_request:detail",
    )
    const canReadReceivable = hasPermission(
        permissions,
        "receivable_account:detail",
    )
    const canReadSalesOrder = hasPermission(permissions, "sales_order:detail")
    const canPostInvoice = hasPermission(permissions, "invoice:post")
    const descriptor = workspaceInvoiceDescriptor(item)
    const idPrefix = `workspace-invoice-${toAutomationIdSegment(item.workItemId)}`
    const requestQuery = useInvoiceRequests(
        {
            work_item_id: item.workItemId,
            receivable_account_id: descriptor?.receivableAccountId,
            page_size: 2,
        },
        Boolean(descriptor && item.workItemId && canReadRequest),
    )
    const request =
        canReadRequest &&
        requestQuery.isSuccess &&
        requestQuery.data.items.length === 1
            ? requestQuery.data.items[0]
            : undefined
    const taskRequest =
        descriptor &&
        request?.work_item_id === item.workItemId &&
        request.receivable_account_id === descriptor.receivableAccountId
            ? request
            : undefined
    const receivableQuery = useCustomerAccountsDetailQuery(
        taskRequest && canReadReceivable ? "receivable" : null,
        descriptor?.receivableAccountId ?? null,
    )
    const receivable =
        canReadReceivable && receivableQuery.isSuccess
            ? receivableQuery.data?.receivable
            : undefined
    const identityOk = Boolean(
        descriptor &&
        taskRequest &&
        receivable?.salesOrderId &&
        receivable.accountId === descriptor.receivableAccountId &&
        (!taskRequest.sales_order_id ||
            taskRequest.sales_order_id === receivable.salesOrderId),
    )
    const referenceSalesOrderId =
        identityOk && canReadSalesOrder ? receivable?.salesOrderId : undefined
    const salesQuery = useSalesOrderDetailQuery(referenceSalesOrderId ?? "")
    const salesOrder = referenceSalesOrderId ? salesQuery.data : undefined
    const currentRevision = salesOrder?.revisions.find(
        (revision) => revision.revisionNo === salesOrder.currentRevisionNo,
    )
    const salesOrderNo =
        (identityOk && receivable?.salesOrderNo) || taskRequest?.sales_order_no
    const stableNo = stripDocumentNumberPrefix(item.stableNumber)
    const title = item.workItemTypeLabel || "销项开票处理"
    const customer =
        (identityOk && receivable?.counterpartyPartyName) ||
        item.counterpartyName ||
        item.objectTitle

    const inspectorMode = useWorkspaceInspectorMode()
    const [salesRailOpen, setSalesRailOpen] = useState(false)
    const [narrowPreviewOpen, setNarrowPreviewOpen] = useState(false)
    const [wide, setWide] = useState(false)
    const rootRef = useRef<HTMLDivElement>(null)
    const toggleRef = useRef<HTMLButtonElement>(null)

    useEffect(() => {
        if (inspectorMode || !rootRef.current) return
        const target =
            rootRef.current.closest<HTMLElement>("#main-content") ??
            rootRef.current
        const media = window.matchMedia("(min-width: 960px)")
        const sync = () => {
            const canSplit = media.matches && target.clientWidth >= 960
            setWide(canSplit)
            if (!canSplit) setSalesRailOpen(false)
            else setNarrowPreviewOpen(false)
        }
        const observer =
            typeof ResizeObserver === "undefined"
                ? null
                : new ResizeObserver(sync)
        observer?.observe(target)
        media.addEventListener("change", sync)
        sync()
        return () => {
            observer?.disconnect()
            media.removeEventListener("change", sync)
        }
    }, [inspectorMode])

    useEffect(() => {
        if (inspectorMode || !referenceSalesOrderId) return
        return subscribeWorkspaceHotkeyAction((action) => {
            if (action !== "toggle-rail") return
            if (wide) setSalesRailOpen((open) => !open)
            else setNarrowPreviewOpen((open) => !open)
        })
    }, [inspectorMode, referenceSalesOrderId, wide])

    const readOnly =
        item.status !== "OPEN" ||
        !item.allowedActions.includes("PROCESS") ||
        taskRequest?.status === "completed"
    const amount = taskRequest
        ? remainingApproved(
              taskRequest.data.amount,
              taskRequest.invoiced_amount,
          )
        : null
    const remainingPositive =
        amount !== null && compareDecimal(amount, "0", 2) > 0
    const accountAmount = receivable
        ? invoiceAmount(receivable.openInvoiceableTotal)
        : null
    const balanceMatches = Boolean(
        amount !== null &&
        accountAmount !== null &&
        compareDecimal(amount, accountAmount, 2) <= 0,
    )
    const canRegister = Boolean(
        taskRequest?.status === "approved" &&
        !readOnly &&
        canPostInvoice &&
        identityOk &&
        remainingPositive &&
        balanceMatches &&
        !requestQuery.isError &&
        !receivableQuery.isError,
    )
    const registration = useWorkspaceInvoiceRegistration({
        item,
        request: taskRequest,
        amount,
        canRegister,
        canVerify: canReadRequest && canPostInvoice && !profileQuery.isError,
        onTaskCompleted,
    })

    let alertBanner
    if (!descriptor) {
        alertBanner = (
            <InvoiceTaskNotice
                title="无法确认本条待办对应的开票资料"
                variant="destructive"
            >
                请联系管理员核对开票负责人和关联销售单。
            </InvoiceTaskNotice>
        )
    } else if (profileQuery.isError) {
        alertBanner = (
            <BusinessFailureState
                idPrefix={`${idPrefix}-profile`}
                title="暂时无法核对账号权限"
                error={profileQuery.error}
                onRetry={() => void profileQuery.refetch()}
            />
        )
    } else if (profileQuery.data && !canReadRequest) {
        alertBanner = <InvoiceTaskNotice title="当前账号不能查看关联开票申请" />
    } else if (isResourceAuthorizationPending(requestQuery.error)) {
        alertBanner = (
            <InvoiceTaskNotice title="资料授权正在处理">
                审批已完成，资料准备好后将自动刷新。
                <Button
                    id={`${idPrefix}-grant-refresh`}
                    type="button"
                    variant="outline"
                    size="xs"
                    className="ml-2"
                    onClick={() => void requestQuery.refetch()}
                >
                    刷新资料
                </Button>
            </InvoiceTaskNotice>
        )
    } else if (requestQuery.isError) {
        alertBanner = (
            <BusinessFailureState
                idPrefix={`${idPrefix}-request`}
                title="开票申请暂不可查看"
                error={requestQuery.error}
                onRetry={() => void requestQuery.refetch()}
            />
        )
    } else if (requestQuery.isSuccess && !taskRequest) {
        alertBanner = (
            <InvoiceTaskNotice title="未能核对本任务关联的开票申请">
                请刷新任务。若仍无法查看，请联系管理员核对任务和申请的关联。
            </InvoiceTaskNotice>
        )
    } else if (taskRequest && readOnly) {
        alertBanner = (
            <InvoiceTaskNotice
                title={
                    taskRequest.status === "completed"
                        ? "本次申请已全部开票"
                        : item.status === "COMPLETED"
                          ? "该开票任务已完成"
                          : "当前仅可查看开票资料"
                }
                variant="info"
            >
                {item.status === "OPEN"
                    ? item.actionBlockers[0]?.message
                    : null}
            </InvoiceTaskNotice>
        )
    } else if (taskRequest && (!canPostInvoice || !canReadReceivable)) {
        alertBanner = (
            <InvoiceTaskNotice title="当前账号没有办理本次开票所需的权限" />
        )
    } else if (taskRequest && receivableQuery.isError) {
        alertBanner = (
            <BusinessFailureState
                idPrefix={`${idPrefix}-receivable`}
                title="销售应收资料暂不可查看"
                error={receivableQuery.error}
                onRetry={() => void receivableQuery.refetch()}
            />
        )
    } else if (taskRequest && receivableQuery.isSuccess && !identityOk) {
        alertBanner = (
            <InvoiceTaskNotice
                title="待办关联的销售应收不匹配"
                variant="destructive"
            >
                请联系管理员核对本条待办关联的销售单和应收记录。
            </InvoiceTaskNotice>
        )
    } else if (taskRequest && taskRequest.status !== "approved") {
        alertBanner = (
            <InvoiceTaskNotice title="此任务没有可执行的已批准申请">
                请刷新查看最新处理结果。
            </InvoiceTaskNotice>
        )
    } else if (identityOk && (!remainingPositive || !balanceMatches)) {
        alertBanner = (
            <InvoiceTaskNotice title="待开票余额需要核对" variant="destructive">
                {remainingPositive && accountAmount !== null ? (
                    <>
                        本申请剩余金额超过销售应收可开票额度{" "}
                        <MoneyValue
                            value={accountAmount}
                            display="exact"
                            inheritColor
                        />
                        。
                    </>
                ) : (
                    "请刷新并核对开票申请与销售应收的余额。"
                )}
                <Button
                    id={`${idPrefix}-balance-refresh`}
                    type="button"
                    variant="outline"
                    size="xs"
                    className="ml-2"
                    onClick={() => {
                        void requestQuery.refetch()
                        void receivableQuery.refetch()
                    }}
                >
                    刷新余额
                </Button>
            </InvoiceTaskNotice>
        )
    }

    const showDetails =
        taskRequest &&
        !requestQuery.isError &&
        (readOnly || identityOk || !canReadReceivable || !canPostInvoice)
    const showRegistration = canRegister || registration.verifying
    const loading =
        !alertBanner &&
        (requestQuery.isPending || (!readOnly && receivableQuery.isPending)) &&
        !showDetails

    const canSplitRail =
        !inspectorMode && wide && Boolean(referenceSalesOrderId)
    const closeRail = () => {
        setSalesRailOpen(false)
        toggleRef.current?.focus()
    }

    return (
        <div ref={rootRef} className="h-full min-h-0 min-w-0 bg-card">
            <WorkspaceProcessingPane
                archetype="action"
                className="@container/document"
                idPrefix={idPrefix}
                aria-label="当前开票任务"
                alertBanner={alertBanner}
                contextRail={
                    canSplitRail && referenceSalesOrderId
                        ? {
                              allowOnFullWidth: true,
                              collapsible: true,
                              resizable: true,
                              defaultWidthPercent: 38,
                              minWidthPercent: 26,
                              maxWidthPercent: 52,
                              storageKey: "workspace-invoice-sales-rail",
                              collapsed: !salesRailOpen,
                              onCollapsedChange: (collapsed) =>
                                  setSalesRailOpen(!collapsed),
                              label: "对照销售单",
                              content: (
                                  <SalesOrderReferenceRail
                                      idPrefix={idPrefix}
                                      salesOrderId={referenceSalesOrderId}
                                      salesOrderNo={salesOrderNo}
                                      query={salesQuery}
                                      onClose={closeRail}
                                  />
                              ),
                          }
                        : undefined
                }
                header={
                    <WorkspaceTaskIdentityHeader
                        item={item}
                        title={title}
                        subtitle={
                            <DetailWorkspaceMeta
                                items={[
                                    customer &&
                                    customer !== title &&
                                    customer !== stableNo
                                        ? customer
                                        : null,
                                    stableNo &&
                                    stableNo !== salesOrderNo &&
                                    stableNo !== title ? (
                                        <span
                                            key="stable-number"
                                            className="num select-all"
                                        >
                                            {stableNo}
                                        </span>
                                    ) : null,
                                    salesOrderNo && referenceSalesOrderId ? (
                                        <Link
                                            key="sales-order"
                                            id={`${idPrefix}-source-sales`}
                                            href={sourceSalesOrderHref(
                                                referenceSalesOrderId,
                                            )}
                                            className="num select-all underline decoration-border underline-offset-4 hover:decoration-foreground"
                                        >
                                            销售单 {salesOrderNo}
                                        </Link>
                                    ) : salesOrderNo ? (
                                        <span
                                            key="sales-order"
                                            className="num select-all"
                                        >
                                            销售单 {salesOrderNo}
                                        </span>
                                    ) : null,
                                    item.dueAt && item.dueAtLabel ? (
                                        <span key="deadline" className="num">
                                            任务截止 {item.dueAtLabel}
                                        </span>
                                    ) : null,
                                ]}
                            />
                        }
                    >
                        {referenceSalesOrderId ? (
                            inspectorMode ? (
                                <Link
                                    id={`${idPrefix}-sales-reference`}
                                    href={sourceSalesOrderHref(
                                        referenceSalesOrderId,
                                    )}
                                    className={buttonVariants({
                                        variant: "ghost",
                                        size: "sm",
                                    })}
                                >
                                    <FileTextIcon
                                        aria-hidden="true"
                                        data-icon="inline-start"
                                    />
                                    查看销售单
                                    <ArrowUpRightIcon
                                        aria-hidden="true"
                                        data-icon="inline-end"
                                    />
                                </Link>
                            ) : (
                                <Button
                                    ref={toggleRef}
                                    id={`${idPrefix}-sales-reference`}
                                    type="button"
                                    variant={
                                        canSplitRail && salesRailOpen
                                            ? "secondary"
                                            : "ghost"
                                    }
                                    size="sm"
                                    aria-expanded={
                                        canSplitRail ? salesRailOpen : undefined
                                    }
                                    aria-haspopup={
                                        canSplitRail ? undefined : "dialog"
                                    }
                                    onClick={() => {
                                        if (canSplitRail)
                                            setSalesRailOpen((open) => !open)
                                        else setNarrowPreviewOpen(true)
                                    }}
                                >
                                    {canSplitRail && salesRailOpen ? (
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
                                    {canSplitRail && salesRailOpen
                                        ? "收起销售单"
                                        : "对照销售单"}
                                </Button>
                            )
                        ) : null}
                    </WorkspaceTaskIdentityHeader>
                }
            >
                {loading ? (
                    <p
                        className="flex items-center gap-2 py-5 text-xs text-muted-foreground"
                        role="status"
                    >
                        <Spinner aria-hidden="true" />
                        正在读取开票资料…
                    </p>
                ) : null}
                {showDetails || showRegistration ? (
                    <div className="@container/invoice-task flex min-w-0 flex-col gap-4 bg-card py-4">
                        {showDetails ? (
                            <>
                                <InvoiceKpiBand
                                    idPrefix={idPrefix}
                                    request={taskRequest}
                                    amount={amount}
                                />
                                <InvoiceCommercialDetails
                                    idPrefix={idPrefix}
                                    request={taskRequest}
                                    canOpenRequest={canOpenRequest}
                                    invoiceType={currentRevision?.invoiceType}
                                    taxPoint={
                                        currentRevision?.taxPoint ||
                                        salesOrder?.taxRatePercent
                                    }
                                />
                            </>
                        ) : null}
                        {showRegistration ? (
                            <WorkspaceInvoiceUpload
                                idPrefix={idPrefix}
                                registration={registration}
                            />
                        ) : null}
                    </div>
                ) : null}
                {referenceSalesOrderId && !inspectorMode && !wide ? (
                    <SalesOrderPaperPreviewDialog
                        salesOrderId={referenceSalesOrderId}
                        title={salesOrderNo}
                        open={narrowPreviewOpen}
                        onOpenChange={setNarrowPreviewOpen}
                    />
                ) : null}
            </WorkspaceProcessingPane>
        </div>
    )
}

function InvoiceTaskNotice({
    title,
    variant = "warning",
    children,
}: {
    title: string
    variant?: "warning" | "destructive" | "info"
    children?: React.ReactNode
}) {
    return (
        <Alert variant={variant} className="border-l-2">
            <AlertTitle>{title}</AlertTitle>
            {children ? <AlertDescription>{children}</AlertDescription> : null}
        </Alert>
    )
}

function InvoiceKpiBand({
    idPrefix,
    request,
    amount,
}: {
    idPrefix: string
    request: InvoiceRequest
    amount: string | null
}) {
    const ratio = registeredRatio(request.data.amount, request.invoiced_amount)
    const ratioPercent =
        ratio !== null
            ? multiplyFixed(ratio, "100", {
                  leftMaxScale: 4,
                  rightMaxScale: 0,
                  outputScale: 1,
              })
            : null

    return (
        <DecisionKpiBar
            columns={2}
            ariaLabel="本申请开票指标"
            items={[
                {
                    id: "balance",
                    label: "待开票余额",
                    emphasis: "primary",
                    formulaTooltipId: `${idPrefix}-balance-formula`,
                    formulaTooltipLabel: "待开票余额计算公式",
                    formulaTooltip: "批复开票总额 − 已登记开票金额",
                    value: (
                        <MoneyValue
                            value={amount}
                            display="exact"
                            inheritColor
                        />
                    ),
                    footnote: "本申请 · 含税",
                },
                {
                    id: "approved-total",
                    label: "批复总额",
                    emphasis: "secondary",
                    value: (
                        <MoneyValue
                            value={request.data.amount}
                            display="exact"
                            inheritColor
                        />
                    ),
                    footnote: (
                        <>
                            已登记
                            <MoneyValue
                                value={request.invoiced_amount}
                                display="exact"
                                inheritColor
                            />
                            {ratioPercent !== null ? (
                                <span className="num">（{ratioPercent}%）</span>
                            ) : null}
                        </>
                    ),
                    extra:
                        ratio !== null ? (
                            <div
                                aria-hidden="true"
                                className="h-1.5 overflow-hidden rounded-full bg-border"
                            >
                                <div
                                    className="h-full rounded-full bg-primary"
                                    style={{ width: `calc(${ratio} * 100%)` }}
                                />
                            </div>
                        ) : undefined,
                },
            ]}
        />
    )
}

function InvoiceCommercialDetails({
    idPrefix,
    request,
    canOpenRequest,
    invoiceType,
    taxPoint,
}: {
    idPrefix: string
    request: InvoiceRequest
    canOpenRequest: boolean
    invoiceType?: string
    taxPoint?: string
}) {
    const { data } = request
    const lines = resolveInvoiceRequestLines(data)
    const lineTotal = lines.length ? invoiceLinesTotal(lines) : null
    const taxRate = taxPoint?.replace(/[%％]$/, "").trim()
    const taxNumber = data.tax_number.trim()
    const sellerTaxNumber = data.seller_tax_number?.trim()
    const requestNumber = (
        <span className="num select-all">{request.request_no}</span>
    )

    return (
        <section aria-label="开票申请资料" className="min-w-0 space-y-4">
            <WorkspaceFactGroup
                title="开票资料与抬头"
                className="border-b border-border/60 pb-4"
                actions={
                    request.request_no ? (
                        canOpenRequest ? (
                            <Link
                                id={`${idPrefix}-linked-request-${toAutomationIdSegment(request.id)}`}
                                href={`/finance/customer-accounts/invoice-requests/${encodeURIComponent(request.id)}`}
                                className={cn(
                                    "inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium",
                                    detailWorkspaceStyles.inlineLinkButton,
                                )}
                            >
                                <span className="font-normal text-muted-foreground">
                                    开票申请
                                </span>
                                {requestNumber}
                                <ArrowUpRightIcon
                                    aria-hidden="true"
                                    className="size-3.5"
                                />
                            </Link>
                        ) : (
                            <span className="text-xs text-muted-foreground">
                                开票申请 {requestNumber}
                            </span>
                        )
                    ) : undefined
                }
            >
                <WorkspaceFactGrid columns={3} ariaLabel="开票资料与抬头">
                    <WorkspaceFactItem
                        label="购方抬头"
                        value={data.invoice_title || undefined}
                        selectable={Boolean(data.invoice_title)}
                    />
                    <WorkspaceFactItem label="购方税号" numeric>
                        {taxNumber ? (
                            <span className="inline-flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5">
                                <span className="num min-w-0 wrap-anywhere select-all">
                                    {taxNumber}
                                </span>
                                <Button
                                    id={`${idPrefix}-copy-tax-number`}
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    aria-label="复制购方税号"
                                    onClick={async () => {
                                        try {
                                            await navigator.clipboard.writeText(
                                                taxNumber,
                                            )
                                            toast.add({
                                                title: "已复制购方税号",
                                                type: "success",
                                            })
                                        } catch {
                                            toast.add({
                                                title: "复制失败，请选中税号手动复制",
                                                type: "error",
                                            })
                                        }
                                    }}
                                >
                                    <CopyIcon
                                        aria-hidden="true"
                                        data-icon="inline-start"
                                    />
                                    复制
                                </Button>
                            </span>
                        ) : null}
                    </WorkspaceFactItem>
                    <WorkspaceFactItem
                        label="开票类型"
                        value={invoiceType || undefined}
                    />
                    <WorkspaceFactItem label="约定税点" numeric>
                        {taxRate ? (
                            <RateValue value={taxRate} precision={2} />
                        ) : null}
                    </WorkspaceFactItem>
                    <WorkspaceFactItem
                        label="销方开票主体"
                        value={data.seller_name || undefined}
                    />
                    <WorkspaceFactItem
                        label="销方税号"
                        value={sellerTaxNumber || undefined}
                        numeric
                        selectable={Boolean(sellerTaxNumber)}
                    />
                    {data.reason ? (
                        <WorkspaceFactItem
                            label="申请事由"
                            value={data.reason}
                            span="full"
                        />
                    ) : null}
                </WorkspaceFactGrid>
            </WorkspaceFactGroup>
            {lines.length ? (
                <Table
                    aria-label="开票明细"
                    className="w-full table-fixed text-body-compact"
                >
                    <TableHeader className="bg-transparent [&_th]:bg-surface-sunken [&_th:first-child]:rounded-l-md [&_th:last-child]:rounded-r-md [&_tr]:border-b-0">
                        <TableRow>
                            <TableHead className="whitespace-normal">
                                开票内容
                            </TableHead>
                            <TableHead className="w-1/6 px-1 text-right whitespace-normal">
                                数量
                            </TableHead>
                            <TableHead className="w-1/5 px-1 text-right whitespace-normal">
                                含税单价
                            </TableHead>
                            <TableHead className="w-1/5 px-1 text-right whitespace-normal">
                                行金额
                            </TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {lines.map((line, index) => (
                            <TableRow key={index}>
                                <TableCell className="wrap-anywhere whitespace-pre-wrap">
                                    {line.content || "—"}
                                </TableCell>
                                <TableCell className="num px-1 text-right wrap-anywhere whitespace-normal">
                                    {invoiceDetailDecimal(
                                        line.quantity,
                                        INVOICE_QUANTITY_SCALE,
                                    )}
                                </TableCell>
                                <TableCell className="num px-1 text-right wrap-anywhere whitespace-normal">
                                    {invoiceDetailDecimal(
                                        line.unit_price,
                                        INVOICE_UNIT_PRICE_SCALE,
                                    )}
                                </TableCell>
                                <TableCell className="px-1 text-right">
                                    <MoneyValue
                                        value={invoiceLineAmount(line)}
                                        display="exact"
                                        className={wrapMoney}
                                    />
                                </TableCell>
                            </TableRow>
                        ))}
                    </TableBody>
                    {lineTotal !== null ? (
                        <TableFooter className="border-0 bg-transparent [&_td]:bg-surface-sunken [&_td:first-child]:rounded-l-md [&_td:last-child]:rounded-r-md [&_tr]:border-b-0">
                            <TableRow className="hover:bg-transparent">
                                <TableCell
                                    colSpan={3}
                                    className="font-semibold"
                                >
                                    整单明细合计{" "}
                                    <span className="num ml-1 text-xs font-normal text-muted-foreground">
                                        {lines.length} 项
                                    </span>
                                </TableCell>
                                <TableCell className="px-1 text-right font-semibold">
                                    <MoneyValue
                                        value={lineTotal}
                                        display="exact"
                                        className={wrapMoney}
                                    />
                                </TableCell>
                            </TableRow>
                        </TableFooter>
                    ) : null}
                </Table>
            ) : null}
        </section>
    )
}

function invoiceDetailDecimal(value: string, maxScale: number) {
    return isPositiveInvoiceDecimal(value, maxScale)
        ? formatFixedDisplay(canonicalInvoiceDecimal(value), { maxScale })
        : "—"
}

function SalesOrderReferenceRail({
    idPrefix,
    salesOrderId,
    salesOrderNo,
    query,
    onClose,
}: {
    idPrefix: string
    salesOrderId: string
    salesOrderNo?: string
    query: ReturnType<typeof useSalesOrderDetailQuery>
    onClose: () => void
}) {
    return (
        <div className="flex h-full min-h-0 min-w-0 flex-col bg-card">
            <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border/60 py-2.5">
                <div className="min-w-0">
                    <h2 className="text-body-compact font-semibold">
                        对照销售单
                    </h2>
                    {salesOrderNo ? (
                        <p className="num truncate text-xs text-muted-foreground select-all">
                            {salesOrderNo}
                        </p>
                    ) : null}
                </div>
                <Button
                    id={`${idPrefix}-sales-rail-close`}
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    aria-label="收起销售单对照栏"
                    onClick={onClose}
                >
                    <XIcon aria-hidden="true" />
                </Button>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain py-3">
                {query.isPending ? (
                    <p
                        className="flex items-center justify-center gap-2 py-10 text-xs text-muted-foreground"
                        role="status"
                    >
                        <Spinner aria-hidden="true" />
                        正在读取销售单…
                    </p>
                ) : query.isError ? (
                    <BusinessFailureState
                        idPrefix={`${idPrefix}-sales-rail`}
                        title="销售单读取失败"
                        error={query.error}
                        onRetry={() => void query.refetch()}
                    />
                ) : query.data ? (
                    <PaperDocumentViewport
                        id={`${idPrefix}-sales-paper`}
                        fitMode="width"
                        fitKey={salesOrderId}
                        role="region"
                        aria-label="销售单纸面"
                        tabIndex={0}
                        className="bg-transparent p-0"
                    >
                        <SalesOrderPaperDocument order={query.data} />
                    </PaperDocumentViewport>
                ) : (
                    <p className="py-10 text-center text-xs text-muted-foreground">
                        暂未找到这张销售单。
                    </p>
                )}
            </div>
        </div>
    )
}
