"use client"

import type { ReactNode } from "react"
import {
    DetailHint,
    DetailRecordSection,
    DetailSummary,
    DetailSummaryItem,
} from "@/components/business/detail-presentation"
import {
    BusinessFailureState,
    MoneyValue,
    QuickPreviewSheet,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import {
    InvoiceDetailBody,
    ReceiptDetailBody,
    ReceivableDetailBody,
} from "@/features/customer-receivables/components/detail-bodies"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { useSalesOrderReceivable } from "@/features/sales-orders/hooks/use-sales-order-receivable"
import {
    amountAllocatedToTargets,
    receivableTargetIds,
} from "@/features/sales-orders/lib/sales-order-receivable"
import { sumFixed } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"

/** 本单票款进度与关联记录；所有办理入口留在财务页面或任务作业面。 */
export function ReceivablePanel({ order }: { order: SalesOrderDetailView }) {
    const state = useSalesOrderReceivable(order)
    const { accounts, receipts, invoices, canRead, detail } = state
    const readableAccounts =
        canRead("receivable_account:list") && accounts.isSuccess
            ? accounts.data
            : undefined
    const targetIds = readableAccounts
        ? receivableTargetIds(readableAccounts)
        : undefined
    const total = (
        field:
            | "openTotal"
            | "settledTotal"
            | "invoicedTotal"
            | "openInvoiceableTotal",
    ) =>
        readableAccounts?.length
            ? sumFixed(
                  readableAccounts.map((row) => row[field]),
                  { maxScale: 2, outputScale: 2 },
              )
            : undefined
    const summaryAmount = (value?: string) =>
        value === undefined ? (
            <span className="text-base font-normal text-muted-foreground">
                待确认
            </span>
        ) : (
            <MoneyValue value={value} />
        )
    const receiptsEmpty =
        canRead("customer_receipt:list") &&
        !receipts.isLoading &&
        !receipts.error &&
        receipts.data?.total === 0
    const invoicesEmpty =
        canRead("invoice:list") &&
        !invoices.isLoading &&
        !invoices.error &&
        invoices.data?.total === 0
    const previewButton = (
        kind: "receivable" | "receipt" | "invoice",
        id: string,
        label: string,
    ) => {
        const permission =
            kind === "receivable"
                ? "receivable_account:detail"
                : kind === "receipt"
                  ? "customer_receipt:detail"
                  : "invoice:detail"
        return canRead(permission) ? (
            <Button
                id={`sales-order-${kind}-${toAutomationIdSegment(id)}-view`}
                size="sm"
                variant="secondary"
                aria-label={`查看${label}`}
                onClick={() => state.setPreview({ kind, id })}
            >
                查看
            </Button>
        ) : (
            <span className="text-xs text-muted-foreground">无详情权限</span>
        )
    }
    const rowAmount = (
        allocations: Parameters<typeof amountAllocatedToTargets>[0],
    ) =>
        targetIds ? (
            <MoneyValue
                value={amountAllocatedToTargets(allocations, targetIds)}
            />
        ) : (
            "待确认"
        )
    const previewAccount = detail.data?.receivable
        ? {
              ...detail.data.receivable,
              businessTypeLabel:
                  order.nature === "card_voucher" ? "卡券" : "实物与服务",
          }
        : undefined
    return (
        <div className="space-y-6">
            <DetailSummary label="本单票款摘要">
                <DetailSummaryItem
                    title="收款"
                    label="待收金额"
                    value={summaryAmount(total("openTotal"))}
                    detail={
                        <>
                            已核销回款{" "}
                            <span className="font-medium text-foreground">
                                {summaryAmount(
                                    total("settledTotal") ??
                                        order.receivedAmount,
                                )}
                            </span>
                        </>
                    }
                    hint={
                        <DetailHint
                            id="sales-order-receipt-summary-hint"
                            label="收款金额"
                        >
                            只统计核销到本单的已过账净额。审批中的拟核销金额不计入已核销回款。
                        </DetailHint>
                    }
                />
                <DetailSummaryItem
                    title="开票"
                    label="待开票金额"
                    value={summaryAmount(total("openInvoiceableTotal"))}
                    detail={
                        <>
                            已开票净额{" "}
                            <span className="font-medium text-foreground">
                                {summaryAmount(
                                    total("invoicedTotal") ??
                                        order.invoicedAmount,
                                )}
                            </span>
                        </>
                    }
                    hint={
                        <DetailHint
                            id="sales-order-invoice-summary-hint"
                            label="开票金额"
                        >
                            只统计分配到本单的含税净额，已扣除冲减金额。开票与回款分别统计。
                        </DetailHint>
                    }
                />
            </DetailSummary>
            {state.profile.isError ? (
                <BusinessFailureState
                    title="票款权限读取失败"
                    error={state.profile.error}
                    onRetry={() => void state.profile.refetch()}
                />
            ) : null}
            <div className="divide-y divide-border/70">
                <DetailRecordSection
                    title="应收明细"
                    count={readableAccounts?.length}
                >
                    <ReadState
                        allowed={canRead("receivable_account:list")}
                        pending={state.profile.isPending || accounts.isLoading}
                        error={accounts.error}
                        retry={() => void accounts.refetch()}
                    >
                        {!readableAccounts?.length ? (
                            <p className="text-sm text-muted-foreground">
                                {order.currentRevisionNo == null
                                    ? "本单尚未生效，尚未形成应收。"
                                    : "本单已生效，当前可见范围未查到应收记录。"}
                            </p>
                        ) : (
                            <Table>
                                <TableHeader>
                                    <TableRow>
                                        <TableHead>应收</TableHead>
                                        <TableHead>结算主体</TableHead>
                                        <TableHead>到期日</TableHead>
                                        <TableHead>状态</TableHead>
                                        <TableHead data-align="end">
                                            待收金额
                                        </TableHead>
                                        <TableHead>操作</TableHead>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {readableAccounts.map((row) => (
                                        <TableRow key={row.accountId}>
                                            <TableCell>
                                                子账 #{row.accountSeq}
                                            </TableCell>
                                            <TableCell>
                                                {row.counterpartyPartyName}
                                            </TableCell>
                                            <TableCell className="num">
                                                {row.dueDate || "未约定"}
                                            </TableCell>
                                            <TableCell>
                                                <StatusBadge
                                                    label={row.statusLabel}
                                                    tone={row.statusTone}
                                                />
                                            </TableCell>
                                            <TableCell data-align="end">
                                                <MoneyValue
                                                    value={row.openTotal}
                                                />
                                            </TableCell>
                                            <TableCell>
                                                {previewButton(
                                                    "receivable",
                                                    row.accountId,
                                                    `应收子账${row.accountSeq}`,
                                                )}
                                            </TableCell>
                                        </TableRow>
                                    ))}
                                </TableBody>
                            </Table>
                        )}
                    </ReadState>
                </DetailRecordSection>
                <DetailRecordSection
                    title="回款记录"
                    count={
                        canRead("customer_receipt:list") && !receipts.error
                            ? receipts.data?.total
                            : undefined
                    }
                    compact={receiptsEmpty}
                >
                    <ReadState
                        allowed={canRead("customer_receipt:list")}
                        pending={state.profile.isPending || receipts.isLoading}
                        error={receipts.error}
                        retry={() => void receipts.refetch()}
                    >
                        {!receipts.data?.items.length ? (
                            <p className="text-sm text-muted-foreground">
                                暂无回款记录
                            </p>
                        ) : (
                            <Table>
                                <TableHeader>
                                    <TableRow>
                                        <TableHead>回款单／到账日期</TableHead>
                                        <TableHead>状态／当前审批人</TableHead>
                                        <TableHead data-align="end">
                                            单据到账金额
                                        </TableHead>
                                        <TableHead data-align="end">
                                            本单已核销
                                        </TableHead>
                                        <TableHead data-align="end">
                                            <span className="inline-flex items-center gap-1">
                                                本单拟核销
                                                <DetailHint
                                                    id="sales-order-pending-allocation-hint"
                                                    label="拟核销金额"
                                                >
                                                    审批中回款拟分配给本单的金额，尚未计入已核销回款。
                                                </DetailHint>
                                            </span>
                                        </TableHead>
                                        <TableHead>操作</TableHead>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {receipts.data.items.map((row) => (
                                        <TableRow key={row.receiptId}>
                                            <TableCell>
                                                {row.receiptNo}
                                                <div className="text-xs text-muted-foreground">
                                                    {row.receivedAt.slice(
                                                        0,
                                                        10,
                                                    )}
                                                </div>
                                            </TableCell>
                                            <TableCell>
                                                <StatusBadge
                                                    label={row.statusLabel}
                                                    tone={row.statusTone}
                                                />
                                                <div className="text-xs text-muted-foreground">
                                                    {
                                                        row.approval?.instance
                                                            ?.currentAssigneeName
                                                    }
                                                </div>
                                            </TableCell>
                                            <TableCell data-align="end">
                                                <MoneyValue
                                                    value={row.amount}
                                                />
                                            </TableCell>
                                            <TableCell data-align="end">
                                                {rowAmount(row.allocations)}
                                            </TableCell>
                                            <TableCell data-align="end">
                                                {row.status ===
                                                "in_approval" ? (
                                                    row.pendingAllocations &&
                                                    targetIds ? (
                                                        <MoneyValue
                                                            value={sumFixed(
                                                                row.pendingAllocations
                                                                    .filter(
                                                                        (
                                                                            line,
                                                                        ) =>
                                                                            targetIds.has(
                                                                                line.targetId,
                                                                            ),
                                                                    )
                                                                    .map(
                                                                        (
                                                                            line,
                                                                        ) =>
                                                                            line.amountGross,
                                                                    ),
                                                                {
                                                                    maxScale: 2,
                                                                    outputScale: 2,
                                                                },
                                                            )}
                                                        />
                                                    ) : (
                                                        "待确认"
                                                    )
                                                ) : (
                                                    "—"
                                                )}
                                            </TableCell>
                                            <TableCell>
                                                {previewButton(
                                                    "receipt",
                                                    row.receiptId,
                                                    row.receiptNo,
                                                )}
                                            </TableCell>
                                        </TableRow>
                                    ))}
                                </TableBody>
                            </Table>
                        )}
                        {receipts.data &&
                        (receipts.data.total > 20 || state.receiptPage > 1) ? (
                            <RecordPages
                                id="sales-order-receipts"
                                page={state.receiptPage}
                                total={receipts.data.total}
                                onPage={state.setReceiptPage}
                            />
                        ) : null}
                    </ReadState>
                </DetailRecordSection>
                <DetailRecordSection
                    title="发票记录"
                    count={
                        canRead("invoice:list") && !invoices.error
                            ? invoices.data?.total
                            : undefined
                    }
                    compact={invoicesEmpty}
                >
                    <ReadState
                        allowed={canRead("invoice:list")}
                        pending={state.profile.isPending || invoices.isLoading}
                        error={invoices.error}
                        retry={() => void invoices.refetch()}
                    >
                        {!invoices.data?.items.length ? (
                            <p className="text-sm text-muted-foreground">
                                暂无发票记录
                            </p>
                        ) : (
                            <Table>
                                <TableHeader>
                                    <TableRow>
                                        <TableHead>
                                            发票号码／开票日期
                                        </TableHead>
                                        <TableHead>类型</TableHead>
                                        <TableHead>状态</TableHead>
                                        <TableHead data-align="end">
                                            核到本单
                                        </TableHead>
                                        <TableHead>操作</TableHead>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {invoices.data.items.map((row) => (
                                        <TableRow key={row.invoiceId}>
                                            <TableCell>
                                                {row.invoiceNo}
                                                <div className="text-xs text-muted-foreground">
                                                    {row.invoiceDate}
                                                </div>
                                            </TableCell>
                                            <TableCell>
                                                {row.invoiceKindLabel}
                                            </TableCell>
                                            <TableCell>
                                                <StatusBadge
                                                    label={row.statusLabel}
                                                    tone={row.statusTone}
                                                />
                                            </TableCell>
                                            <TableCell data-align="end">
                                                {rowAmount(row.allocations)}
                                            </TableCell>
                                            <TableCell>
                                                {previewButton(
                                                    "invoice",
                                                    row.invoiceId,
                                                    row.invoiceNo,
                                                )}
                                            </TableCell>
                                        </TableRow>
                                    ))}
                                </TableBody>
                            </Table>
                        )}
                        {invoices.data &&
                        (invoices.data.total > 20 || state.invoicePage > 1) ? (
                            <RecordPages
                                id="sales-order-invoices"
                                page={state.invoicePage}
                                total={invoices.data.total}
                                onPage={state.setInvoicePage}
                            />
                        ) : null}
                    </ReadState>
                </DetailRecordSection>
            </div>
            <QuickPreviewSheet
                id="sales-order-finance-preview"
                open={state.preview != null}
                onOpenChange={(open) => {
                    if (!open) state.setPreview(null)
                }}
                title={
                    previewAccount
                        ? `应收子账 #${previewAccount.accountSeq}`
                        : (detail.data?.receipt?.receiptNo ??
                          detail.data?.invoice?.invoiceNo ??
                          "票款记录")
                }
                size="detail"
                footer={
                    <Button
                        id="sales-order-finance-preview-close"
                        variant="outline"
                        onClick={() => state.setPreview(null)}
                    >
                        关闭
                    </Button>
                }
            >
                {detail.isPending ? (
                    <p role="status" className="p-7">
                        正在加载记录…
                    </p>
                ) : detail.isError ? (
                    <BusinessFailureState
                        title="票款详情加载失败"
                        error={detail.error}
                        onRetry={() => void detail.refetch()}
                    />
                ) : previewAccount ? (
                    <ReceivableDetailBody row={previewAccount} />
                ) : detail.data?.receipt ? (
                    <ReceiptDetailBody row={detail.data.receipt} readOnly />
                ) : detail.data?.invoice ? (
                    <InvoiceDetailBody row={detail.data.invoice} />
                ) : (
                    <p className="p-7">未找到可查看的记录。</p>
                )}
            </QuickPreviewSheet>
        </div>
    )
}

function ReadState({
    allowed,
    pending,
    error,
    retry,
    children,
}: {
    allowed: boolean
    pending: boolean
    error: unknown
    retry: () => void
    children: ReactNode
}) {
    if (pending)
        return (
            <p role="status" className="text-sm text-muted-foreground">
                正在加载…
            </p>
        )
    if (!allowed)
        return (
            <p className="text-sm text-muted-foreground">
                当前账号无此类明细查看权限，以上销售单摘要仍可查阅。
            </p>
        )
    if (error)
        return (
            <BusinessFailureState
                title="记录加载失败"
                error={error}
                onRetry={retry}
            />
        )
    return children
}

function RecordPages({
    id,
    page,
    total,
    onPage,
}: {
    id: string
    page: number
    total: number
    onPage: (page: number) => void
}) {
    return (
        <div className="mt-3 flex items-center justify-end gap-3 text-xs text-muted-foreground">
            <span>共 {total} 笔</span>
            {total > 20 || page > 1 ? (
                <>
                    <Button
                        id={`${id}-previous`}
                        size="sm"
                        variant="outline"
                        disabled={page === 1}
                        onClick={() => onPage(page - 1)}
                    >
                        上一页
                    </Button>
                    <span>第 {page} 页</span>
                    <Button
                        id={`${id}-next`}
                        size="sm"
                        variant="outline"
                        disabled={page * 20 >= total}
                        onClick={() => onPage(page + 1)}
                    >
                        下一页
                    </Button>
                </>
            ) : null}
        </div>
    )
}
