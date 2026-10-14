"use client"

import * as React from "react"

import {
    BusinessFailureState,
    DocumentSection,
    MoneyValue,
    RelatedDocumentList,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    ExpensePurchaseCreateDialog,
    type ExpensePurchasePrefill,
} from "@/features/purchase-orders/components/expense-purchase-create-dialog"
import { PurchaseOrderPaperDialog } from "@/features/purchase-orders/components/purchase-order-paper-dialog"
import { usePurchaseOrdersQuery } from "@/features/purchase-orders/hooks/queries"
import { displayPurchaseOrderNo } from "@/features/purchase-orders/lib/purchase-orders-list-helpers"
import type {
    PurchaseOrderListItem,
    PurchaseOrderStatus,
} from "@/features/purchase-orders/types"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { useSalesOrderDetailPermissions } from "@/features/sales-orders/hooks/use-sales-order-detail-permissions"
import { SalesOrderProcurementProgress } from "@/features/sales-orders/components/sales-order-procurement-progress"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"

const RELATED_PURCHASE_PAGE_SIZE = 100

/** 销售扫采购单时，阻塞态优先：审批中/草稿先于已生效。 */
const PURCHASE_STATUS_SUMMARY_ORDER: readonly PurchaseOrderStatus[] = [
    "PENDING_REVIEW",
    "DRAFT",
    "EFFECTIVE",
    "PARTIAL",
    "COMPLETED",
    "VOID",
]

function PreviewButton({
    id,
    onClick,
    disabled,
    disabledReason,
}: {
    id?: string
    onClick: () => void
    disabled?: boolean
    disabledReason?: string
}) {
    return (
        <Button
            id={id}
            type="button"
            size="sm"
            variant="secondary"
            disabled={disabled}
            title={disabled ? disabledReason : undefined}
            onClick={disabled ? undefined : onClick}
        >
            预览
        </Button>
    )
}

/**
 * 销售单「采购」分区：列出本单已创建的采购单。
 * 有 `purchase_order:list` 才拉列表；有 `purchase_order:detail` 才能纸质预览。
 * 无列表权限时只展示销售单详情里的采购单笔数，不暴露单号与内容。
 * 列表展示单据生命周期与履约/付款进度，供销售判断采购是否卡住。
 */
export function PurchasePanel({ order }: { order: SalesOrderDetailView }) {
    const permissions = useSalesOrderDetailPermissions()
    const canList = permissions.openPurchase.enabled
    const canPreview = permissions.previewPurchase.enabled
    const purchaseCount = order.related.purchaseOrders

    const listQuery = usePurchaseOrdersQuery(
        {
            salesOrderId: order.id,
            page: 1,
            pageSize: RELATED_PURCHASE_PAGE_SIZE,
        },
        { enabled: canList },
    )

    const [previewId, setPreviewId] = React.useState<string | null>(null)
    const [expenseOpen, setExpenseOpen] = React.useState(false)
    const [expensePrefill, setExpensePrefill] = React.useState<
        ExpensePurchasePrefill | undefined
    >(undefined)
    const freightFollowUps = order.related.freightFollowUps ?? []
    const [expenseCreated, setExpenseCreated] = React.useState<string | null>(
        null,
    )
    const canAddExpense =
        order.nature === "physical_service" &&
        order.currentRevisionNo != null &&
        hasPermission(permissions.granted, "purchase_order:create")
    const affectedIds = new Set(
        order.related.procurementProgress.sourceUpdates.map(
            (update) => update.purchaseOrderId,
        ),
    )
    const statusSummary = purchaseOrderStatusSummary(
        canList ? (listQuery.data?.rows ?? []) : [],
    )
    return (
        <div className="space-y-6">
            <SalesOrderProcurementProgress
                order={order}
                surface="purchase"
                statusSummary={statusSummary}
            />

            {permissions.accountQuery.isPending ? (
                <div
                    className="h-36 animate-pulse rounded-lg bg-muted"
                    aria-busy="true"
                    aria-label="正在核对采购单权限"
                />
            ) : !canList ? (
                <PurchaseCountOnlyNotice count={purchaseCount} />
            ) : listQuery.isPending ? (
                <div
                    className="h-48 animate-pulse rounded-lg bg-muted"
                    aria-busy="true"
                    aria-label="加载本单采购单"
                />
            ) : listQuery.isError ? (
                <BusinessFailureState
                    id="sales-orders-detail-purchase-retry"
                    title="本单采购单加载失败"
                    error={listQuery.error}
                    onRetry={() => {
                        void listQuery.refetch()
                    }}
                />
            ) : (
                <DocumentSection
                    title="采购明细"
                    className="border-0 py-0 [&_h2]:text-sm [&>div:last-child]:pt-3"
                    description={
                        listQuery.data &&
                        listQuery.data.total > RELATED_PURCHASE_PAGE_SIZE
                            ? `本单共 ${listQuery.data.total} 张，当前只列出前 ${RELATED_PURCHASE_PAGE_SIZE} 张。`
                            : undefined
                    }
                >
                    <RelatedDocumentList
                        layout="name-first"
                        identityLabel="供应商 / 采购单"
                        measureLabel="含税金额"
                        documents={(listQuery.data?.rows ?? []).map((row) =>
                            toRelatedPurchaseDocument(row, {
                                sourceOutdated: affectedIds.has(
                                    row.purchaseOrderId,
                                ),
                                canPreview,
                                previewReason:
                                    permissions.previewPurchase.reason,
                                onPreview: () =>
                                    setPreviewId(row.purchaseOrderId),
                            }),
                        )}
                        emptyContent={
                            purchaseCount > 0
                                ? "本单已有采购单，但当前账号的数据范围看不到明细。"
                                : "本单还没有采购单。"
                        }
                    />
                </DocumentSection>
            )}

            {canAddExpense && freightFollowUps.length ? (
                <section
                    aria-labelledby="sales-orders-detail-freight-title"
                    className="space-y-2 rounded-lg border border-warning-border bg-warning-soft px-4 py-3"
                >
                    <h3
                        id="sales-orders-detail-freight-title"
                        className="text-sm font-medium text-warning-soft-foreground"
                    >
                        运费待追加
                    </h3>
                    <p className="text-xs text-warning-soft-foreground">
                        以下商品为集采运费另计。供应商收取运费时追加物流费用采购；供应商不收运费时在采购单上标记。
                    </p>
                    <ul className="divide-y divide-warning-border">
                        {freightFollowUps.map((item) => (
                            <li
                                key={`${item.purchaseOrderId}-${item.salesOrderLineId}`}
                                className="flex flex-wrap items-center justify-between gap-2 py-2 text-sm"
                            >
                                <span className="min-w-0">
                                    {item.productName ?? "商品"}
                                    <span className="ml-2 text-xs text-muted-foreground">
                                        {item.supplierName ?? "供应商"} · 采购单{" "}
                                        {item.purchaseNo || "草稿"}
                                    </span>
                                </span>
                                <Button
                                    id={`sales-orders-detail-freight-add-${toAutomationIdSegment(item.purchaseOrderId)}-${toAutomationIdSegment(item.salesOrderLineId)}`}
                                    type="button"
                                    size="sm"
                                    variant="secondary"
                                    onClick={() => {
                                        setExpensePrefill({
                                            supplierId: item.supplierId,
                                            salesOrderLineId:
                                                item.salesOrderLineId,
                                            description: `${item.productName ?? "商品"} 运费`,
                                        })
                                        setExpenseOpen(true)
                                    }}
                                >
                                    追加运费
                                </Button>
                            </li>
                        ))}
                    </ul>
                </section>
            ) : null}

            {canAddExpense ? (
                <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-dashed px-4 py-3">
                    <p className="text-sm text-muted-foreground">
                        {expenseCreated
                            ? `已创建费用采购单 ${expenseCreated}，已提交审批。`
                            : "本单有我方承担、不对应销售明细的费用（第三方物流、额外包装、上门安装等）时，追加费用采购单走审批和付款。"}
                    </p>
                    <Button
                        id="sales-orders-detail-add-expense-purchase"
                        type="button"
                        size="sm"
                        variant="secondary"
                        onClick={() => {
                            setExpensePrefill(undefined)
                            setExpenseOpen(true)
                        }}
                    >
                        追加附加费用
                    </Button>
                </div>
            ) : null}

            <ExpensePurchaseCreateDialog
                salesOrderId={order.id}
                salesLines={order.lineItems
                    .filter((line) => line.quantity !== "0")
                    .map((line) => ({ id: line.id, name: line.name }))}
                open={expenseOpen}
                onOpenChange={setExpenseOpen}
                prefill={expensePrefill}
                onCreated={(created) => setExpenseCreated(created.purchaseNo)}
            />

            <PurchaseOrderPaperDialog
                purchaseOrderId={previewId}
                open={previewId != null}
                onOpenChange={(open) => {
                    if (!open) setPreviewId(null)
                }}
            />
        </div>
    )
}

function PurchaseCountOnlyNotice({ count }: { count: number }) {
    return (
        <div
            className="rounded-lg bg-muted px-4 py-3 text-sm text-muted-foreground"
            data-testid="sales-order-purchase-count-only"
        >
            {count > 0
                ? `已创建 ${count} 张采购单。当前账号没有采购单查看权限，无法预览单据内容。`
                : "本单还没有采购单。"}
        </div>
    )
}

function purchaseOrderStatusSummary(
    rows: readonly PurchaseOrderListItem[],
): string {
    if (rows.length === 0) return ""
    const counts = new Map<
        PurchaseOrderStatus,
        { label: string; count: number }
    >()
    for (const row of rows) {
        const current = counts.get(row.status)
        if (current) {
            current.count += 1
        } else {
            counts.set(row.status, { label: row.statusLabel, count: 1 })
        }
    }
    return PURCHASE_STATUS_SUMMARY_ORDER.flatMap((status) => {
        const item = counts.get(status)
        return item ? [`${item.count} 张${item.label}`] : []
    }).join(" · ")
}

function purchaseProgressTracks(
    row: PurchaseOrderListItem,
    sourceOutdated: boolean,
) {
    const sourceTrack = sourceOutdated
        ? [
              {
                  id: "source",
                  label: "依据",
                  status: { label: "待更新", tone: "warning" as const },
              },
          ]
        : []
    return [
        ...sourceTrack,
        {
            id: "fulfillment",
            label: "履约",
            status: {
                label: row.fulfillmentProgress,
                tone:
                    row.paymentGate === "BLOCKED"
                        ? ("warning" as const)
                        : row.fulfillmentProgress === "完成"
                          ? ("success" as const)
                          : row.fulfillmentProgress === "部分"
                            ? ("info" as const)
                            : ("neutral" as const),
            },
        },
        {
            id: "payment",
            label: "付款",
            status: {
                label: row.paymentProgress,
                tone:
                    row.paymentProgress === "已付"
                        ? ("success" as const)
                        : row.paymentProgress === "部分付款"
                          ? ("info" as const)
                          : ("neutral" as const),
            },
        },
    ]
}

function toRelatedPurchaseDocument(
    row: PurchaseOrderListItem,
    options: {
        sourceOutdated: boolean
        canPreview: boolean
        previewReason?: string
        onPreview: () => void
    },
) {
    return {
        id: row.purchaseOrderId,
        documentType: "采购单",
        name: row.supplierName,
        documentNumber: displayPurchaseOrderNo(row),
        status: {
            label: row.statusLabel,
            tone: row.statusTone,
        },
        tracks: purchaseProgressTracks(row, options.sourceOutdated),
        measure: {
            kind: "amount" as const,
            value: row.costMasked ? (
                <span className="text-muted-foreground">•••</span>
            ) : (
                <MoneyValue value={row.grossAmount} />
            ),
            label: "含税金额",
        },
        owner: row.ownerName,
        openAction: (
            <PreviewButton
                id={`sales-orders-detail-purchase-${toAutomationIdSegment(row.purchaseOrderId)}-preview`}
                disabled={!options.canPreview}
                disabledReason={options.previewReason}
                onClick={options.onPreview}
            />
        ),
    }
}
