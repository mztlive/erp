"use client"
import { useEffect, useRef, useState } from "react"
import Link from "next/link"
import { WorkspaceTaskPane, WorkspaceTaskFooter } from "@/components/business"
import { MoneyValue } from "@/components/business/values"
import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api"
import {
    compareDecimal,
    minFixed,
    subtractFixed,
    sumFixed,
    clampZeroFixed,
} from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { usePurchaseOrderCenterQuery } from "@/features/purchase-orders/hooks/queries"
import { sourceReturnProgress } from "@/features/purchase-orders/lib/source-return-progress"
import { useReturnSourcesQuery } from "@/features/supplier-payables/hooks/queries"
import { useSupplierRefundFlow } from "@/features/supplier-payables/pages/hooks/use-supplier-refund-flow"
import { SupplierRefundRequestDialog } from "@/features/supplier-payables/components/supplier-refund-request-dialog"
import type {
    FormalSubmitResult,
    PurchaseInvoiceRow,
} from "@/features/supplier-payables/types"
import { useWorkItemDetailQuery } from "@/features/work-items/queries"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"
import { ReturnInvoiceDialog } from "./return-invoice-dialog"
import { ReturnFinanceSummary } from "./return-finance-summary"

const decimal = { maxScale: 2, outputScale: 2 }
export function WorkspaceReturnFinance({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (id: string) => void
}) {
    const invoiceTask = item.workItemType === "PURCHASE_RETURN_INVOICE"
    const order = usePurchaseOrderCenterQuery(item.businessObjectId)
    const task = useWorkItemDetailQuery(item.workItemId)
    const sources = useReturnSourcesQuery(
        order.data?.header.supplierId,
        invoiceTask,
    )
    const [result, setResult] = useState<FormalSubmitResult | null>(null)
    const [invoice, setInvoice] = useState<PurchaseInvoiceRow | null>(null)
    const notified = useRef(false)
    const flow = useSupplierRefundFlow({
        openRefundPreview: () => undefined,
        setLastResult: setResult,
        setActionError: (message) =>
            setResult({
                status: "failed",
                title: "提交失败",
                description: message,
            }),
    })
    useEffect(() => {
        if (
            task.data?.status === "COMPLETED" &&
            !task.isFetching &&
            !notified.current
        ) {
            notified.current = true
            onTaskCompleted?.(item.workItemId)
        }
    }, [task.data?.status, task.isFetching, item.workItemId, onTaskCompleted])
    const progress = order.data ? sourceReturnProgress(order.data) : null
    const plan = progress?.plan
    const decision = plan?.settlement_decision
    const current =
        !!decision && progress?.decisionCurrent && !progress.physicalPending
    const balance = (method: "CASH_REFUND" | "SUPPLIER_CREDIT") =>
        clampZeroFixed(
            subtractFixed(
                method === "CASH_REFUND"
                    ? (decision?.refund_amount ?? "0")
                    : (decision?.credit_amount ?? "0"),
                sumFixed(
                    (plan?.settlements ?? [])
                        .filter(
                            (row) =>
                                row.method === method &&
                                row.decision_key === decision?.idempotency_key,
                        )
                        .map((row) => row.amount),
                    decimal,
                ),
                decimal,
            ),
            decimal,
        )
    const refund = balance("CASH_REFUND"),
        credit = balance("SUPPLIER_CREDIT")
    const availableInvoices = (sources.data?.invoices ?? []).filter(
        (row) =>
            row.invoiceKind === "BLUE" &&
            row.status !== "REVERSED" &&
            compareDecimal(
                invoiceRemaining(
                    row,
                    sources.data?.invoices ?? [],
                    plan?.payable_account_id,
                ),
                "0",
                2,
            ) > 0,
    )
    const busy = flow.refundSubmitPending
    return (
        <WorkspaceTaskPane
            header={
                <WorkspaceTaskIdentityHeader
                    item={item}
                    title={invoiceTask ? "退货进项红票" : "退货款处理"}
                    subtitle={
                        <span className="flex flex-wrap gap-x-3 gap-y-1">
                            <span>{item.counterpartyName}</span>
                            <Link
                                id="return-finance-purchase"
                                href={`/procurement/orders/${encodeURIComponent(item.businessObjectId)}`}
                                className="underline underline-offset-4"
                            >
                                查看来源采购单
                            </Link>
                        </span>
                    }
                />
            }
        >
            <div className="flex min-h-0 flex-1 flex-col">
                <div className="flex-1 space-y-5 overflow-y-auto px-7 py-6">
                    {order.isPending ||
                    (sources.isPending && !order.isError) ? (
                        <p className="text-sm text-muted-foreground">
                            正在读取退货和款票依据…
                        </p>
                    ) : order.isError || sources.isError ? (
                        <p role="alert" className="text-sm text-destructive">
                            {getErrorMessage(
                                order.error ?? sources.error,
                                "读取失败，请刷新后重试",
                            )}
                        </p>
                    ) : (
                        <>
                            {!current ? (
                                <p
                                    role="alert"
                                    className="text-sm text-destructive"
                                >
                                    当前退货或款项方案尚不满足办理条件。请联系采购负责人核对实际退货和处理方案，再刷新办理。
                                </p>
                            ) : (
                                <>
                                    <ReturnFinanceSummary
                                        invoiceTask={invoiceTask}
                                        refund={refund}
                                        credit={credit}
                                        redInvoice={clampZeroFixed(
                                            progress?.redInvoice ?? "0",
                                            decimal,
                                        )}
                                        decision={decision}
                                    />
                                    {(plan?.settlements ?? []).length > 0 ? (
                                        <h3 className="text-sm font-medium">
                                            已提交的款项及办理状态
                                        </h3>
                                    ) : null}
                                    {(plan?.settlements ?? []).map((row) => (
                                        <div
                                            key={row.id}
                                            className="flex flex-wrap justify-between gap-2 text-sm"
                                        >
                                            <span>
                                                {row.method ===
                                                "SUPPLIER_CREDIT"
                                                    ? "留抵"
                                                    : "退款"}{" "}
                                                <MoneyValue
                                                    value={row.amount}
                                                />
                                            </span>
                                            <Link
                                                id={`return-finance-record-${toAutomationIdSegment(row.id)}`}
                                                href={`/finance/supplier-accounts?view=payment&previewKind=refund&detailId=${encodeURIComponent(row.id)}`}
                                                className="underline underline-offset-4"
                                            >
                                                {row.status === "posted"
                                                    ? "已入账"
                                                    : "审批中"}{" "}
                                                · {row.number}
                                            </Link>
                                        </div>
                                    ))}
                                    {invoiceTask ? (
                                        <div className="divide-y">
                                            {availableInvoices.map((row) => (
                                                <div
                                                    key={row.invoiceId}
                                                    className="flex flex-wrap items-center justify-between gap-3 py-3 text-sm"
                                                >
                                                    <div className="space-y-1">
                                                        <p>
                                                            原蓝票：
                                                            {row.invoiceNo}
                                                        </p>
                                                        <p className="text-muted-foreground">
                                                            原票总额{" "}
                                                            <MoneyValue
                                                                value={
                                                                    row.grossAmount
                                                                }
                                                            />
                                                        </p>
                                                        <p>
                                                            本采购剩余可冲减{" "}
                                                            <MoneyValue
                                                                value={invoiceRemaining(
                                                                    row,
                                                                    sources.data
                                                                        ?.invoices ??
                                                                        [],
                                                                    plan?.payable_account_id,
                                                                )}
                                                            />
                                                        </p>
                                                    </div>
                                                    <Button
                                                        id={`return-finance-invoice-${toAutomationIdSegment(row.invoiceId)}`}
                                                        variant="outline"
                                                        disabled={
                                                            compareDecimal(
                                                                progress?.redInvoice ??
                                                                    "0",
                                                                "0",
                                                                2,
                                                            ) <= 0
                                                        }
                                                        onClick={() =>
                                                            setInvoice(row)
                                                        }
                                                    >
                                                        登记进项红票
                                                    </Button>
                                                </div>
                                            ))}
                                        </div>
                                    ) : (
                                        <div className="divide-y">
                                            {sources.data?.payments
                                                .filter(
                                                    (row) =>
                                                        row.status === "POSTED",
                                                )
                                                .map((row) => {
                                                    const allocated =
                                                        clampZeroFixed(
                                                            sumFixed(
                                                                row.allocations
                                                                    .filter(
                                                                        (
                                                                            line,
                                                                        ) =>
                                                                            line.payableAccountId ===
                                                                            plan?.payable_account_id,
                                                                    )
                                                                    .map(
                                                                        (
                                                                            line,
                                                                        ) =>
                                                                            line.action ===
                                                                            "REVERSE"
                                                                                ? `-${line.amount}`
                                                                                : line.amount,
                                                                    ),
                                                                decimal,
                                                            ),
                                                            decimal,
                                                        )
                                                    if (
                                                        compareDecimal(
                                                            allocated,
                                                            "0",
                                                            2,
                                                        ) <= 0
                                                    )
                                                        return null
                                                    const begin = (
                                                        method:
                                                            | "CASH_REFUND"
                                                            | "SUPPLIER_CREDIT",
                                                        amount: string,
                                                    ) =>
                                                        flow.setRefundRequest({
                                                            supplierId:
                                                                row.supplierId,
                                                            supplierName:
                                                                row.supplierName,
                                                            sourcePaymentId:
                                                                row.paymentId,
                                                            sourcePaymentNo:
                                                                row.paymentNo,
                                                            sourcePurchaseOrderId:
                                                                item.businessObjectId,
                                                            settlementDecisionKey:
                                                                decision.idempotency_key,
                                                            settlementMethod:
                                                                method,
                                                            amount: minFixed(
                                                                amount,
                                                                allocated,
                                                                decimal,
                                                            ),
                                                        })
                                                    return (
                                                        <div
                                                            key={row.paymentId}
                                                            className="flex flex-wrap items-center justify-between gap-3 py-3 text-sm"
                                                        >
                                                            <div>
                                                                <p>
                                                                    {
                                                                        row.paymentNo
                                                                    }
                                                                </p>
                                                                <p className="mt-1 text-muted-foreground">
                                                                    分配到本采购{" "}
                                                                    <MoneyValue
                                                                        value={
                                                                            allocated
                                                                        }
                                                                    />
                                                                </p>
                                                            </div>
                                                            <div className="flex gap-2">
                                                                {compareDecimal(
                                                                    refund,
                                                                    "0",
                                                                    2,
                                                                ) > 0 && (
                                                                    <Button
                                                                        id={`return-finance-refund-${toAutomationIdSegment(row.paymentId)}`}
                                                                        variant="outline"
                                                                        disabled={
                                                                            busy
                                                                        }
                                                                        onClick={() =>
                                                                            begin(
                                                                                "CASH_REFUND",
                                                                                refund,
                                                                            )
                                                                        }
                                                                    >
                                                                        登记实际退款
                                                                    </Button>
                                                                )}
                                                                {compareDecimal(
                                                                    credit,
                                                                    "0",
                                                                    2,
                                                                ) > 0 && (
                                                                    <Button
                                                                        id={`return-finance-credit-${toAutomationIdSegment(row.paymentId)}`}
                                                                        variant="outline"
                                                                        disabled={
                                                                            busy
                                                                        }
                                                                        onClick={() =>
                                                                            begin(
                                                                                "SUPPLIER_CREDIT",
                                                                                credit,
                                                                            )
                                                                        }
                                                                    >
                                                                        提交留抵审批
                                                                    </Button>
                                                                )}
                                                            </div>
                                                        </div>
                                                    )
                                                })}
                                        </div>
                                    )}
                                    {invoiceTask &&
                                        availableInvoices.length === 0 &&
                                        compareDecimal(
                                            progress?.redInvoice ?? "0",
                                            "0",
                                            2,
                                        ) > 0 && (
                                            <p
                                                role="status"
                                                className="text-sm text-muted-foreground"
                                            >
                                                未找到本采购可红冲的蓝票，请核对原票分配或联系财务负责人后刷新。
                                            </p>
                                        )}
                                    {!invoiceTask &&
                                        compareDecimal(refund, "0", 2) === 0 &&
                                        compareDecimal(credit, "0", 2) ===
                                            0 && (
                                            <p className="text-sm text-muted-foreground">
                                                {(plan?.settlements ?? []).some(
                                                    (row) =>
                                                        row.status ===
                                                        "in_approval",
                                                )
                                                    ? "当前方案的款项已提交，请等待审批及入账，无需重复登记。"
                                                    : progress?.refundPending
                                                      ? "当前没有可提交的方案余额，请刷新核对款项结果；仍有差额时联系采购负责人核对方案。"
                                                      : "本采购退货款差额已处理，请刷新待办状态。"}
                                            </p>
                                        )}
                                </>
                            )}
                        </>
                    )}
                    {result && (
                        <div
                            role="status"
                            className="rounded-md border p-3 text-sm"
                        >
                            <p className="font-medium">{result.title}</p>
                            <p className="mt-1 text-muted-foreground">
                                {result.description}
                            </p>
                        </div>
                    )}
                </div>
                <WorkspaceTaskFooter>
                    <Button
                        id="return-finance-refresh"
                        variant="outline"
                        disabled={
                            order.isFetching || sources.isFetching || busy
                        }
                        onClick={() =>
                            void Promise.all([
                                order.refetch(),
                                sources.refetch(),
                                task.refetch(),
                            ])
                        }
                    >
                        刷新处理结果
                    </Button>
                </WorkspaceTaskFooter>
            </div>
            {flow.refundRequest && (
                <SupplierRefundRequestDialog
                    open
                    pending={busy}
                    settlementMethod={flow.refundRequest.settlementMethod}
                    sourceLabel={flow.refundRequest.sourcePaymentNo}
                    amount={flow.refundRequest.amount}
                    onOpenChange={(open) => {
                        if (!open) flow.setRefundRequest(null)
                    }}
                    onSubmit={flow.prepareRefundDraft}
                />
            )}
            {invoice && plan?.payable_account_id && (
                <ReturnInvoiceDialog
                    invoice={invoice}
                    payableAccountId={plan.payable_account_id}
                    amount={minFixed(
                        clampZeroFixed(progress?.redInvoice ?? "0", decimal),
                        invoiceRemaining(
                            invoice,
                            sources.data?.invoices ?? [],
                            plan.payable_account_id,
                        ),
                        decimal,
                    )}
                    onClose={() => setInvoice(null)}
                    onResult={setResult}
                />
            )}
        </WorkspaceTaskPane>
    )
}

/** 原蓝票的本采购分配减去所有关联红票反向分配。 */
function invoiceRemaining(
    invoice: PurchaseInvoiceRow,
    invoices: readonly PurchaseInvoiceRow[],
    accountId?: string | null,
) {
    const applied = invoice.allocations.filter(
        (line) =>
            line.action === "APPLY" && line.payableAccountId === accountId,
    )
    const ids = new Set(applied.map((line) => line.allocationId))
    const reversed = invoices
        .flatMap((row) => row.allocations)
        .filter(
            (line) =>
                line.action === "REVERSE" &&
                !!line.reverseOfAllocationId &&
                ids.has(line.reverseOfAllocationId),
        )
    return clampZeroFixed(
        subtractFixed(
            sumFixed(
                applied.map((line) => line.amountGross),
                decimal,
            ),
            sumFixed(
                reversed.map((line) => line.amountGross),
                decimal,
            ),
            decimal,
        ),
        decimal,
    )
}
