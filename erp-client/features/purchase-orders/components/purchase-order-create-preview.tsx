"use client"

import * as React from "react"

import { PaperDocumentViewport, QuantityValue } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
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
    SourcingSalesOrder,
    StockAllocationPreviewLine,
} from "@/features/purchase-orders/lib/purchase-order-create-model"
import { PURCHASE_TYPE_LABEL } from "@/features/purchase-orders/types"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"
import { DownloadIcon } from "lucide-react"
import { toast } from "@/components/ui/toast"
import { PurchaseOrderPreviewPaper } from "./purchase-order-create-preview-paper"

export type PurchaseOrderCreatePreviewDialogProps = {
    open: boolean
    previews: readonly PurchaseOrderPreview[]
    stockAllocations: readonly StockAllocationPreviewLine[]
    sourceOrder?: SourcingSalesOrder
    scopeRows?: readonly SourcingEditorRow[]
    creating?: boolean
    unresolved?: boolean
    actionError?: { title: string; description: string } | null
    onOpenChange: (open: boolean) => void
    description?: string
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
    scopeRows = [],
    creating,
    unresolved,
    actionError,
    onOpenChange,
    onConfirm,
    description,
}: PurchaseOrderCreatePreviewDialogProps) {
    const [activeKey, setActiveKey] = React.useState("")
    const [downloadProgress, setDownloadProgress] = React.useState<
        number | null
    >(null)
    const downloadLock = React.useRef(false)
    const downloading = downloadProgress !== null

    async function downloadImages() {
        if (downloadLock.current || previews.length === 0) return
        downloadLock.current = true
        setDownloadProgress(0)
        try {
            const { downloadPurchasePreviewImages } =
                await import("../lib/download-purchase-preview-images")
            await downloadPurchasePreviewImages(
                previews,
                sourceOrder,
                setDownloadProgress,
            )
            toast.add({
                title: `已生成 ${previews.length} 张采购单图片`,
                description: "图片已打包下载，请在浏览器下载列表中查看。",
                type: "success",
            })
        } catch {
            toast.add({
                title: "采购单图片下载失败",
                description: "请稍后重试。",
                type: "error",
            })
        } finally {
            downloadLock.current = false
            setDownloadProgress(null)
        }
    }
    const excluded = scopeRows.filter(
        (row) => !row.selected && !row.partiallySelected,
    )
    const partial = scopeRows.filter((row) => row.status === "部分分配")
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
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!creating && !unresolved && !downloading) onOpenChange(next)
            }}
        >
            <DialogContent
                className="flex h-[90vh] max-h-[90vh] w-full flex-col gap-0 overflow-hidden p-0 sm:max-w-6xl"
                closeButtonId="procurement-orders-create-preview-close"
                showCloseButton={!creating && !unresolved && !downloading}
            >
                <DialogHeader className="shrink-0 border-b border-border px-6 py-4 text-left">
                    <DialogTitle>预览供给分配</DialogTitle>
                    <DialogDescription>
                        {description ?? allocationSummary}
                    </DialogDescription>
                </DialogHeader>

                {scopeRows.length > 0 ? (
                    <div className="shrink-0 border-b border-border px-6 py-3 text-sm">
                        <p className="font-medium">
                            本次分配 {scopeRows.length - excluded.length} 项 ·
                            暂不分配 {excluded.length} 项
                        </p>
                        {excluded.length > 0 || partial.length > 0 ? (
                            <div className="mt-2 max-h-32 overflow-auto space-y-1 text-xs text-muted-foreground">
                                {excluded.map((row) => (
                                    <p key={row.product.salesOrderLineId}>
                                        {row.product.itemName} · 暂不分配 ·
                                        本次不提交
                                    </p>
                                ))}
                                {partial.map((row) => (
                                    <p key={row.product.salesOrderLineId}>
                                        {row.product.itemName} · 本次{" "}
                                        <QuantityValue
                                            value={row.quantity}
                                            unit={row.product.unit}
                                        />{" "}
                                        · 剩余{" "}
                                        <QuantityValue
                                            value={row.remainingQuantity}
                                            unit={row.product.unit}
                                        />
                                    </p>
                                ))}
                                <p>
                                    未分配的需求仍需后续处理，本次确认不会将其标为完成。
                                </p>
                            </div>
                        ) : null}
                    </div>
                ) : null}

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
                                description: `${PURCHASE_TYPE_LABEL[preview.purchaseType]} · ${preview.paymentTermLabel}${preview.targetWarehouseName ? ` · ${preview.targetWarehouseName}` : ""} · ${preview.lines.length} 行`,
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

                <DialogFooter className="shrink-0 flex-wrap border-t border-border px-6 py-4">
                    {previews.length > 0 ? (
                        <LoadingButton
                            id="procurement-orders-create-preview-download"
                            type="button"
                            variant="outline"
                            className="sm:mr-auto"
                            loading={downloading}
                            disabled={creating || unresolved || downloading}
                            onClick={() => void downloadImages()}
                            title="每张采购单保存为 PNG 图片，打包下载"
                        >
                            <DownloadIcon data-icon="inline-start" />
                            {downloading
                                ? `生成图片 ${downloadProgress}/${previews.length}`
                                : `下载全部采购单图片（${previews.length}）`}
                        </LoadingButton>
                    ) : null}
                    <Button
                        id="procurement-orders-create-preview-cancel"
                        type="button"
                        variant="outline"
                        disabled={creating || unresolved || downloading}
                        onClick={() => onOpenChange(false)}
                    >
                        返回编辑
                    </Button>
                    <LoadingButton
                        id="procurement-orders-create-preview-confirm"
                        loading={creating}
                        type="button"
                        data-testid="purchase-create-from-basis"
                        disabled={
                            (previews.length === 0 &&
                                stockAllocations.length === 0 &&
                                !unresolved) ||
                            creating ||
                            downloading
                        }
                        onClick={onConfirm}
                    >
                        {creating
                            ? "提交中…"
                            : unresolved
                              ? "核对提交结果"
                              : confirmLabel}
                    </LoadingButton>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
