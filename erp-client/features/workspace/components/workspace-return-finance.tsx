"use client"

import {
    useCallback,
    useEffect,
    useLayoutEffect,
    useRef,
    useState,
} from "react"
import Link from "next/link"
import { useQueryClient } from "@tanstack/react-query"
import {
    FileTextIcon,
    PanelRightCloseIcon,
    TriangleAlertIcon,
    XIcon,
} from "lucide-react"
import {
    DetailWorkspaceMeta,
    PaperDocumentViewport,
    WorkspaceProcessingPane,
} from "@/components/business"
import { WorkspaceTaskFooter } from "@/components/business/workspace-task-pane"
import { Button, buttonVariants } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { PurchaseOrderPaperDocument } from "@/features/purchase-orders/components/purchase-order-paper-document"
import { usePurchaseOrderCenterQuery } from "@/features/purchase-orders/hooks/queries"
import { sourceReturnProgress } from "@/features/purchase-orders/lib/source-return-progress"
import { useReturnSourcesQuery } from "@/features/supplier-payables/hooks/queries"
import { useSupplierRefundFlow } from "@/features/supplier-payables/pages/hooks/use-supplier-refund-flow"
import { SupplierRefundRequestDialog } from "@/features/supplier-payables/components/supplier-refund-request-dialog"
import type {
    FormalSubmitResult,
    PaymentRow,
    PurchaseInvoiceRow,
} from "@/features/supplier-payables/types"
import {
    useWorkItemDetailQuery,
    workItemKeys,
} from "@/features/work-items/queries"
import { getErrorMessage } from "@/lib/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    clampZeroFixed,
    compareDecimal,
    minFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import { queryKeyRoots } from "@/lib/query-key-roots"
import { cn } from "@/lib/utils"
import { buildWorkspaceDetailHref } from "@/lib/workspace-detail-href"
import { subscribeWorkspaceHotkeyAction } from "@/lib/workspace-hotkeys"
import { stripDocumentNumberPrefix } from "../lib/stable-number"
import type { WorkspaceWorkItem } from "../types"
import {
    ReturnFinanceAmounts,
    ReturnFinanceSummary,
    ReturnFinanceTable,
    returnFinanceDecimal as decimal,
    type ReturnFinanceTableHandle,
    type ReturnFinanceTableRow,
} from "./return-finance-summary"
import { ReturnInvoiceDialog } from "./return-invoice-dialog"
import { WorkspaceDocumentPaperDialog } from "./workspace-document-paper-dialog"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

export function WorkspaceReturnFinance({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (id: string) => void
}) {
    const invoiceTask = item.workItemType === "PURCHASE_RETURN_INVOICE"
    const title = invoiceTask ? "退货进项红票" : "退货款处理"
    const id = `workspace-return-finance-${toAutomationIdSegment(item.workItemId)}`
    const purchaseHref = `/procurement/orders/${encodeURIComponent(item.businessObjectId)}`
    const queryClient = useQueryClient()
    const order = usePurchaseOrderCenterQuery(item.businessObjectId)
    const task = useWorkItemDetailQuery(item.workItemId)
    const supplierId = order.data?.header.supplierId
    const sources = useReturnSourcesQuery(supplierId, invoiceTask)
    const canvas = useRef<HTMLDivElement>(null)
    const sourceTableRef = useRef<ReturnFinanceTableHandle>(null)
    const historyTableRef = useRef<ReturnFinanceTableHandle>(null)
    const [isWide, setIsWide] = useState(false)
    const [inSheet, setInSheet] = useState(false)
    const [referenceOpen, setReferenceOpen] = useState(false)
    const [result, setResult] = useState<FormalSubmitResult | null>(null)
    const [invoice, setInvoice] = useState<{
        row: PurchaseInvoiceRow
        accountId: string
    } | null>(null)
    const notified = useRef(false)
    const notifyCompleted = useCallback(() => {
        if (notified.current) return
        notified.current = true
        onTaskCompleted?.(item.workItemId)
    }, [item.workItemId, onTaskCompleted])

    useLayoutEffect(() => {
        const root = canvas.current?.closest<HTMLElement>(
            '[data-slot="unified-task-workbench"]',
        )
        if (!root) return
        const sheet = root.closest(
            '[data-business-component="task-action-sheet"]',
        )
        const main = root.closest<HTMLElement>("#main-content") ?? root
        setInSheet(Boolean(sheet))

        const sync = () =>
            setIsWide(
                !sheet && root.clientWidth >= 960 && main.clientWidth >= 960,
            )
        sync()
        if (typeof ResizeObserver === "undefined") return
        const observer = new ResizeObserver(sync)
        observer.observe(root)
        if (main !== root) observer.observe(main)
        return () => observer.disconnect()
    }, [])

    useEffect(() => {
        if (task.data?.status === "COMPLETED" && !task.isFetching)
            notifyCompleted()
    }, [task.data?.status, task.isFetching, notifyCompleted])

    useEffect(() => {
        if (!isWide || !order.data) return
        return subscribeWorkspaceHotkeyAction((action) => {
            if (action === "toggle-rail") {
                setReferenceOpen((open) => !open)
            }
        })
    }, [isWide, order.data])

    const flow = useSupplierRefundFlow({
        openRefundPreview: () => undefined,
        setLastResult: setResult,
        setActionError: (description) =>
            setResult({ status: "failed", title: "提交失败", description }),
    })
    const progress = order.data ? sourceReturnProgress(order.data) : null
    const plan = progress?.plan
    const decision = plan?.settlement_decision
    const accountId = plan?.payable_account_id
    const decisionCurrent = progress?.decisionCurrent === true
    const settlements = plan?.settlements ?? []
    const currentSettlements = settlements.filter(
        (row) => row.decision_key === decision?.idempotency_key,
    )
    const refund = outstanding(
        decision?.refund_amount,
        currentSettlements
            .filter((row) => row.method === "CASH_REFUND")
            .map((row) => row.amount),
    )
    const credit = outstanding(
        decision?.credit_amount,
        currentSettlements
            .filter((row) => row.method === "SUPPLIER_CREDIT")
            .map((row) => row.amount),
    )
    const redInvoiceRemaining = clampZeroFixed(
        progress?.redInvoice ?? "0",
        decimal,
    )
    const pending = invoiceTask
        ? redInvoiceRemaining
        : clampZeroFixed(progress?.refund ?? "0", decimal)
    const invoiceSources = accountId
        ? invoiceBalances(sources.data?.invoices ?? [], accountId)
        : []
    const paymentSources = accountId
        ? paymentBalances(sources.data?.payments ?? [], accountId)
        : []
    const invoiceAvailable = invoiceSources.filter(
        ({ row, remaining }) =>
            row.status !== "REVERSED" && positive(remaining),
    )
    const blueBalance = sumFixed(
        invoiceAvailable.map(({ remaining }) => remaining),
        decimal,
    )
    const loading =
        order.isPending ||
        Boolean(supplierId && sources.isPending && !order.isError)
    const readError = order.isError || (Boolean(supplierId) && sources.isError)
    const refreshing = order.isFetching || sources.isFetching
    const authorized = (
        task.data?.allowed_actions ?? item.allowedActions
    ).includes("PROCESS")
    const taskOpen = (task.data?.status ?? item.status) === "OPEN"
    const eligible =
        authorized &&
        taskOpen &&
        !loading &&
        !readError &&
        !!accountId &&
        decisionCurrent &&
        !progress?.physicalPending &&
        positive(pending)
    const canStart =
        eligible &&
        !refreshing &&
        !flow.refundSubmitPending &&
        result?.status !== "unknown"
    const fileRefund = positive(refund)
    const fileCredit = positive(credit)
    const showSourceActions =
        eligible &&
        !inSheet &&
        (invoiceTask
            ? invoiceAvailable.length > 0
            : paymentSources.length > 0 && (fileRefund || fileCredit))
    const balances = invoiceTask
        ? [{ label: "蓝票可冲", value: blueBalance }]
        : [
              { label: "现金退款", value: refund },
              { label: "转留抵", value: credit },
          ]
    const footerAmounts = invoiceTask
        ? [{ label: "待冲红", value: pending }, ...balances]
        : [
              { label: "现金退款余额", value: refund },
              { label: "转留抵余额", value: credit },
          ]

    async function refresh() {
        await Promise.all([
            order.refetch(),
            supplierId ? sources.refetch() : undefined,
            task.refetch(),
        ])
    }

    async function refreshAfterSuccess() {
        const [nextOrder, , nextTask] = await Promise.all([
            order.refetch(),
            supplierId ? sources.refetch() : undefined,
            task.refetch(),
            queryClient.invalidateQueries({
                queryKey: queryKeyRoots.workspaceHome,
            }),
        ])
        const nextProgress =
            nextOrder.isSuccess && nextOrder.data?.payableSummary
                ? sourceReturnProgress(nextOrder.data)
                : null
        if (
            nextTask.data?.status === "COMPLETED" ||
            (nextProgress?.plan &&
                !(invoiceTask
                    ? nextProgress.invoicePending
                    : nextProgress.refundPending))
        ) {
            queryClient.setQueryData<typeof task.data>(
                workItemKeys.detail(item.workItemId),
                (current) =>
                    current ? { ...current, status: "COMPLETED" } : current,
            )
            notifyCompleted()
        }
    }

    async function handleRefundSubmit(reason: string, amount?: string) {
        await flow.prepareRefundDraft(reason, amount)
        await refreshAfterSuccess()
    }

    function beginRefund(
        row: PaymentRow,
        allocated: string,
        method: "CASH_REFUND" | "SUPPLIER_CREDIT",
    ) {
        if (!canStart || !decision) return
        setResult(null)
        flow.setRefundRequest({
            supplierId: row.supplierId,
            supplierName: row.supplierName,
            sourcePaymentId: row.paymentId,
            sourcePaymentNo: row.paymentNo,
            sourcePurchaseOrderId: item.businessObjectId,
            settlementDecisionKey: decision.idempotency_key,
            settlementMethod: method,
            amount: minFixed(
                method === "CASH_REFUND" ? refund : credit,
                allocated,
                decimal,
            ),
        })
    }

    function beginInvoice(row: PurchaseInvoiceRow) {
        if (!canStart || !accountId) return
        setResult(null)
        setInvoice({ row, accountId })
    }

    const selectedInvoice = invoiceSources.find(
        ({ row }) => row.invoiceId === invoice?.row.invoiceId,
    )
    const invoiceLimit =
        authorized &&
        invoice?.accountId === accountId &&
        decisionCurrent &&
        !progress?.physicalPending &&
        !readError &&
        taskOpen &&
        selectedInvoice
            ? minFixed(redInvoiceRemaining, selectedInvoice.remaining, decimal)
            : "0.00"

    const sourceTableId = `${id}-sources`
    const historyTableId = `${id}-settlements`
    const invoiceDialogId = `${id}-red-invoice`
    const jumpTo = (target: string) => {
        if (target === sourceTableId && sourceTableRef.current) {
            sourceTableRef.current.locateFirst()
            return
        }
        if (target === historyTableId && historyTableRef.current) {
            historyTableRef.current.locateFirst()
            return
        }
        const element = document.getElementById(target)
        const reducedMotion = window.matchMedia(
            "(prefers-reduced-motion: reduce)",
        ).matches
        element?.focus({ preventScroll: true })
        element?.scrollIntoView({
            behavior: reducedMotion ? "instant" : "smooth",
            block: "nearest",
        })
    }
    const purchaseNo =
        order.data?.identity.purchaseNo ||
        stripDocumentNumberPrefix(item.stableNumber)
    const supplier = (
        order.data?.header.supplierSnapshot ||
        (!invoiceTask && item.counterpartyName) ||
        ""
    ).trim()
    const taskLabel = item.workItemTypeLabel?.trim()
    const openReference = () => setReferenceOpen((open) => !open)
    const refreshButton = (suffix: string) => (
        <Button
            id={`${id}-${suffix}`}
            type="button"
            variant="ghost"
            size="xs"
            disabled={refreshing}
            onClick={() => void refresh()}
        >
            刷新最新数据
        </Button>
    )
    const orderLink = (suffix: string, label: string) => (
        <Link
            id={`${id}-${suffix}`}
            href={purchaseHref}
            className={buttonVariants({ variant: "ghost", size: "xs" })}
        >
            {label}
        </Link>
    )

    let blocker
    if (readError) {
        blocker = (
            <ReturnFinanceNotice
                tone="destructive"
                title="读取退货或款票依据失败"
                detail={getErrorMessage(
                    order.error ?? sources.error,
                    "请刷新后重试。",
                )}
                action={refreshButton("read-retry")}
            />
        )
    } else if (!loading && progress?.physicalPending) {
        blocker = (
            <ReturnFinanceNotice
                tone="warning"
                title="实物退货待完成"
                detail="请先核对退货进度，完成实物退货后再办理。"
                action={orderLink("physical-return", "查看采购退货")}
            />
        )
    } else if (!loading && !decisionCurrent) {
        blocker = (
            <ReturnFinanceNotice
                tone="warning"
                title={decision ? "采购确认方案已变更" : "缺少采购确认方案"}
                detail="请联系采购负责人确认当前退货方案。"
                action={orderLink("settlement-decision", "查看采购方案")}
            />
        )
    } else if (!loading && (!supplierId || !accountId)) {
        blocker = (
            <ReturnFinanceNotice
                tone="destructive"
                title="缺少供应商或应付依据"
                detail="请核对采购单关联的供应商和应付记录。"
                action={orderLink("account-basis", "查看采购单")}
            />
        )
    } else if (
        !loading &&
        positive(pending) &&
        (invoiceTask
            ? invoiceAvailable.length === 0
            : paymentSources.length === 0)
    ) {
        blocker = (
            <ReturnFinanceNotice
                tone="warning"
                title={
                    invoiceTask
                        ? "未找到本采购可红冲的蓝票"
                        : "未找到本采购的已入账付款"
                }
                detail="请核对原单分配后刷新。"
                action={refreshButton("source-retry")}
            />
        )
    } else if (
        !loading &&
        !invoiceTask &&
        positive(pending) &&
        !fileRefund &&
        !fileCredit
    ) {
        const inApproval = currentSettlements.some(
            (row) => row.status === "in_approval",
        )
        blocker = (
            <ReturnFinanceNotice
                tone="warning"
                title={inApproval ? "款项已提交审批" : "当前方案没有可提交余额"}
                detail={
                    inApproval
                        ? "审批通过并入账后更新待退款金额。"
                        : "请联系采购负责人核对剩余差额。"
                }
                action={
                    inApproval ? (
                        <Button
                            id={`${id}-view-settlements`}
                            type="button"
                            size="xs"
                            variant="ghost"
                            onClick={() => jumpTo(historyTableId)}
                        >
                            查看办理记录
                        </Button>
                    ) : (
                        orderLink("balance-decision", "查看采购方案")
                    )
                }
            />
        )
    }

    const invoiceRows: ReturnFinanceTableRow[] = invoiceSources.map(
        ({ row, applied, remaining }) => ({
            key: row.invoiceId,
            documentNo: row.invoiceNo,
            detail: row.invoiceDate,
            status: { label: row.statusLabel, tone: row.statusTone },
            amount: applied,
            balance: remaining,
            action:
                showSourceActions &&
                row.status !== "REVERSED" &&
                positive(remaining) ? (
                    <Button
                        id={`${id}-invoice-${toAutomationIdSegment(row.invoiceId)}`}
                        className="h-auto min-h-control-xs whitespace-normal"
                        type="button"
                        size="xs"
                        variant="outline"
                        disabled={!canStart}
                        onClick={() => beginInvoice(row)}
                    >
                        登记红票
                    </Button>
                ) : null,
        }),
    )
    const paymentRows: ReturnFinanceTableRow[] = paymentSources.map(
        ({ row, allocated }) => ({
            key: row.paymentId,
            documentNo: row.paymentNo,
            amount: allocated,
            status: { label: row.statusLabel, tone: row.statusTone },
            action: showSourceActions ? (
                <div className="flex flex-wrap justify-end gap-1">
                    {fileRefund && (
                        <Button
                            id={`${id}-refund-${toAutomationIdSegment(row.paymentId)}`}
                            className="h-auto min-h-control-xs whitespace-normal"
                            type="button"
                            size="xs"
                            variant="outline"
                            disabled={!canStart}
                            onClick={() =>
                                beginRefund(row, allocated, "CASH_REFUND")
                            }
                        >
                            登记退款
                        </Button>
                    )}
                    {fileCredit && (
                        <Button
                            id={`${id}-credit-${toAutomationIdSegment(row.paymentId)}`}
                            className="h-auto min-h-control-xs whitespace-normal"
                            type="button"
                            size="xs"
                            variant="outline"
                            disabled={!canStart}
                            onClick={() =>
                                beginRefund(row, allocated, "SUPPLIER_CREDIT")
                            }
                        >
                            转留抵
                        </Button>
                    )}
                </div>
            ) : null,
        }),
    )
    const settlementRows: ReturnFinanceTableRow[] = settlements.map((row) => ({
        key: row.id,
        documentNo: row.number,
        documentLink: (
            <Link
                id={`${id}-settlement-${toAutomationIdSegment(row.id)}`}
                href={`/finance/supplier-accounts?view=payment&previewKind=refund&detailId=${encodeURIComponent(row.id)}`}
                className="num text-primary underline underline-offset-2"
            >
                {row.number}
            </Link>
        ),
        detail: row.method === "CASH_REFUND" ? "现金退款" : "转留抵",
        status:
            row.status === "posted"
                ? { label: "已入账", tone: "success" }
                : { label: "审批中", tone: "warning" },
        amount: row.amount,
    }))

    const [singleInvoice] =
        invoiceAvailable.length === 1 ? invoiceAvailable : []
    const [singlePayment] = paymentSources.length === 1 ? paymentSources : []
    const primaryLabel = invoiceTask
        ? singleInvoice
            ? "登记进项红票"
            : "选择来源蓝票"
        : singlePayment
          ? fileRefund
              ? "登记实际退款"
              : "提交留抵审批"
          : "选择来源付款"
    const startPrimary = () => {
        if (invoiceTask && singleInvoice) beginInvoice(singleInvoice.row)
        else if (!invoiceTask && singlePayment)
            beginRefund(
                singlePayment.row,
                singlePayment.allocated,
                fileRefund ? "CASH_REFUND" : "SUPPLIER_CREDIT",
            )
        else jumpTo(sourceTableId)
    }
    const footerSummary = <ReturnFinanceAmounts amounts={footerAmounts} />
    const footerActions =
        inSheet && authorized && taskOpen && positive(pending) ? (
            <Link
                id={`${id}-open-full-task`}
                href={buildWorkspaceDetailHref("tasks", item.workItemId)}
                className={buttonVariants({ variant: "default", size: "sm" })}
            >
                打开完整任务办理
            </Link>
        ) : showSourceActions ? (
            <>
                {!invoiceTask && singlePayment && fileRefund && fileCredit && (
                    <Button
                        id={`${id}-start-credit`}
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={!canStart}
                        onClick={() =>
                            beginRefund(
                                singlePayment.row,
                                singlePayment.allocated,
                                "SUPPLIER_CREDIT",
                            )
                        }
                    >
                        提交留抵审批
                    </Button>
                )}
                <Button
                    id={`${id}-start`}
                    type="button"
                    variant="default"
                    size="sm"
                    disabled={!canStart}
                    onClick={startPrimary}
                >
                    {primaryLabel}
                </Button>
            </>
        ) : null

    return (
        <>
            <WorkspaceProcessingPane
                archetype="action"
                className="@container/return-finance"
                idPrefix={id}
                aria-label={title}
                contextRail={
                    isWide
                        ? {
                              collapsible: true,
                              resizable: true,
                              allowOnFullWidth: true,
                              defaultWidthPercent: 38,
                              minWidthPercent: 26,
                              maxWidthPercent: 52,
                              storageKey:
                                  "erp.workspace.returnFinanceRailWidth",
                              collapsed: !referenceOpen,
                              onCollapsedChange: (collapsed) =>
                                  setReferenceOpen(!collapsed),
                              label: "对照采购单",
                              content: order.data ? (
                                  <div className="flex h-full min-h-0 flex-col bg-card">
                                      <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border/60 py-2">
                                          <div className="min-w-0">
                                              <h2 className="truncate text-body-compact font-semibold">
                                                  来源采购单对照
                                              </h2>
                                              {purchaseNo ? (
                                                  <p className="num truncate text-xs text-muted-foreground">
                                                      {purchaseNo}
                                                  </p>
                                              ) : null}
                                          </div>
                                          <Button
                                              id={`${id}-rail-close`}
                                              type="button"
                                              size="icon-sm"
                                              variant="ghost"
                                              aria-label="收起采购单对照栏"
                                              onClick={() =>
                                                  setReferenceOpen(false)
                                              }
                                          >
                                              <XIcon aria-hidden="true" />
                                          </Button>
                                      </div>
                                      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain py-3">
                                          <PaperDocumentViewport
                                              id={`${id}-purchase-paper`}
                                              fitMode="width"
                                              fitKey={item.businessObjectId}
                                              role="region"
                                              tabIndex={0}
                                              aria-label="来源采购单纸面"
                                              className="bg-card p-0"
                                          >
                                              <PurchaseOrderPaperDocument
                                                  order={order.data}
                                              />
                                          </PaperDocumentViewport>
                                      </div>
                                  </div>
                              ) : null,
                          }
                        : undefined
                }
                header={
                    <WorkspaceTaskIdentityHeader
                        compact
                        item={item}
                        title={title}
                        subtitle={
                            <DetailWorkspaceMeta
                                items={[
                                    supplier &&
                                    supplier !== title &&
                                    supplier !== purchaseNo
                                        ? supplier
                                        : null,
                                    purchaseNo && purchaseNo !== title ? (
                                        <span
                                            key="purchase-number"
                                            className="num select-all"
                                            data-workspace-document-number={
                                                purchaseNo
                                            }
                                        >
                                            {purchaseNo}
                                        </span>
                                    ) : null,
                                    taskLabel &&
                                    ![title, purchaseNo, supplier].includes(
                                        taskLabel,
                                    )
                                        ? taskLabel
                                        : null,
                                    item.dueAt && item.dueAtLabel ? (
                                        <span key="due" className="num">
                                            任务截止 {item.dueAtLabel}
                                        </span>
                                    ) : null,
                                ]}
                            />
                        }
                    >
                        {inSheet ? (
                            orderLink("purchase-reference-link", "对照采购单")
                        ) : (
                            <Button
                                id={`${id}-purchase-reference`}
                                type="button"
                                size="sm"
                                variant={
                                    referenceOpen && isWide
                                        ? "secondary"
                                        : "ghost"
                                }
                                aria-pressed={
                                    isWide ? referenceOpen : undefined
                                }
                                aria-haspopup={isWide ? undefined : "dialog"}
                                disabled={isWide && !order.data}
                                onClick={openReference}
                            >
                                {referenceOpen && isWide ? (
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
                                {referenceOpen && isWide
                                    ? "收起采购单"
                                    : "对照采购单"}
                            </Button>
                        )}
                    </WorkspaceTaskIdentityHeader>
                }
            >
                <div
                    ref={canvas}
                    className="@container/return-finance min-w-0 space-y-4 bg-card py-3"
                >
                    {result && result.status !== "succeeded" && (
                        <ReturnFinanceNotice
                            tone={
                                result.status === "unknown"
                                    ? "warning"
                                    : "destructive"
                            }
                            title={result.title}
                            detail={result.description}
                            action={
                                result.status === "unknown" ? (
                                    <Button
                                        id={`${id}-recover`}
                                        type="button"
                                        size="xs"
                                        variant="ghost"
                                        onClick={() =>
                                            jumpTo(
                                                invoiceTask
                                                    ? `${invoiceDialogId}-recover`
                                                    : "supplier-payables-refund-request-submit",
                                            )
                                        }
                                    >
                                        核对提交结果
                                    </Button>
                                ) : (
                                    refreshButton("submit-retry")
                                )
                            }
                        />
                    )}
                    {blocker}
                    {loading ? (
                        <p
                            role="status"
                            className="flex items-center gap-2 text-xs text-muted-foreground"
                        >
                            <Spinner aria-hidden="true" className="size-3.5" />
                            正在读取退货和款票依据…
                        </p>
                    ) : null}
                    {!loading && !readError && (
                        <>
                            <ReturnFinanceSummary
                                idPrefix={id}
                                invoiceTask={invoiceTask}
                                pending={pending}
                                total={
                                    invoiceTask
                                        ? sumFixed(
                                              invoiceSources.map(
                                                  ({ applied }) => applied,
                                              ),
                                              decimal,
                                          )
                                        : sumFixed(
                                              [
                                                  decision?.refund_amount ??
                                                      "0",
                                                  decision?.credit_amount ??
                                                      "0",
                                              ],
                                              decimal,
                                          )
                                }
                                settled={
                                    invoiceTask
                                        ? sumFixed(
                                              invoiceSources.map(
                                                  ({ applied, remaining }) =>
                                                      subtractFixed(
                                                          applied,
                                                          remaining,
                                                          decimal,
                                                      ),
                                              ),
                                              decimal,
                                          )
                                        : sumFixed(
                                              currentSettlements
                                                  .filter(
                                                      (row) =>
                                                          row.status ===
                                                          "posted",
                                                  )
                                                  .map((row) => row.amount),
                                              decimal,
                                          )
                                }
                                balances={balances}
                                physicalPending={
                                    plan ? progress?.physicalPending : undefined
                                }
                                decisionStatus={
                                    decisionCurrent
                                        ? {
                                              label: "方案已确认",
                                              tone: "success",
                                          }
                                        : {
                                              label: decision
                                                  ? "方案待更新"
                                                  : "方案待确认",
                                              tone: "warning",
                                          }
                                }
                                targetGross={plan?.target_gross}
                                returnNumbers={plan?.return_numbers ?? []}
                                confirmation={decision?.supplier_confirmation}
                                refundPlan={
                                    decision && !invoiceTask
                                        ? [
                                              {
                                                  label: "方案现金退款",
                                                  value: decision.refund_amount,
                                              },
                                              {
                                                  label: "方案留抵",
                                                  value: decision.credit_amount,
                                              },
                                          ]
                                        : []
                                }
                            />
                            {result?.status === "succeeded" && (
                                <p
                                    role="status"
                                    className="text-xs text-success-soft-foreground"
                                >
                                    {result.title}
                                    {result.reference
                                        ? ` · ${result.reference}`
                                        : ""}
                                </p>
                            )}
                            <ReturnFinanceTable
                                ref={sourceTableRef}
                                id={sourceTableId}
                                title={invoiceTask ? "来源蓝票" : "来源付款"}
                                amountLabel={
                                    invoiceTask
                                        ? "本采购原分配"
                                        : "本采购已分配"
                                }
                                balanceLabel={
                                    invoiceTask ? "本采购可冲减" : undefined
                                }
                                showActions={showSourceActions}
                                emptyText={
                                    invoiceTask
                                        ? "暂无关联蓝票。"
                                        : "暂无关联付款。"
                                }
                                rows={invoiceTask ? invoiceRows : paymentRows}
                            />
                            {settlementRows.length > 0 && (
                                <ReturnFinanceTable
                                    ref={historyTableRef}
                                    id={historyTableId}
                                    title="款项办理记录"
                                    amountLabel="办理金额"
                                    emptyText="暂无款项办理记录。"
                                    rows={settlementRows}
                                />
                            )}
                            <WorkspaceTaskFooter
                                summary={footerSummary}
                                fallback={
                                    <div className="flex w-full flex-wrap items-center justify-between gap-x-3 gap-y-2">
                                        {footerSummary}
                                        {footerActions && (
                                            <div className="ml-auto flex flex-wrap items-center justify-end gap-2">
                                                {footerActions}
                                            </div>
                                        )}
                                    </div>
                                }
                            >
                                {footerActions}
                            </WorkspaceTaskFooter>
                        </>
                    )}
                </div>
            </WorkspaceProcessingPane>
            {referenceOpen &&
                !isWide &&
                !inSheet &&
                !invoice &&
                !flow.refundRequest && (
                    <WorkspaceDocumentPaperDialog
                        open
                        target={{
                            kind: "purchase_order",
                            objectId: item.businessObjectId,
                            title: purchaseNo || "采购单",
                        }}
                        closeButtonId={`${id}-paper-close`}
                        onOpenChange={setReferenceOpen}
                    />
                )}
            {!inSheet && flow.refundRequest && (
                <SupplierRefundRequestDialog
                    open
                    pending={flow.refundSubmitPending}
                    settlementMethod={flow.refundRequest.settlementMethod}
                    sourceLabel={flow.refundRequest.sourcePaymentNo}
                    amount={flow.refundRequest.amount}
                    onOpenChange={(open) => {
                        if (!open) flow.setRefundRequest(null)
                    }}
                    onSubmit={handleRefundSubmit}
                />
            )}
            {!inSheet && invoice && (
                <ReturnInvoiceDialog
                    idPrefix={invoiceDialogId}
                    invoice={invoice.row}
                    payableAccountId={invoice.accountId}
                    amount={invoiceLimit}
                    refreshing={refreshing}
                    onRefresh={() => void refresh()}
                    onClose={() => setInvoice(null)}
                    onResult={setResult}
                    onSucceeded={refreshAfterSuccess}
                />
            )}
        </>
    )
}

function ReturnFinanceNotice({
    tone,
    title,
    detail,
    action,
}: {
    tone: "warning" | "destructive"
    title: string
    detail: string
    action: React.ReactNode
}) {
    return (
        <div
            role={tone === "destructive" ? "alert" : "status"}
            className={cn(
                "flex flex-wrap items-center justify-between gap-x-3 gap-y-1.5 rounded-r-md border-l-2 px-3 py-2 text-xs",
                tone === "destructive"
                    ? "border-l-destructive bg-destructive-soft text-destructive-soft-foreground"
                    : "border-l-warning bg-warning-soft text-warning-soft-foreground",
            )}
        >
            <div className="flex min-w-0 flex-1 items-start gap-2 wrap-anywhere">
                <TriangleAlertIcon
                    aria-hidden="true"
                    className="mt-0.5 size-3.5 shrink-0"
                />
                <div className="min-w-0 flex-1">
                    <p className="font-semibold">{title}</p>
                    <p>{detail}</p>
                </div>
            </div>
            {action}
        </div>
    )
}

function positive(amount: string) {
    return compareDecimal(amount, "0", 2) > 0
}

function outstanding(amount: string | undefined, allocated: string[]) {
    return clampZeroFixed(
        subtractFixed(amount ?? "0", sumFixed(allocated, decimal), decimal),
        decimal,
    )
}

/** 按原分配关联红票，保留已冲完的蓝票供核对。 */
function invoiceBalances(
    invoices: readonly PurchaseInvoiceRow[],
    accountId: string,
) {
    const reversedByAllocation = new Map<string, string[]>()
    for (const { allocations } of invoices) {
        for (const line of allocations) {
            if (line.action !== "REVERSE" || !line.reverseOfAllocationId)
                continue
            const amounts = reversedByAllocation.get(line.reverseOfAllocationId)
            if (amounts) amounts.push(line.amountGross)
            else
                reversedByAllocation.set(line.reverseOfAllocationId, [
                    line.amountGross,
                ])
        }
    }

    return invoices.flatMap((row) => {
        if (row.invoiceKind !== "BLUE") return []
        const lines = row.allocations.filter(
            (line) =>
                line.action === "APPLY" && line.payableAccountId === accountId,
        )
        if (lines.length === 0) return []
        const applied = sumFixed(
            lines.map((line) => line.amountGross),
            decimal,
        )
        const reversed = lines.flatMap(
            (line) => reversedByAllocation.get(line.allocationId) ?? [],
        )
        return [
            {
                row,
                applied,
                remaining:
                    row.status === "REVERSED"
                        ? "0.00"
                        : outstanding(applied, reversed),
            },
        ]
    })
}

function paymentBalances(payments: readonly PaymentRow[], accountId: string) {
    return payments.flatMap((row) => {
        if (row.status !== "POSTED") return []
        const allocated = clampZeroFixed(
            sumFixed(
                row.allocations
                    .filter((line) => line.payableAccountId === accountId)
                    .map((line) =>
                        line.action === "REVERSE"
                            ? `-${line.amount}`
                            : line.amount,
                    ),
                { ...decimal, allowNegative: true },
            ),
            decimal,
        )
        return positive(allocated) ? [{ row, allocated }] : []
    })
}
