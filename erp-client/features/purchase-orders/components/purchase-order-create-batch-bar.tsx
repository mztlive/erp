"use client"

import * as React from "react"
import {
    CalendarDaysIcon,
    CheckCircle2Icon,
    CheckSquareIcon,
    CircleIcon,
    PauseCircleIcon,
    ReplaceIcon,
    RotateCcwIcon,
    SparklesIcon,
    WarehouseIcon,
    XIcon,
} from "lucide-react"
import { FulfillmentRouteBadge, MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { DatePicker } from "@/components/ui/date-picker"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    DialogTrigger,
} from "@/components/ui/dialog"
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { WarehouseSearchCombobox } from "@/features/entity-selectors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { multiplyFixed, sumFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type {
    SourcingSalesOrder,
    SourcingSupplierOption,
} from "../lib/purchase-order-create-model"
import { findSourcingOption } from "../lib/purchase-order-create-model"
import { sourcingRouteLabel } from "../lib/sourcing/delivery-terms"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"

export type PurchaseOrderCreateUtilityActionsProps = {
    disabled?: boolean
    matchDisabled?: boolean
    onMatchBest: () => void
    onReuseInquiry: () => void
    inquiryUnavailableReason?: string
    canUndo?: boolean
    onUndo?: () => void
}

/** 常态顶部快捷动作：保持克制，仅展示当前单据真正可用的辅助动作，避免按钮堆砌。 */
export function PurchaseOrderCreateUtilityActions({
    disabled,
    matchDisabled,
    onMatchBest,
    onReuseInquiry,
    inquiryUnavailableReason,
    canUndo,
    onUndo,
}: PurchaseOrderCreateUtilityActionsProps) {
    const canReuseInquiry = !disabled && !inquiryUnavailableReason
    return (
        <div className="flex flex-wrap items-center gap-1.5">
            {canUndo && onUndo ? (
                <Tooltip>
                    <TooltipTrigger
                        render={
                            <Button
                                id="procurement-orders-create-batch-undo"
                                type="button"
                                variant="ghost"
                                size="sm"
                                className="text-muted-foreground hover:text-foreground"
                                onClick={onUndo}
                            />
                        }
                    >
                        <RotateCcwIcon aria-hidden="true" />
                        撤销
                    </TooltipTrigger>
                    <TooltipContent>撤销上次批量更改</TooltipContent>
                </Tooltip>
            ) : null}

            {canReuseInquiry ? (
                <Tooltip>
                    <TooltipTrigger
                        render={
                            <Button
                                id="procurement-orders-create-reuse-inquiry"
                                type="button"
                                variant="ghost"
                                size="sm"
                                onClick={onReuseInquiry}
                            />
                        }
                    >
                        沿用询价
                    </TooltipTrigger>
                    <TooltipContent>
                        将本单询价报价应用到参与分配的商品（保留现有库存分配）。
                    </TooltipContent>
                </Tooltip>
            ) : null}

            <Tooltip>
                <TooltipTrigger
                    render={
                        <Button
                            id="procurement-orders-create-batch-match"
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={disabled || matchDisabled}
                            onClick={onMatchBest}
                            data-testid="purchase-create-match-best"
                        />
                    }
                >
                    <SparklesIcon aria-hidden="true" />
                    重新自动分配
                </TooltipTrigger>
                <TooltipContent>
                    将本次下单商品重置为系统推荐方案（优先现有库存与供应商直发）。
                </TooltipContent>
            </Tooltip>
        </div>
    )
}

export type PurchaseOrderCreateSelectionBarProps = {
    checkedRows: readonly SourcingEditorRow[]
    visibleRowIds: readonly string[]
    disabled?: boolean
    onSelectAllVisible: () => void
    onClearSelection: () => void
    onBatchApplySource: (
        basisId: string,
        warehouse?: { id: string; name: string },
    ) => void
    onBatchApplyWarehouse: (warehouse: { id: string; name: string }) => void
    onBatchApplyDeliveryDate: (date: string, clampToDeadline: boolean) => void
    onBatchSetParticipation: (included: boolean) => void
}

/**
 * 批量选区操作条：勾选 ≥1 个商品后出现，与「是否纳入本次提交」完全解耦；
 * 支持批量指定来源（连带目标仓与影响预览）、批量设目标仓、批量改交期与批量切参与态。
 */
export function PurchaseOrderCreateSelectionBar({
    checkedRows,
    visibleRowIds,
    disabled,
    onSelectAllVisible,
    onClearSelection,
    onBatchApplySource,
    onBatchApplyWarehouse,
    onBatchApplyDeliveryDate,
    onBatchSetParticipation,
}: PurchaseOrderCreateSelectionBarProps) {
    const [sourceDialogOpen, setSourceDialogOpen] = React.useState(false)
    const [selectedBasisId, setSelectedBasisId] = React.useState<string>("")
    const [batchSourceWarehouseId, setBatchSourceWarehouseId] =
        React.useState("")
    const [batchSourceWarehouseName, setBatchSourceWarehouseName] =
        React.useState("")

    const [warehouseDialogOpen, setWarehouseDialogOpen] = React.useState(false)
    const [targetWarehouseId, setTargetWarehouseId] = React.useState("")
    const [targetWarehouseName, setTargetWarehouseName] = React.useState("")

    const [dateDialogOpen, setDateDialogOpen] = React.useState(false)
    const [batchDate, setBatchDate] = React.useState<string>("")
    const [clampToDeadline, setClampToDeadline] = React.useState(true)

    const checkedIds = React.useMemo(
        () => new Set(checkedRows.map((row) => row.product.salesOrderLineId)),
        [checkedRows],
    )
    const allVisibleChecked =
        visibleRowIds.length > 0 &&
        visibleRowIds.every((id) => checkedIds.has(id))

    // 统计勾选商品中所有可选方案的并集，并计算可覆盖数与预估含税小计
    const candidateGroups = React.useMemo(() => {
        const map = new Map<
            string,
            {
                option: SourcingSupplierOption
                applicableRows: SourcingEditorRow[]
                estimatedGross: string
            }
        >()
        for (const row of checkedRows) {
            for (const option of row.product.options) {
                if (!option.basisId) continue
                const existing = map.get(option.basisId)
                if (existing) {
                    existing.applicableRows.push(row)
                } else {
                    map.set(option.basisId, {
                        option,
                        applicableRows: [row],
                        estimatedGross: "0.00",
                    })
                }
            }
        }
        return [...map.values()]
            .map((entry) => {
                if (entry.option.sourceType === "EXISTING_STOCK") return entry
                const amounts: string[] = []
                for (const row of entry.applicableRows) {
                    const matched = findSourcingOption(
                        row.product,
                        entry.option.basisId,
                    )
                    if (!matched) continue
                    try {
                        amounts.push(
                            multiplyFixed(
                                matched.unitCostGross,
                                matched.maxCreateQuantity,
                                {
                                    leftMaxScale: 4,
                                    rightMaxScale: 6,
                                    outputScale: 2,
                                },
                            ),
                        )
                    } catch {
                        // ignore invalid decimal
                    }
                }
                return {
                    ...entry,
                    estimatedGross:
                        amounts.length > 0
                            ? sumFixed(amounts, { maxScale: 2, outputScale: 2 })
                            : "0.00",
                }
            })
            .sort(
                (a, b) =>
                    b.applicableRows.length - a.applicableRows.length ||
                    a.option.supplierName.localeCompare(
                        b.option.supplierName,
                        "zh-CN",
                    ),
            )
    }, [checkedRows])

    React.useEffect(() => {
        if (!sourceDialogOpen) return
        if (
            !selectedBasisId ||
            !candidateGroups.some((g) => g.option.basisId === selectedBasisId)
        ) {
            setSelectedBasisId(candidateGroups[0]?.option.basisId ?? "")
        }
    }, [candidateGroups, selectedBasisId, sourceDialogOpen])

    const activeGroup = candidateGroups.find(
        (group) => group.option.basisId === selectedBasisId,
    )
    const activeNeedsWarehouse =
        activeGroup?.option.sourceType === "PURCHASE" &&
        activeGroup.option.fulfillmentResponsibility === "WAREHOUSE"

    // 统计勾选行中支持「批量设目标仓」和「批量改交期」的行数
    const warehouseApplicableCount = React.useMemo(
        () =>
            checkedRows.filter((row) =>
                row.allocations.some(({ line }) => {
                    const opt = findSourcingOption(row.product, line.basisId)
                    return (
                        opt?.sourceType === "PURCHASE" &&
                        opt.fulfillmentResponsibility === "WAREHOUSE"
                    )
                }),
            ).length,
        [checkedRows],
    )

    const purchaseApplicableCount = React.useMemo(
        () =>
            checkedRows.filter((row) =>
                row.allocations.some(({ line }) => {
                    const opt = findSourcingOption(row.product, line.basisId)
                    return opt?.sourceType === "PURCHASE"
                }),
            ).length,
        [checkedRows],
    )

    const lateDateCount = React.useMemo(() => {
        if (!batchDate) return 0
        return checkedRows.filter(
            (row) =>
                row.product.deliveryDeadline &&
                batchDate > row.product.deliveryDeadline,
        ).length
    }, [batchDate, checkedRows])

    const excludedCheckedCount = checkedRows.filter(
        (row) => !row.selected && !row.partiallySelected,
    ).length
    const includedCheckedCount = checkedRows.length - excludedCheckedCount

    return (
        <div
            data-slot="sourcing-batch-selection-bar"
            className="flex flex-wrap items-center justify-between gap-2"
        >
            <div className="flex flex-wrap items-center gap-2 text-xs">
                <span className="font-semibold text-info-soft-foreground">
                    已选 <span className="num">{checkedRows.length}</span>{" "}
                    个商品
                </span>
                {!allVisibleChecked &&
                visibleRowIds.length > checkedRows.length ? (
                    <Button
                        id="procurement-orders-create-batch-select-visible"
                        type="button"
                        variant="link"
                        size="xs"
                        className="h-auto px-1 text-xs"
                        onClick={onSelectAllVisible}
                    >
                        全选当前筛选（{visibleRowIds.length}）
                    </Button>
                ) : null}
                <Button
                    id="procurement-orders-create-batch-clear-selection"
                    type="button"
                    variant="ghost"
                    size="xs"
                    className="h-6 px-1.5 text-xs text-muted-foreground hover:text-foreground"
                    onClick={onClearSelection}
                >
                    <XIcon aria-hidden="true" />
                    取消选择
                </Button>
            </div>

            <div className="flex flex-wrap items-center gap-1.5">
                {/* 1. 更换供应商（带覆盖预览与连带目标仓） */}
                <Dialog
                    open={sourceDialogOpen}
                    onOpenChange={(open) => {
                        setSourceDialogOpen(open)
                        if (!open) {
                            setBatchSourceWarehouseId("")
                            setBatchSourceWarehouseName("")
                        }
                    }}
                >
                    <DialogTrigger
                        id="procurement-orders-create-batch-toggle"
                        render={
                            <Button
                                type="button"
                                size="xs"
                                variant="outline"
                                className="bg-card"
                                disabled={disabled || checkedRows.length === 0}
                            />
                        }
                    >
                        <ReplaceIcon aria-hidden="true" />
                        更换供应商
                    </DialogTrigger>
                    <DialogContent
                        size="lg"
                        scroll="body"
                        closeButtonId="procurement-orders-create-batch-close"
                    >
                        <DialogHeader>
                            <DialogTitle>批量指定供给来源</DialogTitle>
                            <DialogDescription>
                                已选 {checkedRows.length}{" "}
                                个商品。仅更新支持所选方案的商品（保留参与/暂缓状态），不支持该方案的商品保持原样。
                            </DialogDescription>
                        </DialogHeader>
                        <DialogBody className="space-y-4">
                            {candidateGroups.length === 0 ? (
                                <p className="rounded-lg border border-dashed border-border bg-surface-sunken px-4 py-6 text-center text-xs text-muted-foreground">
                                    所选商品暂无可指定的合格供给方案。
                                </p>
                            ) : (
                                <div className="space-y-2">
                                    <p className="text-xs font-medium text-foreground/80">
                                        选择目标供给方案（共{" "}
                                        {candidateGroups.length} 个候选）
                                    </p>
                                    <ul className="max-h-56 divide-y divide-grid overflow-y-auto rounded-lg border border-border/80">
                                        {candidateGroups.map((group) => {
                                            const opt = group.option
                                            const active =
                                                opt.basisId === selectedBasisId
                                            const isStock =
                                                opt.sourceType ===
                                                "EXISTING_STOCK"
                                            const label = isStock
                                                ? opt.warehouseName ||
                                                  opt.supplierName
                                                : opt.supplierName
                                            return (
                                                <li key={opt.basisId}>
                                                    <button
                                                        id={`procurement-orders-create-batch-opt-${toAutomationIdSegment(opt.basisId)}`}
                                                        type="button"
                                                        aria-pressed={active}
                                                        onClick={() =>
                                                            setSelectedBasisId(
                                                                opt.basisId,
                                                            )
                                                        }
                                                        className={cn(
                                                            "flex w-full cursor-pointer items-center justify-between gap-3 px-3.5 py-2.5 text-left transition-colors outline-none hover:bg-row-hover focus-visible:ring-2 focus-visible:ring-ring",
                                                            active &&
                                                                "bg-sidebar-accent/70",
                                                        )}
                                                    >
                                                        <div className="flex min-w-0 items-center gap-2.5">
                                                            {active ? (
                                                                <CheckCircle2Icon
                                                                    aria-hidden="true"
                                                                    className="size-4 shrink-0 text-primary"
                                                                />
                                                            ) : (
                                                                <CircleIcon
                                                                    aria-hidden="true"
                                                                    className="size-4 shrink-0 text-muted-foreground/50"
                                                                />
                                                            )}
                                                            <div className="min-w-0">
                                                                <div className="flex flex-wrap items-center gap-1.5">
                                                                    <span className="truncate text-body-compact font-medium text-foreground">
                                                                        {label}
                                                                    </span>
                                                                    <FulfillmentRouteBadge
                                                                        option={
                                                                            opt
                                                                        }
                                                                        label={sourcingRouteLabel(
                                                                            opt,
                                                                        )}
                                                                        size="compact"
                                                                    />
                                                                    {opt.inquirySource ? (
                                                                        <Badge
                                                                            variant="violet"
                                                                            className="h-4.5 px-1.5 text-tiny"
                                                                        >
                                                                            询价
                                                                        </Badge>
                                                                    ) : null}
                                                                </div>
                                                                {!isStock &&
                                                                opt.paymentTermLabel ? (
                                                                    <p className="mt-0.5 text-xs text-muted-foreground">
                                                                        付款条件{" "}
                                                                        {
                                                                            opt.paymentTermLabel
                                                                        }
                                                                    </p>
                                                                ) : null}
                                                            </div>
                                                        </div>
                                                        <div className="shrink-0 text-right">
                                                            <p className="num text-xs font-medium text-foreground">
                                                                可覆盖{" "}
                                                                {
                                                                    group
                                                                        .applicableRows
                                                                        .length
                                                                }{" "}
                                                                /{" "}
                                                                {
                                                                    checkedRows.length
                                                                }{" "}
                                                                项
                                                            </p>
                                                            {!isStock ? (
                                                                <p className="mt-0.5 text-xs text-muted-foreground">
                                                                    估{" "}
                                                                    <MoneyValue
                                                                        display="exact"
                                                                        value={
                                                                            group.estimatedGross
                                                                        }
                                                                    />
                                                                </p>
                                                            ) : (
                                                                <p className="mt-0.5 text-xs text-muted-foreground">
                                                                    库存核算
                                                                </p>
                                                            )}
                                                        </div>
                                                    </button>
                                                </li>
                                            )
                                        })}
                                    </ul>
                                </div>
                            )}

                            {activeNeedsWarehouse ? (
                                <div className="space-y-1.5 rounded-lg border border-border/70 bg-surface-sunken p-3">
                                    <p className="text-xs font-medium text-foreground">
                                        同时指定采购入库目标仓（推荐）
                                    </p>
                                    <p className="text-xs text-muted-foreground">
                                        所选方案为「集采入仓」，可在此一步指定目标仓，避免批量切换后逐行补选。
                                    </p>
                                    <WarehouseSearchCombobox
                                        id="procurement-orders-create-batch-source-warehouse"
                                        value={
                                            batchSourceWarehouseId || undefined
                                        }
                                        onValueChange={(id) =>
                                            setBatchSourceWarehouseId(id ?? "")
                                        }
                                        onItemChange={(warehouse) =>
                                            setBatchSourceWarehouseName(
                                                warehouse
                                                    ? `${warehouse.warehouseCode} · ${warehouse.warehouseName}`
                                                    : "",
                                            )
                                        }
                                        purpose="purchase-receipt"
                                        placeholder="选择目标仓（可选）"
                                        emptyLabel="没有可用仓库"
                                        aria-label="批量指定来源时的入库目标仓"
                                    />
                                </div>
                            ) : null}

                            {activeGroup ? (
                                <div className="space-y-1.5">
                                    <p className="text-xs font-medium text-foreground/80">
                                        影响明细预览（更新{" "}
                                        {activeGroup.applicableRows.length} 项
                                        {checkedRows.length >
                                        activeGroup.applicableRows.length
                                            ? ` · 跳过 ${checkedRows.length - activeGroup.applicableRows.length} 项`
                                            : ""}
                                        ）
                                    </p>
                                    <ul className="max-h-40 divide-y divide-grid overflow-y-auto rounded-lg border border-border/60 bg-surface-sunken/50 text-xs">
                                        {checkedRows.map((row) => {
                                            const matched = findSourcingOption(
                                                row.product,
                                                activeGroup.option.basisId,
                                            )
                                            return (
                                                <li
                                                    key={
                                                        row.product
                                                            .salesOrderLineId
                                                    }
                                                    className="flex items-center justify-between gap-3 px-3 py-1.5"
                                                >
                                                    <span className="min-w-0 truncate text-foreground">
                                                        {row.product.itemName}
                                                    </span>
                                                    {matched ? (
                                                        <span className="shrink-0 text-success-soft-foreground">
                                                            将更新
                                                            {matched.sourceType ===
                                                            "PURCHASE" ? (
                                                                <>
                                                                    {" · "}
                                                                    <MoneyValue
                                                                        display="exact"
                                                                        value={
                                                                            matched.unitCostGross
                                                                        }
                                                                    />
                                                                </>
                                                            ) : (
                                                                " · 库存"
                                                            )}
                                                        </span>
                                                    ) : (
                                                        <span className="shrink-0 text-muted-foreground">
                                                            不支持，保留原方案
                                                        </span>
                                                    )}
                                                </li>
                                            )
                                        })}
                                    </ul>
                                </div>
                            ) : null}
                        </DialogBody>
                        <DialogFooter>
                            <Button
                                id="procurement-orders-create-batch-cancel"
                                type="button"
                                variant="outline"
                                onClick={() => setSourceDialogOpen(false)}
                            >
                                取消
                            </Button>
                            <Button
                                id="procurement-orders-create-batch-apply"
                                type="button"
                                disabled={
                                    disabled ||
                                    !activeGroup ||
                                    activeGroup.applicableRows.length === 0
                                }
                                data-testid="purchase-create-batch-apply"
                                onClick={() => {
                                    if (!activeGroup) return
                                    onBatchApplySource(
                                        activeGroup.option.basisId,
                                        activeNeedsWarehouse &&
                                            batchSourceWarehouseId
                                            ? {
                                                  id: batchSourceWarehouseId,
                                                  name: batchSourceWarehouseName,
                                              }
                                            : undefined,
                                    )
                                    setSourceDialogOpen(false)
                                }}
                            >
                                应用到 {activeGroup?.applicableRows.length ?? 0}{" "}
                                个商品
                            </Button>
                        </DialogFooter>
                    </DialogContent>
                </Dialog>

                {/* 2. 设目标仓（仅当已选商品含「集采入仓」项时展示，避免无用灰按钮） */}
                {warehouseApplicableCount > 0 ? (
                    <Dialog
                        open={warehouseDialogOpen}
                        onOpenChange={(open) => {
                            setWarehouseDialogOpen(open)
                            if (!open) {
                                setTargetWarehouseId("")
                                setTargetWarehouseName("")
                            }
                        }}
                    >
                        <DialogTrigger
                            id="procurement-orders-create-batch-warehouse-toggle"
                            render={
                                <Button
                                    type="button"
                                    size="xs"
                                    variant="outline"
                                    className="bg-card"
                                    disabled={disabled}
                                />
                            }
                        >
                            <WarehouseIcon aria-hidden="true" />
                            设目标仓（{warehouseApplicableCount}）
                        </DialogTrigger>
                        <DialogContent
                            size="sm"
                            closeButtonId="procurement-orders-create-batch-warehouse-close"
                        >
                            <DialogHeader>
                                <DialogTitle>批量设置入库目标仓</DialogTitle>
                                <DialogDescription>
                                    将为已选商品中的 {warehouseApplicableCount}{" "}
                                    个「集采入仓」商品统一设置目标仓。
                                </DialogDescription>
                            </DialogHeader>
                            <WarehouseSearchCombobox
                                id="procurement-orders-create-batch-warehouse-input"
                                value={targetWarehouseId || undefined}
                                onValueChange={(id) =>
                                    setTargetWarehouseId(id ?? "")
                                }
                                onItemChange={(warehouse) =>
                                    setTargetWarehouseName(
                                        warehouse
                                            ? `${warehouse.warehouseCode} · ${warehouse.warehouseName}`
                                            : "",
                                    )
                                }
                                purpose="purchase-receipt"
                                placeholder="搜索并选择入库目标仓"
                                emptyLabel="没有可用仓库"
                                aria-label="批量设置入库目标仓"
                            />
                            <DialogFooter>
                                <Button
                                    id="procurement-orders-create-batch-warehouse-cancel"
                                    type="button"
                                    variant="outline"
                                    onClick={() =>
                                        setWarehouseDialogOpen(false)
                                    }
                                >
                                    取消
                                </Button>
                                <Button
                                    id="procurement-orders-create-batch-warehouse-apply"
                                    type="button"
                                    disabled={!targetWarehouseId}
                                    onClick={() => {
                                        if (!targetWarehouseId) return
                                        onBatchApplyWarehouse({
                                            id: targetWarehouseId,
                                            name: targetWarehouseName,
                                        })
                                        setWarehouseDialogOpen(false)
                                    }}
                                >
                                    应用到 {warehouseApplicableCount} 项
                                </Button>
                            </DialogFooter>
                        </DialogContent>
                    </Dialog>
                ) : null}

                {/* 3. 改交期（仅当已选商品含采购项时展示） */}
                {purchaseApplicableCount > 0 ? (
                    <Dialog
                        open={dateDialogOpen}
                        onOpenChange={(open) => {
                            setDateDialogOpen(open)
                            if (!open) setBatchDate("")
                        }}
                    >
                        <DialogTrigger
                            id="procurement-orders-create-batch-date-toggle"
                            render={
                                <Button
                                    type="button"
                                    size="xs"
                                    variant="outline"
                                    className="bg-card"
                                    disabled={disabled}
                                />
                            }
                        >
                            <CalendarDaysIcon aria-hidden="true" />
                            改交期
                        </DialogTrigger>
                        <DialogContent
                            size="sm"
                            closeButtonId="procurement-orders-create-batch-date-close"
                        >
                            <DialogHeader>
                                <DialogTitle>批量修改预计交付日</DialogTitle>
                                <DialogDescription>
                                    将为已选中的 {purchaseApplicableCount}{" "}
                                    个采购商品设置统一预计交付日（现有库存行不受影响）。
                                </DialogDescription>
                            </DialogHeader>
                            <div className="space-y-3">
                                <DatePicker
                                    id="procurement-orders-create-batch-date-input"
                                    value={batchDate || undefined}
                                    onValueChange={(next) =>
                                        setBatchDate(next ?? "")
                                    }
                                    placeholder="选择预计交付日"
                                    clearable={false}
                                />
                                {lateDateCount > 0 ? (
                                    <p className="text-xs text-warning-soft-foreground">
                                        其中 {lateDateCount}{" "}
                                        个商品的销售承诺最晚交期早于 {batchDate}
                                        。
                                    </p>
                                ) : null}
                                <label
                                    htmlFor="procurement-orders-create-batch-date-clamp"
                                    className="flex cursor-pointer items-center gap-2 text-xs text-muted-foreground select-none"
                                >
                                    <Checkbox
                                        id="procurement-orders-create-batch-date-clamp"
                                        checked={clampToDeadline}
                                        onCheckedChange={(checked) =>
                                            setClampToDeadline(checked === true)
                                        }
                                    />
                                    超过单品最晚承诺交期时，自动按该商品最晚交期截断
                                </label>
                            </div>
                            <DialogFooter>
                                <Button
                                    id="procurement-orders-create-batch-date-cancel"
                                    type="button"
                                    variant="outline"
                                    onClick={() => setDateDialogOpen(false)}
                                >
                                    取消
                                </Button>
                                <Button
                                    id="procurement-orders-create-batch-date-apply"
                                    type="button"
                                    disabled={!batchDate}
                                    onClick={() => {
                                        if (!batchDate) return
                                        onBatchApplyDeliveryDate(
                                            batchDate,
                                            clampToDeadline,
                                        )
                                        setDateDialogOpen(false)
                                    }}
                                >
                                    应用到 {purchaseApplicableCount} 项
                                </Button>
                            </DialogFooter>
                        </DialogContent>
                    </Dialog>
                ) : null}

                {excludedCheckedCount > 0 ? (
                    <Button
                        id="procurement-orders-create-batch-include"
                        type="button"
                        size="xs"
                        variant="outline"
                        className="bg-card"
                        disabled={disabled}
                        onClick={() => onBatchSetParticipation(true)}
                    >
                        <CheckSquareIcon aria-hidden="true" />
                        纳入本次分配
                    </Button>
                ) : null}
                {includedCheckedCount > 0 ? (
                    <Button
                        id="procurement-orders-create-batch-exclude"
                        type="button"
                        size="xs"
                        variant="outline"
                        className="bg-card"
                        disabled={disabled}
                        onClick={() => onBatchSetParticipation(false)}
                    >
                        <PauseCircleIcon aria-hidden="true" />
                        暂不分配
                    </Button>
                ) : null}
            </div>
        </div>
    )
}

/** 保持向后兼容的类型与默认导出别名，供调用方按需引入。 */
export { orderHasAnyInquiry }
function orderHasAnyInquiry(order: SourcingSalesOrder): boolean {
    return order.lines.some((line) =>
        line.options.some((option) => Boolean(option.inquirySource)),
    )
}
