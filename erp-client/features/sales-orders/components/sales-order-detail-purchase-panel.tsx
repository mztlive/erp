"use client"

import * as React from "react"
import Link from "next/link"

import {
    BusinessFailureState,
    DocumentSection,
    MoneyValue,
    RelatedDocumentList,
    surfaceInsetClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { PurchaseOrderPaperDialog } from "@/features/purchase-orders/components/purchase-order-paper-dialog"
import { usePurchaseOrdersQuery } from "@/features/purchase-orders/hooks/queries"
import { displayPurchaseOrderNo } from "@/features/purchase-orders/lib/purchase-orders-list-helpers"
import type {
    PurchaseOrderListItem,
    PurchaseOrderStatus,
} from "@/features/purchase-orders/types"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { SectionLead } from "@/features/sales-orders/components/sales-order-detail-lifecycle-rail"
import { useSalesOrderDetailPermissions } from "@/features/sales-orders/hooks/use-sales-order-detail-permissions"
import {
    canCreatePurchaseFromSalesOrder,
    purchaseOrdersWorkspaceHref,
} from "@/features/sales-orders/lib/sales-order-detail-model"
import { cn } from "@/lib/utils"

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
    onClick,
    disabled,
    disabledReason,
}: {
    onClick: () => void
    disabled?: boolean
    disabledReason?: string
}) {
    return (
        <Button
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
export function PurchasePanel({
    order,
    selfReturn,
}: {
    order: SalesOrderDetailView
    selfReturn: string
}) {
    const permissions = useSalesOrderDetailPermissions()
    const canList = permissions.openPurchase.enabled
    const canPreview = permissions.previewPurchase.enabled
    const createPurchase = canCreatePurchaseFromSalesOrder(order)
    const createGate = createPurchase
        ? permissions.createPurchase(true, "当前不能从本单分配供给")
        : { enabled: false, reason: undefined }
    const purchaseCount = order.related.purchaseOrders
    const progress = order.related.procurementProgress

    const listQuery = usePurchaseOrdersQuery(
        {
            salesOrderId: order.id,
            page: 1,
            pageSize: RELATED_PURCHASE_PAGE_SIZE,
        },
        { enabled: canList },
    )

    const [previewId, setPreviewId] = React.useState<string | null>(null)
    const listedRows = canList ? (listQuery.data?.rows ?? []) : []
    const statusSummary = purchaseOrderStatusSummary(listedRows)

    return (
        <div className="flex flex-col gap-4">
            <SectionLead>
                本销售单的供给覆盖与已创建采购单。现有库存可直接形成预占，采购缺口继续查看审批、履约和付款进度。
            </SectionLead>

            <div className={cn(surfaceInsetClassName, "px-3 py-3")}>
                <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="min-w-0">
                        <h3 className="text-sm font-medium">本单供给与采购</h3>
                        <p
                            className="mt-1 text-xs text-muted-foreground"
                            data-testid="sales-order-purchase-status"
                        >
                            采购单 {purchaseCount} 笔
                            {statusSummary
                                ? ` · ${statusSummary}`
                                : ` · ${progress.label}`}
                        </p>
                        <p
                            className="num mt-1 text-xs text-muted-foreground"
                            data-testid="sales-order-purchase-progress"
                        >
                            供给目标 {progress.salesQuantity} · 已覆盖{" "}
                            {progress.coveredQuantity} · 剩余{" "}
                            {progress.remainingQuantity}
                            {statusSummary ? ` · ${progress.label}` : null}
                        </p>
                    </div>
                    {createPurchase ? (
                        createGate.enabled ? (
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                data-testid="sales-order-purchase-create"
                                render={
                                    <Link
                                        href={purchaseOrdersWorkspaceHref(
                                            order,
                                            selfReturn,
                                        )}
                                    />
                                }
                            >
                                继续分配供给
                            </Button>
                        ) : (
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                data-testid="sales-order-purchase-create"
                                disabled
                                title={createGate.reason}
                            >
                                继续分配供给
                            </Button>
                        )
                    ) : null}
                </div>
            </div>

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
                    title="本单采购单加载失败"
                    error={listQuery.error}
                    onRetry={() => {
                        void listQuery.refetch()
                    }}
                />
            ) : (
                <DocumentSection
                    title="已创建的采购单"
                    description={
                        listQuery.data &&
                        listQuery.data.total > RELATED_PURCHASE_PAGE_SIZE
                            ? `本单共 ${listQuery.data.total} 张，当前只列出前 ${RELATED_PURCHASE_PAGE_SIZE} 张。`
                            : undefined
                    }
                >
                    <RelatedDocumentList
                        documents={(listQuery.data?.rows ?? []).map((row) =>
                            toRelatedPurchaseDocument(row, {
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

function purchaseProgressTracks(row: PurchaseOrderListItem) {
    return [
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
                        : row.paymentProgress === "部分"
                          ? ("info" as const)
                          : ("neutral" as const),
            },
        },
    ]
}

function toRelatedPurchaseDocument(
    row: PurchaseOrderListItem,
    options: {
        canPreview: boolean
        previewReason?: string
        onPreview: () => void
    },
) {
    return {
        id: row.purchaseOrderId,
        documentType: `采购单 · ${row.supplierName}`,
        documentNumber: displayPurchaseOrderNo(row),
        status: {
            label: row.statusLabel,
            tone: row.statusTone,
        },
        tracks: purchaseProgressTracks(row),
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
                disabled={!options.canPreview}
                disabledReason={options.previewReason}
                onClick={options.onPreview}
            />
        ),
    }
}
