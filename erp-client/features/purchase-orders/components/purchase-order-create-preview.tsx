"use client"

import * as React from "react"
import {
    ArrowUpRightIcon,
    DownloadIcon,
    FileTextIcon,
    LayoutGridIcon,
    MapPinIcon,
    PackageCheckIcon,
    TriangleAlertIcon,
} from "lucide-react"
import {
    MoneyValue,
    PaperDocumentViewport,
    QuantityValue,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toast } from "@/components/ui/toast"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import type {
    PurchaseOrderPreview,
    SourcingSalesOrder,
    StockAllocationPreviewLine,
} from "../lib/purchase-order-create-model"
import {
    countPurchaseDeliveryRows,
    shouldBundleDeliveryWorkbook,
} from "../lib/purchase-delivery-plan"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"
import {
    comparePurchaseCostToSalesPrice,
    summarizeRowPricing,
} from "../lib/sourcing/pricing"
import { PURCHASE_TYPE_LABEL } from "../types"
import { PurchaseDeliveryPlanExplorer } from "./purchase-delivery-plan"
import { PurchaseOrderPreviewPaper } from "./purchase-order-create-preview-paper"

type PreviewRightTab = "paper" | "addresses" | "overview"

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
    /** 从预览中直接定位回主表修改某商品。 */
    onLocateProduct?: (salesOrderLineId: string) => void
}

/**
 * 升级版供给分配预览弹窗：
 * - 左侧固定核对轨整合待创建采购单、现有库存分配、价格倒挂风控与暂不分配清单，不再竖向挤压纸面高度；
 * - 右侧支持「采购单纸面 / 收货地址明细 / 分单总览表」同层切换，彻底消除 Dialog 内再弹 Sheet 的套娃。
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
    onLocateProduct,
    description,
}: PurchaseOrderCreatePreviewDialogProps) {
    const [activeKey, setActiveKey] = React.useState("")
    const [rightTab, setRightTab] = React.useState<PreviewRightTab>("paper")
    const [downloadProgress, setDownloadProgress] = React.useState<
        number | null
    >(null)
    const downloadLock = React.useRef(false)
    const downloading = downloadProgress !== null

    const hasCompanionWorkbooks = previews.some((preview) =>
        shouldBundleDeliveryWorkbook(
            preview.lines,
            preview.fulfillmentResponsibility === "WAREHOUSE",
        ),
    )

    async function downloadImages() {
        if (downloadLock.current || previews.length === 0) return
        downloadLock.current = true
        setDownloadProgress(0)
        try {
            const { downloadPurchasePreviewImages } =
                await import("../lib/download-purchase-preview-images")
            const result = await downloadPurchasePreviewImages(
                previews,
                sourceOrder,
                setDownloadProgress,
            )
            if (!result.archived) {
                toast.add({
                    title: "采购单图片已下载",
                    description:
                        "已保存为供应商版 PNG 图片（已隐藏来源销售与客户资料），请在浏览器下载列表中查看。",
                    type: "success",
                })
            } else if (result.workbookCount > 0) {
                toast.add({
                    title: `已生成 ${result.imageCount} 张采购单图片与 ${result.workbookCount} 份地址表`,
                    description:
                        "已打包下载（主单图片已隐藏来源销售与客户资料），请在浏览器下载列表中查看。",
                    type: "success",
                })
            } else {
                toast.add({
                    title: `已生成 ${result.imageCount} 张采购单图片`,
                    description:
                        "供应商版图片已打包下载（已隐藏来源销售与客户资料），请在浏览器下载列表中查看。",
                    type: "success",
                })
            }
        } catch {
            toast.add({
                title: "采购单文件下载失败",
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

    // 统计参与本次提交商品中的价格倒挂预警（采购含税单价 > 销售含税单价）
    const marginAlerts = React.useMemo(() => {
        const items: {
            salesOrderLineId: string
            itemName: string
            unitCostGross: string
            salesUnitPriceGross: string
            absSpread: string
        }[] = []
        for (const row of scopeRows) {
            if (!row.selected && !row.partiallySelected) continue
            const summary = summarizeRowPricing(row)
            if (
                summary.hasInvertedSource &&
                summary.highestInvertedCost &&
                summary.salesUnitPriceGross
            ) {
                const cmp = comparePurchaseCostToSalesPrice(
                    summary.highestInvertedCost,
                    summary.salesUnitPriceGross,
                )
                items.push({
                    salesOrderLineId: row.product.salesOrderLineId,
                    itemName: row.product.itemName,
                    unitCostGross: summary.highestInvertedCost,
                    salesUnitPriceGross: summary.salesUnitPriceGross,
                    absSpread: cmp?.absSpread ?? "0",
                })
            }
        }
        return items
    }, [scopeRows])

    const invertedLineIds = React.useMemo(
        () => new Set(marginAlerts.map((item) => item.salesOrderLineId)),
        [marginAlerts],
    )

    const activePreview =
        previews.find((preview) => preview.key === activeKey) ?? previews[0]
    const activeIndex = activePreview
        ? previews.findIndex((preview) => preview.key === activePreview.key)
        : -1

    const activeAddressCount = activePreview
        ? countPurchaseDeliveryRows(
              activePreview.lines,
              activePreview.fulfillmentResponsibility === "WAREHOUSE",
          )
        : 0

    const showLeftRail =
        previews.length > 1 ||
        stockAllocations.length > 0 ||
        excluded.length > 0 ||
        partial.length > 0 ||
        marginAlerts.length > 0

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
        setRightTab("paper")
        if (previews.some((preview) => preview.key === activeKey)) return
        setActiveKey(previews[0]?.key ?? "")
    }, [activeKey, open, previews])

    React.useEffect(() => {
        if (rightTab === "addresses" && activeAddressCount === 0) {
            setRightTab("paper")
        }
        if (rightTab === "overview" && previews.length <= 1) {
            setRightTab("paper")
        }
    }, [activeAddressCount, previews.length, rightTab])

    const locateAndClose = (salesOrderLineId: string) => {
        if (!onLocateProduct) return
        onOpenChange(false)
        onLocateProduct(salesOrderLineId)
    }

    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!creating && !unresolved && !downloading) onOpenChange(next)
            }}
        >
            <DialogContent
                size="2xl"
                scroll="body"
                className="h-[92dvh]"
                closeButtonId="procurement-orders-create-preview-close"
                showCloseButton={!creating && !unresolved && !downloading}
            >
                <DialogHeader className="border-border/60 bg-card py-3 text-left">
                    <div className="flex flex-wrap items-center gap-2">
                        <DialogTitle>预览供给分配</DialogTitle>
                        {scopeRows.length > 0 ? (
                            <Badge variant="neutral" className="h-5">
                                本次分配 {scopeRows.length - excluded.length} /{" "}
                                {scopeRows.length} 项
                            </Badge>
                        ) : null}
                        {marginAlerts.length > 0 ? (
                            <Badge variant="destructive" className="h-5">
                                <TriangleAlertIcon aria-hidden="true" />
                                {marginAlerts.length} 项采购价高于售价
                            </Badge>
                        ) : null}
                    </div>
                    <DialogDescription>
                        {description ?? allocationSummary}
                    </DialogDescription>
                </DialogHeader>

                <DialogBody className="flex flex-col overflow-hidden p-0">
                    {actionError ? (
                        <div className="shrink-0 border-b border-border/60 px-overlay-padding py-3">
                            <Alert variant="destructive">
                                <AlertTitle>{actionError.title}</AlertTitle>
                                <AlertDescription>
                                    {actionError.description}
                                </AlertDescription>
                            </Alert>
                        </div>
                    ) : null}

                    <div className="flex min-h-0 flex-1 flex-col overflow-hidden md:flex-row">
                        {showLeftRail ? (
                            <aside
                                aria-label="分单与核对摘要"
                                className="flex max-h-56 w-full shrink-0 flex-col gap-3 overflow-y-auto overscroll-contain border-b border-border/60 bg-sidebar p-3 md:max-h-none md:w-76 md:border-r md:border-b-0"
                            >
                                {previews.length > 0 ? (
                                    <nav aria-label="将创建的采购单">
                                        <p className="mb-2 px-1 text-xs font-medium text-muted-foreground">
                                            将创建的采购单{" "}
                                            <span className="num">
                                                {previews.length}
                                            </span>
                                        </p>
                                        <ul className="space-y-1.5">
                                            {previews.map((preview, index) => {
                                                const active =
                                                    preview.key ===
                                                        activePreview?.key &&
                                                    rightTab !== "overview"
                                                const addrCount =
                                                    countPurchaseDeliveryRows(
                                                        preview.lines,
                                                        preview.fulfillmentResponsibility ===
                                                            "WAREHOUSE",
                                                    )
                                                const hasInverted =
                                                    preview.lines.some((line) =>
                                                        invertedLineIds.has(
                                                            line.salesOrderLineId,
                                                        ),
                                                    )
                                                return (
                                                    <li key={preview.key}>
                                                        <button
                                                            id={`procurement-orders-create-switch-${toAutomationIdSegment(preview.key)}`}
                                                            type="button"
                                                            data-testid={`purchase-create-switch-${preview.key}`}
                                                            aria-current={
                                                                active
                                                                    ? "true"
                                                                    : undefined
                                                            }
                                                            onClick={() => {
                                                                setActiveKey(
                                                                    preview.key,
                                                                )
                                                                if (
                                                                    rightTab ===
                                                                    "overview"
                                                                ) {
                                                                    setRightTab(
                                                                        "paper",
                                                                    )
                                                                }
                                                            }}
                                                            className={cn(
                                                                "w-full cursor-pointer rounded-lg border px-3 py-2 text-left transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                                                active
                                                                    ? "border-primary/50 bg-card shadow-xs ring-1 ring-primary/15"
                                                                    : "border-border/60 bg-card/60 hover:border-border hover:bg-card",
                                                            )}
                                                        >
                                                            <div className="flex items-start justify-between gap-2">
                                                                <span className="num min-w-0 truncate text-body-compact font-medium text-foreground">
                                                                    {index + 1}.{" "}
                                                                    {
                                                                        preview.supplierName
                                                                    }
                                                                </span>
                                                                {hasInverted ? (
                                                                    <TriangleAlertIcon
                                                                        aria-label="含价格倒挂行"
                                                                        className="mt-0.5 size-3.5 shrink-0 text-destructive"
                                                                    />
                                                                ) : null}
                                                            </div>
                                                            <p className="mt-0.5 truncate text-xs text-muted-foreground">
                                                                {
                                                                    preview.routeLabel
                                                                }
                                                                {" · "}
                                                                {
                                                                    preview.paymentTermLabel
                                                                }
                                                                {preview.targetWarehouseName
                                                                    ? ` · ${preview.targetWarehouseName}`
                                                                    : ""}
                                                            </p>
                                                            <div className="mt-1.5 flex items-baseline justify-between gap-2 border-t border-border/50 pt-1.5 text-xs">
                                                                <span className="num text-muted-foreground">
                                                                    {
                                                                        preview
                                                                            .lines
                                                                            .length
                                                                    }{" "}
                                                                    行
                                                                    {addrCount >
                                                                    0
                                                                        ? ` · ${addrCount} 地址`
                                                                        : ""}
                                                                </span>
                                                                <MoneyValue
                                                                    display="exact"
                                                                    value={
                                                                        preview
                                                                            .totals
                                                                            .gross
                                                                    }
                                                                    className="font-semibold text-foreground"
                                                                />
                                                            </div>
                                                        </button>
                                                    </li>
                                                )
                                            })}
                                        </ul>
                                    </nav>
                                ) : null}

                                {stockAllocations.length > 0 ? (
                                    <section
                                        aria-label="现有库存分配"
                                        className="rounded-lg border border-info-border/70 bg-info-soft/45 p-3"
                                    >
                                        <h3 className="flex items-center gap-1.5 text-xs font-semibold text-info-soft-foreground">
                                            <PackageCheckIcon
                                                aria-hidden="true"
                                                className="size-3.5 shrink-0"
                                            />
                                            现有库存分配
                                        </h3>
                                        <ul className="mt-2 space-y-1 text-xs text-foreground">
                                            {stockAllocations.map(
                                                (line, index) => (
                                                    <li
                                                        key={`${line.salesOrderLineId}:${line.warehouseName}:${index}`}
                                                        className="leading-5"
                                                    >
                                                        {line.itemName} ·{" "}
                                                        {line.warehouseName} ·{" "}
                                                        {line.quantity}{" "}
                                                        {line.unit}
                                                    </li>
                                                ),
                                            )}
                                        </ul>
                                    </section>
                                ) : null}

                                {marginAlerts.length > 0 ? (
                                    <section
                                        aria-label="价格倒挂风险提醒"
                                        className="rounded-lg border border-destructive-border bg-destructive-soft/50 p-3"
                                    >
                                        <h3 className="flex items-center gap-1.5 text-xs font-semibold text-destructive-soft-foreground">
                                            <TriangleAlertIcon
                                                aria-hidden="true"
                                                className="size-3.5 shrink-0"
                                            />
                                            采购价高于售价（
                                            {marginAlerts.length}）
                                        </h3>
                                        <ul className="mt-2 space-y-1.5 text-xs">
                                            {marginAlerts.map((alert) => (
                                                <li
                                                    key={alert.salesOrderLineId}
                                                    className="flex items-start justify-between gap-2"
                                                >
                                                    <div className="min-w-0">
                                                        <p className="truncate font-medium text-foreground">
                                                            {alert.itemName}
                                                        </p>
                                                        <p className="num text-tiny text-destructive-soft-foreground">
                                                            采 ¥
                                                            {
                                                                alert.unitCostGross
                                                            }{" "}
                                                            / 售 ¥
                                                            {
                                                                alert.salesUnitPriceGross
                                                            }
                                                            （高 ¥
                                                            {alert.absSpread}）
                                                        </p>
                                                    </div>
                                                    {onLocateProduct ? (
                                                        <Button
                                                            id={`procurement-orders-create-preview-fix-margin-${toAutomationIdSegment(alert.salesOrderLineId)}`}
                                                            type="button"
                                                            variant="ghost"
                                                            size="xs"
                                                            className="h-5 shrink-0 px-1.5 text-tiny"
                                                            onClick={() =>
                                                                locateAndClose(
                                                                    alert.salesOrderLineId,
                                                                )
                                                            }
                                                        >
                                                            回改
                                                        </Button>
                                                    ) : null}
                                                </li>
                                            ))}
                                        </ul>
                                    </section>
                                ) : null}

                                {excluded.length > 0 || partial.length > 0 ? (
                                    <section
                                        aria-label="未全量分配商品"
                                        className="rounded-lg border border-border/70 bg-card p-3"
                                    >
                                        <h3 className="text-xs font-semibold text-foreground">
                                            留待后续处理（
                                            {excluded.length + partial.length}）
                                        </h3>
                                        <ul className="mt-1.5 space-y-1.5 text-xs text-muted-foreground">
                                            {partial.map((row) => (
                                                <li
                                                    key={
                                                        row.product
                                                            .salesOrderLineId
                                                    }
                                                    className="flex items-center justify-between gap-2"
                                                >
                                                    <span className="min-w-0 truncate">
                                                        {row.product.itemName} ·
                                                        剩{" "}
                                                        <QuantityValue
                                                            value={
                                                                row.remainingQuantity
                                                            }
                                                            unit={
                                                                row.product.unit
                                                            }
                                                        />
                                                    </span>
                                                    {onLocateProduct ? (
                                                        <Button
                                                            id={`procurement-orders-create-preview-fix-partial-${toAutomationIdSegment(row.product.salesOrderLineId)}`}
                                                            type="button"
                                                            variant="ghost"
                                                            size="xs"
                                                            className="h-5 shrink-0 px-1.5 text-tiny"
                                                            onClick={() =>
                                                                locateAndClose(
                                                                    row.product
                                                                        .salesOrderLineId,
                                                                )
                                                            }
                                                        >
                                                            回改
                                                        </Button>
                                                    ) : null}
                                                </li>
                                            ))}
                                            {excluded.map((row) => (
                                                <li
                                                    key={
                                                        row.product
                                                            .salesOrderLineId
                                                    }
                                                    className="flex items-center justify-between gap-2"
                                                >
                                                    <span className="min-w-0 truncate">
                                                        {row.product.itemName} ·
                                                        暂不分配
                                                    </span>
                                                    {onLocateProduct ? (
                                                        <Button
                                                            id={`procurement-orders-create-preview-fix-excluded-${toAutomationIdSegment(row.product.salesOrderLineId)}`}
                                                            type="button"
                                                            variant="ghost"
                                                            size="xs"
                                                            className="h-5 shrink-0 px-1.5 text-tiny"
                                                            onClick={() =>
                                                                locateAndClose(
                                                                    row.product
                                                                        .salesOrderLineId,
                                                                )
                                                            }
                                                        >
                                                            纳入
                                                        </Button>
                                                    ) : null}
                                                </li>
                                            ))}
                                        </ul>
                                        <p className="mt-2 text-tiny text-muted-foreground">
                                            未分配需求本次不提交，待办将继续保留。
                                        </p>
                                    </section>
                                ) : null}
                            </aside>
                        ) : null}

                        <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
                            {activePreview &&
                            (activeAddressCount > 0 || previews.length > 1) ? (
                                <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border/60 bg-card px-4 py-2">
                                    <div
                                        role="group"
                                        aria-label="预览视图切换"
                                        className="flex flex-wrap items-center gap-1"
                                    >
                                        <Button
                                            id="procurement-orders-create-preview-tab-paper"
                                            type="button"
                                            size="xs"
                                            variant={
                                                rightTab === "paper"
                                                    ? "secondary"
                                                    : "ghost"
                                            }
                                            aria-pressed={rightTab === "paper"}
                                            onClick={() => setRightTab("paper")}
                                        >
                                            <FileTextIcon aria-hidden="true" />
                                            采购单纸面
                                        </Button>
                                        {activeAddressCount > 0 ? (
                                            <Button
                                                id="procurement-orders-create-preview-tab-addresses"
                                                type="button"
                                                size="xs"
                                                variant={
                                                    rightTab === "addresses"
                                                        ? "secondary"
                                                        : "ghost"
                                                }
                                                aria-pressed={
                                                    rightTab === "addresses"
                                                }
                                                onClick={() =>
                                                    setRightTab("addresses")
                                                }
                                            >
                                                <MapPinIcon aria-hidden="true" />
                                                收货地址明细（
                                                {activeAddressCount}）
                                            </Button>
                                        ) : null}
                                        {previews.length > 1 ? (
                                            <Button
                                                id="procurement-orders-create-preview-tab-overview"
                                                type="button"
                                                size="xs"
                                                variant={
                                                    rightTab === "overview"
                                                        ? "secondary"
                                                        : "ghost"
                                                }
                                                aria-pressed={
                                                    rightTab === "overview"
                                                }
                                                onClick={() =>
                                                    setRightTab("overview")
                                                }
                                            >
                                                <LayoutGridIcon aria-hidden="true" />
                                                分单总览（{previews.length}）
                                            </Button>
                                        ) : null}
                                    </div>
                                    <span className="num text-xs text-muted-foreground">
                                        {rightTab === "overview"
                                            ? `共 ${previews.length} 张采购单`
                                            : `${activeIndex + 1} / ${previews.length} · ${activePreview.supplierName}`}
                                    </span>
                                </div>
                            ) : null}

                            {rightTab === "overview" && previews.length > 1 ? (
                                <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain p-5">
                                    <div className="overflow-hidden rounded-lg border border-border/80 bg-card">
                                        <Table data-density="compact">
                                            <TableHeader>
                                                <TableRow>
                                                    <TableHead>
                                                        采购单 / 供应商
                                                    </TableHead>
                                                    <TableHead>
                                                        履约 / 目标仓
                                                    </TableHead>
                                                    <TableHead>
                                                        采购类型 / 付款
                                                    </TableHead>
                                                    <TableHead data-align="end">
                                                        明细 / 地址
                                                    </TableHead>
                                                    <TableHead data-align="end">
                                                        含税合计
                                                    </TableHead>
                                                    <TableHead data-align="end">
                                                        查看
                                                    </TableHead>
                                                </TableRow>
                                            </TableHeader>
                                            <TableBody>
                                                {previews.map(
                                                    (preview, index) => {
                                                        const addrCount =
                                                            countPurchaseDeliveryRows(
                                                                preview.lines,
                                                                preview.fulfillmentResponsibility ===
                                                                    "WAREHOUSE",
                                                            )
                                                        return (
                                                            <TableRow
                                                                key={
                                                                    preview.key
                                                                }
                                                            >
                                                                <TableCell className="font-medium">
                                                                    {index + 1}.{" "}
                                                                    {
                                                                        preview.supplierName
                                                                    }
                                                                </TableCell>
                                                                <TableCell>
                                                                    <div>
                                                                        {
                                                                            preview.routeLabel
                                                                        }
                                                                    </div>
                                                                    {preview.targetWarehouseName ? (
                                                                        <div className="text-xs text-muted-foreground">
                                                                            {
                                                                                preview.targetWarehouseName
                                                                            }
                                                                        </div>
                                                                    ) : null}
                                                                </TableCell>
                                                                <TableCell className="text-xs text-muted-foreground">
                                                                    {
                                                                        PURCHASE_TYPE_LABEL[
                                                                            preview
                                                                                .purchaseType
                                                                        ]
                                                                    }
                                                                    {" · "}
                                                                    {
                                                                        preview.paymentTermLabel
                                                                    }
                                                                </TableCell>
                                                                <TableCell
                                                                    data-align="end"
                                                                    className="num text-xs"
                                                                >
                                                                    {
                                                                        preview
                                                                            .lines
                                                                            .length
                                                                    }{" "}
                                                                    行
                                                                    {addrCount >
                                                                    0
                                                                        ? ` / ${addrCount} 址`
                                                                        : ""}
                                                                </TableCell>
                                                                <TableCell data-align="end">
                                                                    <MoneyValue
                                                                        display="exact"
                                                                        value={
                                                                            preview
                                                                                .totals
                                                                                .gross
                                                                        }
                                                                        className="font-semibold"
                                                                    />
                                                                </TableCell>
                                                                <TableCell data-align="end">
                                                                    <Button
                                                                        id={`procurement-orders-create-overview-open-${toAutomationIdSegment(preview.key)}`}
                                                                        type="button"
                                                                        variant="ghost"
                                                                        size="xs"
                                                                        onClick={() => {
                                                                            setActiveKey(
                                                                                preview.key,
                                                                            )
                                                                            setRightTab(
                                                                                "paper",
                                                                            )
                                                                        }}
                                                                    >
                                                                        纸面
                                                                        <ArrowUpRightIcon aria-hidden="true" />
                                                                    </Button>
                                                                </TableCell>
                                                            </TableRow>
                                                        )
                                                    },
                                                )}
                                            </TableBody>
                                        </Table>
                                    </div>
                                </div>
                            ) : rightTab === "addresses" && activePreview ? (
                                <PurchaseDeliveryPlanExplorer
                                    lines={activePreview.lines}
                                    supplierName={activePreview.supplierName}
                                    documentLabel={`预览 ${Math.max(activeIndex, 0) + 1}`}
                                    idPrefix={`procurement-orders-create-preview-addr-${toAutomationIdSegment(activePreview.key)}`}
                                />
                            ) : (
                                <PaperDocumentViewport
                                    fitMode="width"
                                    role="region"
                                    aria-label="采购单预览"
                                    tabIndex={0}
                                    fitKey={activePreview?.key ?? "empty"}
                                >
                                    {activePreview ? (
                                        <PurchaseOrderPreviewPaper
                                            preview={activePreview}
                                            index={Math.max(activeIndex, 0)}
                                            sourceOrder={sourceOrder}
                                            onInspectAddresses={
                                                activeAddressCount > 0
                                                    ? () =>
                                                          setRightTab(
                                                              "addresses",
                                                          )
                                                    : undefined
                                            }
                                        />
                                    ) : (
                                        <p className="text-xs text-muted-foreground">
                                            本次全部由现有库存满足，不会创建采购单。
                                        </p>
                                    )}
                                </PaperDocumentViewport>
                            )}
                        </div>
                    </div>
                </DialogBody>

                <DialogFooter className="flex-wrap">
                    {previews.length > 0 ? (
                        <LoadingButton
                            id="procurement-orders-create-preview-download"
                            type="button"
                            variant="outline"
                            className="sm:mr-auto"
                            loading={downloading}
                            disabled={creating || unresolved || downloading}
                            onClick={() => void downloadImages()}
                            title={
                                hasCompanionWorkbooks
                                    ? "地址较多的采购单将导出清晰主单 PNG 与 Excel 发货地址明细表并打包下载（已隐藏来源销售与客户资料）"
                                    : previews.length === 1
                                      ? "保存为供应商版 PNG 图片（已隐藏来源销售与客户资料）"
                                      : "每张采购单保存为供应商版 PNG 图片并打包下载（已隐藏来源销售与客户资料）"
                            }
                        >
                            <DownloadIcon data-icon="inline-start" />
                            {downloading
                                ? `生成中 ${downloadProgress}/${previews.length}`
                                : hasCompanionWorkbooks
                                  ? previews.length === 1
                                      ? "下载采购单及地址表"
                                      : `下载全部采购单及地址表（${previews.length}）`
                                  : previews.length === 1
                                    ? "下载采购单图片"
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
