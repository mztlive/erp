"use client"

import { CustomerSourceDocuments } from "./customer-source-documents"
import { MoneyValue } from "@/components/business"
import {
    PreviewAmount,
    PreviewSection,
    PreviewFact,
    PreviewNote,
} from "@/components/business/financial-preview"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { ApprovalReadonly } from "@/features/approval-workflow/components/approval-readonly"
import { CustomerReceiptApprovalArea } from "@/features/customer-receivables/components/customer-receipt-approval-area"
import { CustomerRefundApprovalArea } from "@/features/customer-receivables/components/customer-refund-approval-area"
import { ReceiptReversalApprovalArea } from "@/features/customer-receivables/components/receipt-reversal-approval-area"
import { customerReceiptApprovalPhase } from "@/features/customer-receivables/lib/customer-receipt-approval"
import { customerRefundApprovalPhase } from "@/features/customer-receivables/lib/customer-refund-approval"
import { receiptReversalApprovalPhase } from "@/features/customer-receivables/lib/receipt-reversal-approval"
import type {
    AllocationLine,
    CustomerRefundRow,
    ReceiptReversalRow,
    ReceiptRow,
    ReceivableAccountRow,
    SalesInvoiceRow,
} from "@/features/customer-receivables/types"
import { formatDateTime } from "@/lib/datetime"

const bodyClassName = "min-h-0 flex-1 space-y-6 overflow-auto px-7 py-6 text-sm"

const receivableEntryLabels: Record<string, string> = {
    original: "原始应收",
    sales_change_delta: "销售变更差额",
    void_reduction: "作废冲减",
    refund: "退款",
    reversal: "冲正",
}

export function ReceivableDetailBody({ row }: { row: ReceivableAccountRow }) {
    return (
        <div className={bodyClassName}>
            <PreviewAmount label="待收金额（含税）" value={row.openTotal}>
                <span>
                    应收总额 <MoneyValue value={row.grossTotal} />
                </span>
                <span>
                    已核销回款 <MoneyValue value={row.settledTotal} />
                </span>
            </PreviewAmount>
            <PreviewSection title="收款安排">
                <dl className="space-y-3">
                    <PreviewFact label="到期日">
                        <span className="num">{row.dueDate}</span>
                        <span className="ml-2 text-xs text-muted-foreground">
                            {row.dueStateLabel}
                        </span>
                    </PreviewFact>
                    {row.customerName !== row.counterpartyPartyName ? (
                        <PreviewFact label="经营归属客户">
                            {row.customerName}
                        </PreviewFact>
                    ) : null}
                    <PreviewFact label="业务性质">
                        {row.businessTypeLabel}
                    </PreviewFact>
                </dl>
            </PreviewSection>
            <PreviewSection title="开票进度">
                <dl className="space-y-3">
                    <PreviewFact label="待开票（含税）">
                        <MoneyValue value={row.openInvoiceableTotal} />
                    </PreviewFact>
                    <PreviewFact label="净已开票（含税）">
                        <MoneyValue value={row.invoicedTotal} />
                    </PreviewFact>
                </dl>
                <PreviewNote>开票与回款独立，已开票不代表已收款。</PreviewNote>
            </PreviewSection>
            <PreviewSection title="应收构成">
                {row.entries.length ? (
                    <ul className="divide-y divide-border">
                        {row.entries.map((entry) => (
                            <li
                                key={entry.entryId}
                                className="py-3 first:pt-0 last:pb-0"
                            >
                                <div className="flex items-baseline justify-between gap-5">
                                    <span>
                                        {receivableEntryLabels[
                                            entry.entryType
                                        ] ?? "应收调整"}
                                    </span>
                                    <span className="shrink-0">
                                        <span className="mr-1 text-muted-foreground">
                                            {entry.direction === "increase"
                                                ? "+"
                                                : "−"}
                                        </span>
                                        <MoneyValue value={entry.amountGross} />
                                    </span>
                                </div>
                                <p className="mt-1 text-xs text-muted-foreground">
                                    {entry.sourceLabel} · 到期{" "}
                                    <span className="num">{entry.dueDate}</span>
                                </p>
                            </li>
                        ))}
                    </ul>
                ) : (
                    <PreviewNote>暂无应收构成记录</PreviewNote>
                )}
            </PreviewSection>
        </div>
    )
}

/**
 * 客户回款详情。草稿展示绑定卡，运行中/终态嵌入通用审批区。
 */
export function ReceiptDetailBody({
    row,
    readOnly = false,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onDecisionApplied,
}: {
    row: ReceiptRow
    readOnly?: boolean
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
}) {
    const posted = row.status === "posted" || row.status === "reversed"
    return (
        <div className={bodyClassName}>
            <PreviewAmount label="待核销回款" value={row.unallocatedAmount}>
                <span>
                    到账金额 <MoneyValue value={row.amount} />
                </span>
                <span>
                    已核销 <MoneyValue value={row.allocatedTotal} />
                </span>
            </PreviewAmount>
            <PreviewSection title="到账信息">
                <dl className="space-y-3">
                    <PreviewFact label="到账时间">
                        <span className="num">
                            {formatDateTime(
                                row.receivedAt,
                                "full",
                                "passthrough",
                            )}
                        </span>
                    </PreviewFact>
                    <PreviewFact label="银行引用">
                        <span className="num">
                            {row.bankReferenceMasked || "—"}
                        </span>
                    </PreviewFact>
                    {row.customerName !== row.counterpartyPartyName ? (
                        <PreviewFact label="经营归属客户">
                            {row.customerName}
                        </PreviewFact>
                    ) : null}
                </dl>
            </PreviewSection>
            <AllocationSection rows={row.allocations} />
            <CustomerSourceDocuments
                scope={{
                    entryIds: row.allocations.map(
                        (allocation) => allocation.targetId,
                    ),
                    counterpartyPartyId: row.counterpartyPartyId,
                }}
            />
            <PreviewSection title="审批记录">
                {readOnly ? (
                    <ApprovalReadonly
                        id={`receipt-${row.receiptId}`}
                        approval={row.approval}
                    />
                ) : (
                    <CustomerReceiptApprovalArea
                        phase={customerReceiptApprovalPhase(
                            row.approval,
                            row.status === "in_approval"
                                ? "IN_APPROVAL"
                                : row.status,
                        )}
                        approval={row.approval}
                        documentId={row.receiptId}
                        workItemId={workItemId}
                        expectedTaskVersion={expectedTaskVersion}
                        workItemAllowedActions={workItemAllowedActions}
                        onDecisionApplied={onDecisionApplied}
                    />
                )}
            </PreviewSection>
            {posted ? (
                <PreviewNote>
                    已过账回款不可编辑或删除；纠错请办理退款或冲正。
                </PreviewNote>
            ) : null}
        </div>
    )
}

/**
 * 客户退款详情。草稿展示绑定卡，运行中/终态嵌入通用审批区。
 */
export function CustomerRefundDetailBody({
    row,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onDecisionApplied,
}: {
    row: CustomerRefundRow
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
}) {
    const posted = row.status === "posted" || row.status === "reversed"
    return (
        <div className={bodyClassName}>
            <PreviewAmount label="退款金额" value={row.amount} />
            <PreviewSection title="原因说明">
                <p className="leading-6">{row.reasonText || "未填写原因"}</p>
                <PreviewNote>
                    退款时间：
                    {formatDateTime(row.occurredAt, "full", "passthrough")}
                </PreviewNote>
            </PreviewSection>
            {!row.originalReceiptId ? (
                <CustomerSourceDocuments
                    scope={{
                        entryIds: row.originalReceivableEntryId
                            ? [row.originalReceivableEntryId]
                            : [],
                        customerId: row.customerId,
                    }}
                />
            ) : null}
            <PreviewSection title="审批记录">
                <CustomerRefundApprovalArea
                    phase={customerRefundApprovalPhase(
                        row.approval,
                        row.status === "in_approval"
                            ? "IN_APPROVAL"
                            : row.status,
                    )}
                    approval={row.approval}
                    documentId={row.refundId}
                    workItemId={workItemId}
                    expectedTaskVersion={expectedTaskVersion}
                    workItemAllowedActions={workItemAllowedActions}
                    onDecisionApplied={onDecisionApplied}
                />
            </PreviewSection>
            {posted ? (
                <PreviewNote>
                    已过账记录不可编辑或删除；纠错须追加反向记录。
                </PreviewNote>
            ) : null}
        </div>
    )
}

/**
 * 回款冲正详情。草稿展示绑定卡，运行中/终态嵌入通用审批区。
 */
export function ReceiptReversalDetailBody({
    row,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onDecisionApplied,
}: {
    row: ReceiptReversalRow
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
}) {
    const posted = row.status === "posted" || row.status === "reversed"
    return (
        <div className={bodyClassName}>
            <PreviewAmount label="冲正金额" value={row.amount} />
            <PreviewSection title="原因说明">
                <p className="leading-6">{row.reasonText || "未填写原因"}</p>
                <PreviewNote>
                    冲正时间：
                    {formatDateTime(row.occurredAt, "full", "passthrough")}
                </PreviewNote>
            </PreviewSection>
            <PreviewSection title="审批记录">
                <ReceiptReversalApprovalArea
                    phase={receiptReversalApprovalPhase(
                        row.approval,
                        row.status === "in_approval"
                            ? "IN_APPROVAL"
                            : row.status,
                    )}
                    approval={row.approval}
                    documentId={row.reversalId}
                    workItemId={workItemId}
                    expectedTaskVersion={expectedTaskVersion}
                    workItemAllowedActions={workItemAllowedActions}
                    onDecisionApplied={onDecisionApplied}
                />
            </PreviewSection>
            {posted ? (
                <PreviewNote>
                    已过账记录不可编辑或删除；纠错须追加反向记录。
                </PreviewNote>
            ) : null}
        </div>
    )
}

/**
 * 发票详情。Invoice 为 NO_APPROVAL，只展示发票事实与分配明细，
 * 不嵌入绑定卡、决定弹窗、撤回或改派入口。
 */
export function InvoiceDetailBody({ row }: { row: SalesInvoiceRow }) {
    return (
        <div className={bodyClassName}>
            <PreviewAmount
                label="待分配金额（含税）"
                value={row.unallocatedAmount}
            >
                <span>
                    发票金额 <MoneyValue value={row.grossAmount} />
                </span>
                <span>
                    已分配 <MoneyValue value={row.allocatedTotal} />
                </span>
            </PreviewAmount>
            <PreviewSection title="发票资料">
                <dl className="space-y-3">
                    <PreviewFact label="开票日期">
                        <span className="num">{row.invoiceDate}</span>
                    </PreviewFact>
                    <PreviewFact label="发票代码">
                        <span className="num">{row.invoiceCode || "—"}</span>
                    </PreviewFact>
                    <PreviewFact label="不含税金额">
                        <MoneyValue value={row.netAmount} />
                    </PreviewFact>
                    <PreviewFact label="税额">
                        <MoneyValue value={row.taxAmount} />
                    </PreviewFact>
                </dl>
            </PreviewSection>
            <AllocationSection rows={row.allocations} />
            <CustomerSourceDocuments
                scope={{
                    accountIds: row.allocations.map(
                        (allocation) => allocation.targetId,
                    ),
                }}
            />
            <PreviewNote>
                开票分配独立于回款。已登记发票不可编辑或删除；纠错请办理红票。
            </PreviewNote>
        </div>
    )
}

function AllocationSection({ rows }: { rows: readonly AllocationLine[] }) {
    return (
        <PreviewSection title="核销去向">
            {rows.length ? (
                <ul className="divide-y divide-border">
                    {rows.map((allocation) => (
                        <li
                            key={allocation.allocationId}
                            className="py-3 first:pt-0 last:pb-0"
                        >
                            <div className="flex items-baseline justify-between gap-5">
                                <span className="min-w-0 break-words">
                                    {allocation.targetLabel}
                                </span>
                                <MoneyValue
                                    value={allocation.amountGross}
                                    className="shrink-0"
                                />
                            </div>
                            <p className="mt-1 text-xs text-muted-foreground">
                                {allocation.action === "REVERSE"
                                    ? "撤销核销"
                                    : "核销"}{" "}
                                ·{" "}
                                <span className="num">
                                    {formatDateTime(
                                        allocation.occurredAt,
                                        "full",
                                        "passthrough",
                                    )}
                                </span>
                            </p>
                        </li>
                    ))}
                </ul>
            ) : (
                <PreviewNote>尚未关联应收单据</PreviewNote>
            )}
        </PreviewSection>
    )
}
