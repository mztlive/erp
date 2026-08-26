"use client"

import * as React from "react"

import {
    MoneyValue,
    PaperDocument,
    PaperDocumentViewport,
    QuantityValue,
    RateValue,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
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
import type {
    PurchaseOrderPreview,
    PurchaseOrderPreviewLine,
    SourcingSalesOrder,
    StockAllocationPreviewLine,
} from "@/features/purchase-orders/lib/purchase-order-create-model"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    PO_STATUS_LABEL,
    PO_STATUS_TONE,
    PURCHASE_TYPE_LABEL,
} from "@/features/purchase-orders/types"
import { multiplyFixed } from "@/lib/fixed-decimal"

export type PurchaseOrderCreatePreviewDialogProps = {
    open: boolean
    previews: readonly PurchaseOrderPreview[]
    stockAllocations: readonly StockAllocationPreviewLine[]
    sourceOrder?: SourcingSalesOrder
    creating?: boolean
    actionError?: { title: string; description: string } | null
    onOpenChange: (open: boolean) => void
    onConfirm: () => void
}

/**
 * 弹出式供给分配预览：展示库存预留，并按拆单结果预览待提交采购单。
 */
export function PurchaseOrderCreatePreviewDialog({
    open,
    previews,
    stockAllocations,
    sourceOrder,
    creating,
    actionError,
    onOpenChange,
    onConfirm,
}: PurchaseOrderCreatePreviewDialogProps) {
    const [activeKey, setActiveKey] = React.useState("")
    const activePreview =
        previews.find((preview) => preview.key === activeKey) ?? previews[0]
    const activeIndex = activePreview
        ? previews.findIndex((preview) => preview.key === activePreview.key)
        : -1
    const allocationSummary =
        stockAllocations.length > 0 && previews.length > 0
            ? `将建立 ${stockAllocations.length} 条库存预留，并按供应商创建 ${previews.length} 张采购单提交审批。`
            : stockAllocations.length > 0
              ? `将建立 ${stockAllocations.length} 条库存预留，本次无需创建采购单。`
              : `将按供应商创建 ${previews.length} 张采购单提交审批，本次不占用现有库存。`
    const confirmLabel =
        stockAllocations.length > 0 && previews.length > 0
            ? `确认库存分配并提交 ${previews.length} 张采购单`
            : stockAllocations.length > 0
              ? "确认库存分配"
              : `确认提交 ${previews.length} 张采购单`

    React.useEffect(() => {
        if (!open) return
        if (previews.some((preview) => preview.key === activeKey)) return
        setActiveKey(previews[0]?.key ?? "")
    }, [activeKey, open, previews])

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="flex h-[90vh] max-h-[90vh] w-full flex-col gap-0 overflow-hidden p-0 sm:max-w-6xl">
                <DialogHeader className="shrink-0 border-b border-border px-6 py-4 text-left">
                    <DialogTitle>预览供给分配</DialogTitle>
                    <DialogDescription>
                        {allocationSummary}确认后由一个后端事务统一处理。
                    </DialogDescription>
                </DialogHeader>

                {stockAllocations.length > 0 ? (
                    <div className="shrink-0 px-6 pt-4">
                        <Alert>
                            <AlertTitle>现有库存分配</AlertTitle>
                            <AlertDescription>
                                <ul className="mt-2 space-y-1">
                                    {stockAllocations.map((line, index) => (
                                        <li
                                            key={`${line.salesOrderLineId}:${line.warehouseName}:${index}`}
                                        >
                                            {line.itemName} ·{" "}
                                            {line.warehouseName} ·{" "}
                                            {line.quantity} {line.unit}
                                        </li>
                                    ))}
                                </ul>
                            </AlertDescription>
                        </Alert>
                    </div>
                ) : null}

                {actionError ? (
                    <div className="shrink-0 px-6 pt-4">
                        <Alert variant="destructive">
                            <AlertTitle>{actionError.title}</AlertTitle>
                            <AlertDescription>
                                {actionError.description}
                            </AlertDescription>
                        </Alert>
                    </div>
                ) : null}

                <div className="flex min-h-0 flex-1 flex-col overflow-hidden md:flex-row">
                    {previews.length > 1 ? (
                        <PurchaseOrderCreateSwitchList
                            label="将创建的采购单"
                            activeId={activePreview?.key ?? ""}
                            items={previews.map((preview, index) => ({
                                id: preview.key,
                                title: `${index + 1}. ${preview.supplierName}`,
                                description: `${PURCHASE_TYPE_LABEL[preview.purchaseType]} · ${preview.paymentTermLabel} · ${preview.lines.length} 行`,
                            }))}
                            onSelect={setActiveKey}
                        />
                    ) : null}

                    <PaperDocumentViewport
                        fitKey={activePreview?.key ?? "empty"}
                    >
                        {activePreview ? (
                            <PurchaseOrderPreviewPaper
                                preview={activePreview}
                                index={Math.max(activeIndex, 0)}
                                sourceOrder={sourceOrder}
                            />
                        ) : (
                            <p className="text-sm text-muted-foreground">
                                本次全部由现有库存满足，不会创建采购单。
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
                        返回编辑
                    </Button>
                    <Button
                        type="button"
                        data-testid="purchase-create-from-basis"
                        disabled={
                            (previews.length === 0 &&
                                stockAllocations.length === 0) ||
                            creating
                        }
                        onClick={onConfirm}
                    >
                        {creating ? "提交中…" : confirmLabel}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}

/**
 * 把拆单预览投影成纸质采购单，金额与明细以当前选源结果为准。
 */
function PurchaseOrderPreviewPaper({
    preview,
    index,
    sourceOrder,
}: {
    preview: PurchaseOrderPreview
    index: number
    sourceOrder?: SourcingSalesOrder
}) {
    return (
        <div data-testid={`purchase-create-preview-${preview.supplierId}`}>
            <PaperDocument<PurchaseOrderPreviewLine>
                frame="bare"
                title="采购单"
                subtitle="提交预览"
                documentNumber={`预览 ${index + 1}`}
                status={{
                    label: PO_STATUS_LABEL.PENDING_REVIEW,
                    tone: PO_STATUS_TONE.PENDING_REVIEW,
                }}
                parties={[
                    {
                        id: "supplier",
                        label: "供应商",
                        name: preview.supplierName,
                        fields: [
                            {
                                id: "payment",
                                label: "付款条件",
                                value: preview.paymentTermLabel,
                            },
                            {
                                id: "type",
                                label: "采购类型",
                                value: PURCHASE_TYPE_LABEL[
                                    preview.purchaseType
                                ],
                            },
                        ],
                    },
                    {
                        id: "source",
                        label: "来源销售单",
                        name: sourceOrder?.customerName ?? "—",
                        reference: sourceOrder?.salesOrderNo,
                        fields: [
                            {
                                id: "contract",
                                label: "合同",
                                value: sourceOrder?.contractNumber ?? "无合同",
                                numeric: true,
                            },
                            {
                                id: "owner",
                                label: "负责销售",
                                value: sourceOrder?.salesOwnerName ?? "—",
                            },
                        ],
                    },
                ]}
                metadata={[
                    {
                        id: "fulfillment",
                        label: "履约责任",
                        value: FULFILLMENT_RESPONSIBILITY_LABEL[
                            preview.fulfillmentResponsibility
                        ],
                    },
                    {
                        id: "lines",
                        label: "明细行数",
                        value: `${preview.lines.length} 行`,
                        numeric: true,
                    },
                    {
                        id: "delivery",
                        label: "预计交期",
                        value: earliestDeliveryDate(preview.lines),
                        numeric: true,
                    },
                    {
                        id: "gross",
                        label: "含税合计",
                        value: <MoneyValue value={preview.totals.gross} />,
                    },
                ]}
                lineItemLabel="采购明细"
                columns={[
                    {
                        id: "item",
                        header: "采购项目",
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
                        id: "qty",
                        header: "数量",
                        align: "end",
                        numeric: true,
                        cell: (row) => (
                            <QuantityValue
                                value={row.quantity}
                                unit={row.unit}
                            />
                        ),
                    },
                    {
                        id: "cost",
                        header: "含税成本",
                        align: "end",
                        numeric: true,
                        cell: (row) => <MoneyValue value={row.unitCostGross} />,
                    },
                    {
                        id: "tax",
                        header: "进项税率",
                        align: "end",
                        numeric: true,
                        cell: (row) => (
                            <RateValue
                                value={multiplyFixed(row.inputTaxRate, "100", {
                                    leftMaxScale: 6,
                                    rightMaxScale: 0,
                                    outputScale: 2,
                                })}
                                precision={2}
                            />
                        ),
                    },
                    {
                        id: "amount",
                        header: "含税金额",
                        align: "end",
                        numeric: true,
                        cell: (row) => <MoneyValue value={row.grossAmount} />,
                    },
                    {
                        id: "due",
                        header: "预计交期",
                        align: "end",
                        numeric: true,
                        cell: (row) => row.expectedDeliveryDate || "—",
                    },
                ]}
                rows={preview.lines}
                getRowId={(row) => row.salesOrderLineId}
                totals={[
                    {
                        id: "net",
                        label: "不含税金额",
                        value: <MoneyValue value={preview.totals.net} />,
                    },
                    {
                        id: "tax",
                        label: "税额",
                        value: <MoneyValue value={preview.totals.tax} />,
                    },
                    {
                        id: "gross",
                        label: "含税合计",
                        value: <MoneyValue value={preview.totals.gross} />,
                        emphasized: true,
                    },
                ]}
                remarks="本预览按当前选源结果拆单，确认后创建采购单并提交审批，金额以系统计算为准。"
            />
        </div>
    )
}

function earliestDeliveryDate(
    lines: readonly PurchaseOrderPreviewLine[],
): string {
    const dates = lines
        .map((line) => line.expectedDeliveryDate.trim())
        .filter(Boolean)
        .sort()
    return dates[0] ?? "—"
}
