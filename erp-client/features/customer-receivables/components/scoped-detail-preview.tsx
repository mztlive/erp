"use client"

import type { ComponentProps } from "react"
import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"

import {
    BusinessStatusBadge,
    LimitedBadge,
    MoneyValue,
    QuickPreviewSheet,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { PermissionGuard } from "@/features/auth/components/permission-guard"
import { instantToIso } from "@/features/customer-receivables/api/mappers"
import type { ReceivableScopeDetail } from "@/features/customer-receivables/api/scoped"
import {
    PreviewAmount,
    PreviewFact,
    PreviewNote,
    PreviewSection,
} from "@/components/business/financial-preview"
import { formatDateTime } from "@/lib/datetime"
import { scopeText } from "@/lib/ui-text"
import { getErrorMessage } from "@/lib/api/errors"
import {
    businessLabelOrPlaceholder,
    MISSING_SALES_ORDER_NO,
} from "../lib/display-labels"

type Props = Readonly<{
    open: boolean
    data?: ReceivableScopeDetail | null
    scopeSummary?: string
    isPending: boolean
    isError: boolean
    error: unknown
    onRetry: () => void
    onClose: () => void
    finalFocus?: ComponentProps<typeof QuickPreviewSheet>["finalFocus"]
}>

const bodyClassName = "space-y-6 px-7 py-6 text-body-compact"

/** 范围详情抽屉：获授权份额常显，整单与未分配受限时为空并注明。 */
export function ReceivableScopeDetailPreview({
    open,
    data,
    scopeSummary,
    isPending,
    isError,
    error,
    onRetry,
    onClose,
    finalFocus,
}: Props) {
    const title =
        data?.kind === "receivable"
            ? `应收款项 #${data.receivable.account_seq}`
            : data?.kind === "receipt"
              ? `回款单 ${data.receipt.receipt_no}`
              : data?.kind === "invoice"
                ? `发票 ${data.invoice.invoice_no ?? "附件登记"}`
                : "往来详情"
    const identity =
        data?.kind === "receivable"
            ? `销售单 ${businessLabelOrPlaceholder(
                  data.receivable.sales_order_no,
                  data.receivable.sales_order_id,
                  MISSING_SALES_ORDER_NO,
              )}`
            : data?.kind === "receipt"
              ? `到账 ${formatDateTime(instantToIso(data.receipt.received_at))}`
              : data?.kind === "invoice"
                ? data.invoice.invoice_date
                    ? `开票 ${data.invoice.invoice_date}`
                    : "票面信息以附件为准"
                : undefined
    const limited =
        data?.kind === "receivable"
            ? data.receivable.permission_limited
            : data?.kind === "receipt"
              ? data.receipt.permission_limited
              : data?.kind === "invoice"
                ? data.invoice.permission_limited
                : false

    return (
        <QuickPreviewSheet
            id="customer-receivables-scope-preview-sheet"
            open={open}
            onOpenChange={(nextOpen) => {
                if (!nextOpen) onClose()
            }}
            finalFocus={finalFocus}
            size="detail"
            title={title}
            identity={identity}
            summary={
                !data || isError ? null : limited ? (
                    <div className="flex items-center gap-2">
                        <BusinessStatusBadge
                            context="preview"
                            label="部分受限"
                            tone="warning"
                        />
                        <span className="text-xs text-muted-foreground">
                            {scopeText.limitedOnlyVisibleShare}
                        </span>
                    </div>
                ) : (
                    <BusinessStatusBadge
                        context="preview"
                        label="范围内"
                        tone="success"
                    />
                )
            }
            footer={
                <>
                    <Button
                        id="customer-receivables-scope-preview-close"
                        type="button"
                        variant="outline"
                        onClick={onClose}
                    >
                        关闭
                    </Button>
                    {data?.kind === "receivable" ? (
                        <PermissionGuard permission="sales_order:detail">
                            <Button
                                id="customer-receivables-scope-preview-open-order"
                                type="button"
                                render={
                                    <Link
                                        href={`/sales/sales-orders/${data.receivable.sales_order_id}`}
                                    />
                                }
                            >
                                打开销售单
                                <ArrowUpRightIcon data-icon="inline-end" />
                            </Button>
                        </PermissionGuard>
                    ) : null}
                </>
            }
        >
            {isPending ? (
                <div className="flex flex-col gap-3 px-7 py-6" aria-busy="true">
                    <Skeleton className="h-24" />
                    <Skeleton className="h-40" />
                </div>
            ) : isError ? (
                <div className="space-y-3 px-7 py-6">
                    <p className="text-xs text-muted-foreground">
                        {getErrorMessage(error, "详情加载失败，请重试。")}
                    </p>
                    <Button
                        id="customer-receivables-scope-preview-retry"
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={onRetry}
                    >
                        重试
                    </Button>
                </div>
            ) : data?.kind === "receivable" ? (
                <div className={bodyClassName}>
                    <PreviewAmount
                        label="权限内已核销"
                        value={data.receivable.visible_settled_share}
                    >
                        <span>
                            整单金额{" "}
                            <MoneyValue
                                value={data.receivable.gross_total}
                                unavailableReason={
                                    data.receivable.permission_limited
                                        ? scopeText.wholeRestricted
                                        : undefined
                                }
                            />
                        </span>
                        <span>
                            未分配{" "}
                            <MoneyValue
                                value={data.receivable.open_total}
                                unavailableReason={
                                    data.receivable.permission_limited
                                        ? scopeText.wholeRestricted
                                        : undefined
                                }
                            />
                        </span>
                    </PreviewAmount>
                    {limited ? (
                        <LimitedBadge id="customer-receivables-scope-preview-limited" />
                    ) : null}
                    {scopeSummary ? (
                        <PreviewNote>{scopeSummary}</PreviewNote>
                    ) : null}
                </div>
            ) : data?.kind === "receipt" ? (
                <div className={bodyClassName}>
                    <PreviewAmount
                        label="权限内已分配"
                        value={data.receipt.visible_allocated_share}
                    >
                        <span>
                            整单金额{" "}
                            <MoneyValue
                                value={data.receipt.amount}
                                unavailableReason={
                                    data.receipt.permission_limited
                                        ? scopeText.wholeRestricted
                                        : undefined
                                }
                            />
                        </span>
                        <span>
                            未分配{" "}
                            <MoneyValue
                                value={data.receipt.unallocated_amount}
                                unavailableReason={
                                    data.receipt.permission_limited
                                        ? scopeText.wholeRestricted
                                        : undefined
                                }
                            />
                        </span>
                    </PreviewAmount>
                    <PreviewSection title="权限内分配">
                        <dl className="space-y-3">
                            {(data.receipt.allocations ?? []).map((line) => (
                                <PreviewFact
                                    key={line.id}
                                    label={`分配 ${line.allocation_seq}`}
                                >
                                    <MoneyValue
                                        value={
                                            line.allocated_amount ??
                                            line.allocated_gross_amount ??
                                            null
                                        }
                                    />
                                </PreviewFact>
                            ))}
                        </dl>
                        {(data.receipt.allocations ?? []).length === 0 ? (
                            <PreviewNote>暂无权限内分配</PreviewNote>
                        ) : null}
                    </PreviewSection>
                    {limited ? (
                        <LimitedBadge id="customer-receivables-scope-preview-limited" />
                    ) : null}
                </div>
            ) : data?.kind === "invoice" ? (
                <div className={bodyClassName}>
                    <PreviewAmount
                        label="权限内已分配"
                        value={data.invoice.visible_allocated_share}
                    >
                        <span>
                            整单金额{" "}
                            <MoneyValue
                                value={data.invoice.gross_amount}
                                unavailableReason={
                                    data.invoice.permission_limited
                                        ? scopeText.wholeRestricted
                                        : undefined
                                }
                            />
                        </span>
                        <span>
                            未分配{" "}
                            <MoneyValue
                                value={data.invoice.unallocated_amount}
                                unavailableReason={
                                    data.invoice.permission_limited
                                        ? scopeText.wholeRestricted
                                        : undefined
                                }
                            />
                        </span>
                    </PreviewAmount>
                    <PreviewSection title="权限内分配">
                        <dl className="space-y-3">
                            {(data.invoice.allocations ?? []).map((line) => (
                                <PreviewFact
                                    key={line.id}
                                    label={`分配 ${line.allocation_seq}`}
                                >
                                    <MoneyValue
                                        value={
                                            line.allocated_gross_amount ??
                                            line.allocated_amount ??
                                            null
                                        }
                                    />
                                </PreviewFact>
                            ))}
                        </dl>
                        {(data.invoice.allocations ?? []).length === 0 ? (
                            <PreviewNote>暂无权限内分配</PreviewNote>
                        ) : null}
                    </PreviewSection>
                    {limited ? (
                        <LimitedBadge id="customer-receivables-scope-preview-limited" />
                    ) : null}
                </div>
            ) : (
                <div className="px-7 py-6 text-body-compact text-muted-foreground">
                    未找到该笔记录，可能已超出当前数据范围。
                </div>
            )}
        </QuickPreviewSheet>
    )
}
