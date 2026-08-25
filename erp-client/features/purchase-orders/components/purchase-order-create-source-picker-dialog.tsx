"use client"

import * as React from "react"

import {
    MoneyValue,
    PaperDocument,
    PaperDocumentViewport,
    QuantityValue,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { PurchaseOrderCreateSwitchList } from "@/features/purchase-orders/components/purchase-order-create-switch-list"
import {
    summarizeSourcingOrder,
    type SourcingProductLine,
    type SourcingSalesOrder,
} from "@/features/purchase-orders/lib/purchase-order-create-model"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    PURCHASE_TYPE_LABEL,
} from "@/features/purchase-orders/types"

export type PurchaseOrderCreateSourcePickerDialogProps = {
    open: boolean
    workspace: readonly SourcingSalesOrder[]
    selectedSalesOrderId: string
    onOpenChange: (open: boolean) => void
    onSelect: (salesOrderId: string) => void
}

function joinLabels(labels: readonly string[]): string {
    return labels.length > 0 ? labels.join("、") : "—"
}

/**
 * 更换来源销售单：左侧切换候选，右侧用纸质单据预览，确认后作为建单来源。
 */
export function PurchaseOrderCreateSourcePickerDialog({
    open,
    workspace,
    selectedSalesOrderId,
    onOpenChange,
    onSelect,
}: PurchaseOrderCreateSourcePickerDialogProps) {
    const [draftId, setDraftId] = React.useState(selectedSalesOrderId)
    const wasOpenRef = React.useRef(open)
    const draftOrder =
        workspace.find((order) => order.salesOrderId === draftId) ??
        workspace[0]

    React.useEffect(() => {
        const justOpened = open && !wasOpenRef.current
        wasOpenRef.current = open
        if (!justOpened) return
        setDraftId(selectedSalesOrderId || workspace[0]?.salesOrderId || "")
    }, [open, selectedSalesOrderId, workspace])

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="flex h-[90vh] max-h-[90vh] w-full flex-col gap-0 overflow-hidden p-0 sm:max-w-6xl">
                <DialogHeader className="shrink-0 border-b border-border px-6 py-4 text-left">
                    <DialogTitle>选择来源销售单</DialogTitle>
                    <DialogDescription>
                        预览可建采购的销售单，选定后作为本次建单来源。
                    </DialogDescription>
                </DialogHeader>

                <div className="flex min-h-0 flex-1 flex-col overflow-hidden md:flex-row">
                    {workspace.length > 1 ? (
                        <PurchaseOrderCreateSwitchList
                            label="可建采购销售单"
                            activeId={draftOrder?.salesOrderId ?? ""}
                            items={workspace.map((order) => ({
                                id: order.salesOrderId,
                                title: order.salesOrderNo,
                                description: `${order.customerName} · ${order.lines.length} 行`,
                            }))}
                            onSelect={setDraftId}
                        />
                    ) : null}

                    <PaperDocumentViewport
                        fitKey={draftOrder?.salesOrderId ?? "empty"}
                    >
                        {draftOrder ? (
                            <SourcingSalesOrderPaper order={draftOrder} />
                        ) : (
                            <p className="text-sm text-muted-foreground">
                                当前没有可预览的销售单。
                            </p>
                        )}
                    </PaperDocumentViewport>
                </div>

                <DialogFooter className="shrink-0 border-t border-border px-6 py-4">
                    <Button
                        type="button"
                        variant="outline"
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <Button
                        type="button"
                        data-testid="purchase-create-source-confirm"
                        disabled={!draftOrder}
                        onClick={() => {
                            if (!draftOrder) return
                            onSelect(draftOrder.salesOrderId)
                            onOpenChange(false)
                        }}
                    >
                        使用该销售单
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}

/**
 * 用来源销售单的可建采购数据投影成纸质单据，不补全未加载的销售详情。
 */
function SourcingSalesOrderPaper({ order }: { order: SourcingSalesOrder }) {
    const summary = summarizeSourcingOrder(order)
    const purchaseTypeLabel = joinLabels(
        summary.purchaseTypes.map((type) => PURCHASE_TYPE_LABEL[type]),
    )
    const fulfillmentLabel = joinLabels(
        summary.fulfillmentResponsibilities.map(
            (item) => FULFILLMENT_RESPONSIBILITY_LABEL[item],
        ),
    )

    return (
        <PaperDocument<SourcingProductLine>
            frame="bare"
            className="min-w-3xl w-3xl max-w-3xl"
            title="销售单"
            subtitle="采购来源"
            documentNumber={order.salesOrderNo}
            parties={[
                {
                    id: "buyer",
                    label: "客户",
                    name: order.customerName,
                    reference: order.contractNumber ?? "无合同",
                    fields: [
                        {
                            id: "contract",
                            label: "合同",
                            value: order.contractNumber ?? "无合同",
                            numeric: true,
                        },
                    ],
                },
                {
                    id: "owner",
                    label: "负责销售",
                    name: order.salesOwnerName ?? "—",
                    fields: [
                        {
                            id: "pending",
                            label: "待采购明细",
                            value: `${summary.lineCount} 行`,
                            numeric: true,
                        },
                        {
                            id: "suppliers",
                            label: "可选供应商",
                            value: `${summary.uniqueSupplierCount} 家`,
                            numeric: true,
                        },
                    ],
                },
            ]}
            metadata={[
                {
                    id: "covered",
                    label: "已覆盖明细",
                    value: `${summary.coveredLineCount} 行`,
                    numeric: true,
                },
                {
                    id: "purchase-type",
                    label: "采购类型",
                    value: purchaseTypeLabel,
                },
                {
                    id: "fulfillment",
                    label: "履约责任",
                    value: fulfillmentLabel,
                },
                {
                    id: "payment",
                    label: "付款条件",
                    value: joinLabels(summary.paymentTermLabels),
                },
                ...(summary.businessCategories.length > 0
                    ? [
                          {
                              id: "category",
                              label: "经营类目",
                              value: joinLabels(summary.businessCategories),
                          },
                      ]
                    : []),
            ]}
            lineItemLabel="待采购明细"
            columns={[
                {
                    id: "item",
                    header: "销售项目",
                    cell: (row) => (
                        <div>
                            <div>{row.itemName}</div>
                            {row.itemSku ? (
                                <div className="num mt-1 text-xs text-muted-foreground">
                                    {row.itemSku}
                                </div>
                            ) : null}
                        </div>
                    ),
                },
                {
                    id: "sales-qty",
                    header: "销售数量",
                    align: "end",
                    numeric: true,
                    cell: (row) => (
                        <QuantityValue
                            value={row.salesQuantity}
                            unit={row.unit}
                        />
                    ),
                },
                {
                    id: "covered",
                    header: "已覆盖",
                    align: "end",
                    numeric: true,
                    cell: (row) => (
                        <QuantityValue
                            value={row.coveredQuantity}
                            unit={row.unit}
                        />
                    ),
                },
                {
                    id: "remaining",
                    header: "剩余数量",
                    align: "end",
                    numeric: true,
                    cell: (row) => (
                        <QuantityValue
                            value={row.remainingQuantity}
                            unit={row.unit}
                        />
                    ),
                },
                {
                    id: "options",
                    header: "可选供应商",
                    align: "end",
                    numeric: true,
                    cell: (row) => `${row.options.length} 家`,
                },
            ]}
            rows={order.lines}
            getRowId={(row) => row.salesOrderLineId}
            totals={[
                {
                    id: "lines",
                    label: "待采购明细",
                    value: `${summary.lineCount} 行`,
                },
                {
                    id: "estimate",
                    label: "最低含税估算",
                    value: <MoneyValue value={summary.minEstimatedGross} />,
                    emphasized: true,
                    description: "按每条明细最低含税成本估算",
                },
            ]}
            remarks="本预览展示该销售单当前可建采购的明细，含税金额为按最低成本估算。"
        />
    )
}
