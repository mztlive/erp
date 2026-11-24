"use client"

import { useEffect, useMemo, useRef, useState } from "react"
import {
    ArrowLeftIcon,
    ArrowRightIcon,
    ChevronDownIcon,
    InfoIcon,
    LayersIcon,
    MapPinIcon,
    PlusIcon,
    RotateCcwIcon,
    TriangleAlertIcon,
    XIcon,
} from "lucide-react"
import {
    DetailWorkspaceMeta,
    MoneyValue,
    QuantityValue,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import {
    findSourcingOption,
    pickBestSourcingOption,
} from "../lib/purchase-order-create-model"
import { sourcingFormValidationError } from "../lib/purchase-order-create-validation"
import { canSplitSourcingProduct } from "../lib/sourcing-quantity"
import {
    buildFillDirectShortfallPatches,
    buildTrimDirectExcessPatches,
    computeGlobalAddressState,
    isPositiveQuantity,
    type LineUpdatePatch,
} from "../lib/sourcing/address-grouping"
import { usesAddressQuantity } from "../lib/sourcing/delivery-editor"
import { sourcingRouteLabel } from "../lib/sourcing/delivery-terms"
import { buildSourcingEditorRows } from "../lib/sourcing/editor-rows"
import {
    sourceAccentForIndex,
    sourceRouteBadgeVariant,
} from "../lib/sourcing/source-palette"
import type { SourcingLineInput } from "../lib/sourcing/types"
import type {
    SourcingEditorProps,
    SourcingParticipationProps,
} from "./create-sourcing/types"
import { SourcingAddressEditor } from "./sourcing-address-editor"
import { SourcingAllocationSummary } from "./sourcing-allocation-summary"
import { SourcingDeliveryRequirementSection } from "./sourcing-delivery-plan"
import { SourcingOptionCompareMatrix } from "./sourcing-option-compare-matrix"
import { SourcingParticipationControl } from "./sourcing-participation-control"
import { SourcingSourceRow } from "./sourcing-source-row"

function statusTone(status: string): StatusTone {
    if (status === "已就绪") return "success"
    if (status === "部分分配" || status === "缺目标仓") return "warning"
    if (status === "待调整") return "destructive"
    return "neutral"
}

function cloneProductLines(
    lines: readonly SourcingLineInput[],
    salesOrderLineId: string,
): SourcingLineInput[] {
    return lines
        .filter((line) => line.salesOrderLineId === salesOrderLineId)
        .map((line) => ({
            ...line,
            deliveryAllocations: line.deliveryAllocations?.map((item) => ({
                ...item,
            })),
        }))
}

/**
 * 一体化多源协同供给分配弹窗：
 * - 顶部固定「商品身份 + 分色堆叠配平条 + 混合履约守恒等式（仓库/库存 + 供应商直发）+ 一键智能配平」；
 * - 上区（供给来源与数量分配）同屏平铺展示全部来源，支持直接行内改数量/选目标仓/改交期/比价换源，
 *   并通过按履约方式分类的比价面板精准添加新来源（现有库存 / 集采入仓 / 代发直发 / 集采直发）；
 * - 下区（交付地址分配工作区，含直发地址时同屏常驻，支持一键最大化专注 100+ 地址）实时联动，
 *   支持直发改派、按量跨源调拨及将选中地址原子转交仓库/库存来源。
 */
export function SourcingProductAdjustDialog({
    productId,
    initialFocusSection,
    onClose,
    onSetParticipation,
    onRestoreProductLines,
    navigationIds,
    onNavigate,
    returnFocusId,
    ...props
}: SourcingEditorProps &
    SourcingParticipationProps & {
        productId: string | null
        initialFocusSection?: "compare" | "addresses" | null
        onClose: () => void
        onRestoreProductLines?: (
            salesOrderLineId: string,
            snapshot: SourcingLineInput[],
        ) => void
        navigationIds?: readonly string[]
        onNavigate?: (id: string) => void
        returnFocusId?: string
    }) {
    const [showErrors, setShowErrors] = useState(false)
    const [selectedRowKey, setSelectedRowKey] = useState<string | null>(null)
    const [addPickerOpen, setAddPickerOpen] = useState(false)
    const [addressMaximized, setAddressMaximized] = useState(false)
    const [addressSourceFilter, setAddressSourceFilter] = useState<
        string | null
    >(null)
    const [actionNotice, setActionNotice] = useState<string | null>(null)
    const [snapshotState, setSnapshotState] = useState<{
        productId: string
        lines: SourcingLineInput[]
    } | null>(null)

    const lastProductId = useRef(productId)
    const sourcesScrollRef = useRef<HTMLDivElement>(null)
    if (productId) lastProductId.current = productId

    const rows = buildSourcingEditorRows(
        props.order,
        props.form.state.values.lines,
    )
    const row = rows.find((item) => item.product.salesOrderLineId === productId)
    const navigation = (navigationIds ?? []).flatMap((id) => {
        const found = rows.find((item) => item.product.salesOrderLineId === id)
        return found ? [found] : []
    })
    const position = navigation.findIndex((item) => item === row)
    const previous = navigation[position - 1]
    const next = navigation[position + 1]
    const pendingItems = [
        ...navigation.slice(position + 1),
        ...navigation.slice(0, Math.max(0, position)),
    ].filter((item) => item.needsAttention)
    const nextPending = pendingItems[0]

    const active =
        row?.allocations.find(({ line }) => line.rowKey === selectedRowKey) ??
        row?.allocations[0]
    const activeRowKey = active?.line.rowKey

    const hasDirectAddressWork = Boolean(
        row &&
        row.product.deliveryAllocations.length > 0 &&
        row.allocations.some(({ line }) =>
            usesAddressQuantity(row.product, line),
        ),
    )

    const addressState = useMemo(
        () =>
            row && hasDirectAddressWork
                ? computeGlobalAddressState(row.product, row.allocations)
                : null,
        [hasDirectAddressWork, row],
    )

    const usedBasisIdsAll = useMemo(
        () =>
            new Set(
                (row?.allocations ?? [])
                    .map(({ line }) => line.basisId)
                    .filter(Boolean),
            ),
        [row],
    )

    const unusedOptions = useMemo(
        () =>
            (row?.product.options ?? []).filter(
                (opt) => !usedBasisIdsAll.has(opt.basisId),
            ),
        [row, usedBasisIdsAll],
    )

    const recommendedBasisId = useMemo(
        () =>
            row
                ? pickBestSourcingOption(
                      row.product.options,
                      row.product.remainingQuantity,
                      row.product,
                  )?.basisId
                : undefined,
        [row],
    )

    const addDisabledReason = !row
        ? undefined
        : unusedOptions.length === 0
          ? "可用来源均已添加"
          : !canSplitSourcingProduct(row.product, row.allocations.length)
            ? "剩余需求不足以继续拆分，或商品单位不支持拆分"
            : undefined

    const fields =
        sourcingFormValidationError(props.order, props.form.state.values)
            ?.fields ?? {}
    const errorsByRowKey = new Map(
        (row?.allocations ?? []).map(({ line, index }) => [
            line.rowKey,
            showErrors
                ? [
                      ...new Set(
                          Object.entries(fields)
                              .filter(([path]) =>
                                  path.startsWith(`lines[${index}].`),
                              )
                              .map(([, message]) => message),
                      ),
                  ]
                : [],
        ]),
    )
    const allProductErrors = showErrors
        ? [
              ...new Set(
                  (row?.allocations ?? []).flatMap(
                      ({ line }) => errorsByRowKey.get(line.rowKey) ?? [],
                  ),
              ),
          ]
        : []

    // 打开或切换商品时，根据入口语义决定默认聚焦态并拍摄还原快照
    useEffect(() => {
        if (!productId) {
            setSnapshotState(null)
            setAddPickerOpen(false)
            setAddressMaximized(false)
            setAddressSourceFilter(null)
            setActionNotice(null)
            return
        }
        setAddPickerOpen(false)
        setAddressMaximized(initialFocusSection === "addresses")
        setAddressSourceFilter(null)
        setActionNotice(null)
        setSnapshotState((current) => {
            if (current?.productId === productId) return current
            return {
                productId,
                lines: cloneProductLines(
                    props.form.state.values.lines,
                    productId,
                ),
            }
        })
    }, [productId, initialFocusSection, props.form.state.values.lines])

    const currentProductLines = productId
        ? cloneProductLines(props.form.state.values.lines, productId)
        : []
    const isDirtyForProduct =
        Boolean(snapshotState && snapshotState.productId === productId) &&
        JSON.stringify(currentProductLines) !==
            JSON.stringify(snapshotState?.lines ?? [])

    const applyPatches = (
        patches: readonly LineUpdatePatch[],
        feedback?: string,
    ) => {
        for (const patch of patches) {
            props.form.setFieldValue(`lines[${patch.index}]`, patch.nextLine)
        }
        if (feedback) setActionNotice(feedback)
    }

    const close = () => {
        setShowErrors(false)
        setSelectedRowKey(null)
        setAddPickerOpen(false)
        setAddressMaximized(false)
        setAddressSourceFilter(null)
        setActionNotice(null)
        onClose()
    }

    const navigateFreely = (targetId: string) => {
        if (!onNavigate) return
        setShowErrors(false)
        setSelectedRowKey(null)
        setAddPickerOpen(false)
        setAddressMaximized(false)
        setAddressSourceFilter(null)
        setActionNotice(null)
        onNavigate(targetId)
    }

    const focusRowControl = (rowKey: string, invalid = false) => {
        requestAnimationFrame(() => {
            const seg = toAutomationIdSegment(rowKey)
            const target = invalid
                ? sourcesScrollRef.current?.querySelector<HTMLElement>(
                      '[aria-invalid="true"], [role="alert"]',
                  )
                : (document.getElementById(
                      `procurement-orders-create-row-${seg}-quantity`,
                  ) ??
                  document.getElementById(
                      `procurement-orders-create-row-${seg}-warehouse`,
                  ) ??
                  document.getElementById(
                      `procurement-orders-create-row-${seg}-sourcing-option`,
                  ))
            if (!target) return
            target.scrollIntoView({ block: "nearest", inline: "nearest" })
            target.focus({ preventScroll: true })
        })
    }

    const handleAddSourceByOption = (basisId: string) => {
        if (!row) return
        let addedKey: string | undefined
        if (props.onAddSplitWithOption) {
            addedKey = props.onAddSplitWithOption(
                row.product.salesOrderLineId,
                basisId,
            )
        } else {
            const previousKeys = new Set(
                row.allocations.map(({ line }) => line.rowKey),
            )
            props.onAddSplit(row.product.salesOrderLineId)
            addedKey = props.form.state.values.lines.find(
                (line) =>
                    line.salesOrderLineId === row.product.salesOrderLineId &&
                    !previousKeys.has(line.rowKey),
            )?.rowKey
        }
        setAddPickerOpen(false)
        setAddressMaximized(false)
        if (addedKey) {
            setSelectedRowKey(addedKey)
            focusRowControl(addedKey)
        }
    }

    const handleAddSourceClick = () => {
        if (!row || addDisabledReason) return
        setAddressMaximized(false)
        // 仅剩 1 个可选方案时一键直接添加；多个可选方案时展开按履约方式分类的比价面板让采购精准选定
        if (unusedOptions.length === 1 && props.onAddSplitWithOption) {
            handleAddSourceByOption(unusedOptions[0]!.basisId)
            return
        }
        if (props.onAddSplitWithOption && unusedOptions.length > 1) {
            setAddPickerOpen((prev) => !prev)
            requestAnimationFrame(() => {
                sourcesScrollRef.current?.scrollTo({
                    top: sourcesScrollRef.current.scrollHeight,
                    behavior: "smooth",
                })
            })
            return
        }
        const previousKeys = new Set(
            row.allocations.map(({ line }) => line.rowKey),
        )
        props.onAddSplit(row.product.salesOrderLineId)
        const added = props.form.state.values.lines.find(
            (line) =>
                line.salesOrderLineId === row.product.salesOrderLineId &&
                !previousKeys.has(line.rowKey),
        )
        if (added) {
            setSelectedRowKey(added.rowKey)
            focusRowControl(added.rowKey)
        }
    }

    const splitResidual = (moveAddressKeys?: readonly string[]) => {
        if (!row) return
        if (props.onAddResidualSplit) {
            const addedKey = props.onAddResidualSplit(
                row.product.salesOrderLineId,
                moveAddressKeys ? { moveAddressKeys } : undefined,
            )
            if (addedKey) {
                setAddPickerOpen(false)
                setSelectedRowKey(addedKey)
                if (!addressMaximized) focusRowControl(addedKey)
            }
            return
        }
        handleAddSourceClick()
    }

    const removeSource = (rowKey: string) => {
        if (!row || row.allocations.length <= 1) return
        const index = row.allocations.findIndex(
            ({ line }) => line.rowKey === rowKey,
        )
        if (index < 0) return
        const fallback =
            row.allocations[index + 1] ?? row.allocations[index - 1]
        const nextRowKey =
            rowKey === activeRowKey ? fallback?.line.rowKey : activeRowKey
        if (rowKey === activeRowKey) setSelectedRowKey(nextRowKey ?? null)
        if (addressSourceFilter === rowKey) setAddressSourceFilter(null)
        props.onRemoveSplit(rowKey)
        requestAnimationFrame(() => {
            if (!nextRowKey) return
            document
                .getElementById(
                    `sourcing-source-${toAutomationIdSegment(nextRowKey)}-edit`,
                )
                ?.focus({ preventScroll: true })
        })
    }

    const handleTrimDirectExcess = (
        onlyRowKey?: string,
        rawExcess?: string,
    ) => {
        if (!row || !addressState) return
        const excessQty = rawExcess ?? addressState.directOverTargetQty
        if (!isPositiveQuantity(excessQty)) return
        const res = buildTrimDirectExcessPatches(
            row.product,
            props.form.state.values.lines,
            excessQty,
            onlyRowKey,
        )
        applyPatches(
            res.patches,
            `已自动缩减直发 ${res.trimmedQty} ${row.product.unit}（释放 ${res.releasedAddressCount} 处地址），总量已重新配平。`,
        )
    }

    const handleFillDirectShortfall = (
        targetRowKey?: string,
        rawShortfall?: string,
    ) => {
        if (!row || !addressState) return
        const resolvedTarget =
            targetRowKey ??
            (activeRowKey &&
            addressState.directSources.some((s) => s.rowKey === activeRowKey)
                ? activeRowKey
                : addressState.primaryDirectSource?.rowKey)
        const shortfallQty = rawShortfall ?? addressState.directShortfallQty
        if (!resolvedTarget || !isPositiveQuantity(shortfallQty)) return
        const res = buildFillDirectShortfallPatches(
            row.product,
            props.form.state.values.lines,
            resolvedTarget,
            shortfallQty,
        )
        if (res.patches.length === 0) {
            setActionNotice(
                "当前暂无可继续直发分配的剩余地址（可能存在不配送地区），请在下方地址表核对。",
            )
            return
        }
        applyPatches(
            res.patches,
            `已为直发来源自动补配 ${res.addedQty} ${row.product.unit}（${res.addedAddressCount} 处可配送地址）。`,
        )
    }

    const finish = async () => {
        if (!row) return
        const prefixes = row.allocations.map(({ index }) => `lines[${index}].`)
        for (const [name, field] of Object.entries(props.form.fieldInfo)) {
            const instance = field?.instance
            if (
                !prefixes.some((prefix) => name.startsWith(prefix)) ||
                !instance ||
                instance.store.state.meta.isTouched
            )
                continue
            instance.setMeta((prev) => ({ ...prev, isTouched: true }))
        }
        await props.form.validate("change")
        const currentErrors =
            sourcingFormValidationError(props.order, props.form.state.values)
                ?.fields ?? {}
        const invalidEntries = row.allocations.filter(({ index }) =>
            Object.keys(currentErrors).some((path) =>
                path.startsWith(`lines[${index}].`),
            ),
        )
        if (invalidEntries.length > 0) {
            setShowErrors(true)
            const firstInvalid = invalidEntries[0]!
            const invalidPaths = Object.keys(currentErrors).filter((path) =>
                path.startsWith(`lines[${firstInvalid.index}].`),
            )
            const onlyAddressErrors =
                hasDirectAddressWork &&
                invalidPaths.every((path) =>
                    path.endsWith(".deliveryAllocations"),
                )
            setSelectedRowKey(firstInvalid.line.rowKey)
            if (!onlyAddressErrors) {
                setAddressMaximized(false)
                focusRowControl(firstInvalid.line.rowKey, true)
            }
            return
        }
        close()
    }

    const salesUnitPrice = row?.product.salesPrices?.unit_price_gross
    const included = Boolean(row && (row.selected || row.partiallySelected))
    const productSeg = row
        ? toAutomationIdSegment(row.product.salesOrderLineId)
        : ""

    return (
        <Dialog
            open={Boolean(row)}
            onOpenChange={(open) => {
                if (!open) close()
            }}
        >
            <DialogContent
                size="2xl"
                scroll="body"
                closeButtonId="sourcing-product-adjust-close"
                finalFocus={() =>
                    (returnFocusId
                        ? document.getElementById(returnFocusId)
                        : null) ??
                    (lastProductId.current
                        ? document.getElementById(
                              `sourcing-list-${toAutomationIdSegment(lastProductId.current)}-edit`,
                          )
                        : null) ??
                    document.getElementById("sourcing-products-search")
                }
            >
                {row ? (
                    <>
                        {/* 页头：商品身份 + 本次下单开关 */}
                        <DialogHeader className="border-border/60 bg-card py-2.5">
                            <div className="flex flex-wrap items-center justify-between gap-3">
                                <div className="min-w-0 flex-1 space-y-0.5">
                                    <div className="flex flex-wrap items-center gap-2">
                                        <DialogTitle className="truncate">
                                            供给分配 · {row.product.itemName}
                                        </DialogTitle>
                                        <StatusBadge
                                            label={row.status}
                                            tone={statusTone(row.status)}
                                        />
                                    </div>
                                    <DetailWorkspaceMeta
                                        items={[
                                            row.product.itemSku ? (
                                                <span key="sku" className="num">
                                                    {row.product.itemSku}
                                                </span>
                                            ) : null,
                                            salesUnitPrice ? (
                                                <span key="sales-price">
                                                    含税售价{" "}
                                                    <MoneyValue
                                                        display="exact"
                                                        value={salesUnitPrice}
                                                        className="font-medium [&_[data-slot=money-value-number]]:text-foreground"
                                                    />
                                                </span>
                                            ) : null,
                                            row.product.deliveryDeadline ? (
                                                <span
                                                    key="deadline"
                                                    className="num"
                                                >
                                                    最晚交付{" "}
                                                    <span className="font-medium text-foreground">
                                                        {
                                                            row.product
                                                                .deliveryDeadline
                                                        }
                                                    </span>
                                                </span>
                                            ) : null,
                                        ]}
                                    />
                                </div>

                                <div className="flex shrink-0 items-center gap-2">
                                    <SourcingParticipationControl
                                        idPrefix={`sourcing-product-adjust-${productSeg}`}
                                        itemName={row.product.itemName}
                                        included={included}
                                        onChange={(nextIncluded) =>
                                            onSetParticipation(
                                                [row.product.salesOrderLineId],
                                                nextIncluded,
                                            )
                                        }
                                        showHint={false}
                                        variant="pill"
                                    />
                                </div>
                            </div>
                            <DialogDescription className="sr-only">
                                同屏查看和分配多个供给来源（现有库存、集采入仓、代发直发、集采直发）的数量、目标仓、交期与收货地址。修改实时存入草稿。
                            </DialogDescription>
                        </DialogHeader>

                        {/* 全局多源配平进度条：分色堆叠 + 混合履约守恒等式 + 一键智能配平 */}
                        <SourcingAllocationSummary
                            row={row}
                            addressState={addressState}
                            canSplitNewSource={!addDisabledReason}
                            onTrimDirectExcess={() => handleTrimDirectExcess()}
                            onFillDirectShortfall={() =>
                                handleFillDirectShortfall()
                            }
                            onSplitResidual={
                                !addDisabledReason
                                    ? () => splitResidual()
                                    : undefined
                            }
                        />

                        {/* 主工作面：上区（全部来源同屏平铺编辑）+ 下区（地址矩阵实时联动） */}
                        <DialogBody className="flex h-[min(78dvh,50rem)] min-h-0 flex-auto flex-col overflow-hidden p-0">
                            {/* 全局校验错误条（始终可见） */}
                            {allProductErrors.length > 0 ? (
                                <div
                                    id="sourcing-product-adjust-source-errors"
                                    role="alert"
                                    tabIndex={-1}
                                    className="flex shrink-0 items-start gap-2 border-b border-destructive-border bg-destructive-soft px-4 py-2 text-xs text-destructive-soft-foreground outline-none sm:px-6"
                                >
                                    <TriangleAlertIcon
                                        aria-hidden="true"
                                        className="mt-0.5 size-3.5 shrink-0"
                                    />
                                    <div className="space-y-0.5">
                                        {allProductErrors.map((message) => (
                                            <p key={message}>{message}</p>
                                        ))}
                                    </div>
                                </div>
                            ) : null}

                            {/* 暂缓下单提示 */}
                            {!included ? (
                                <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border/60 bg-surface-sunken px-4 py-2 text-xs text-muted-foreground sm:px-6">
                                    <span className="flex items-center gap-2">
                                        <InfoIcon
                                            aria-hidden="true"
                                            className="size-3.5 shrink-0"
                                        />
                                        <span>
                                            当前已暂缓下单：本次提交不会包含此商品，所配方案保存在草稿中。
                                        </span>
                                    </span>
                                    <Button
                                        id={`sourcing-editor-${productSeg}-enable-participation`}
                                        type="button"
                                        variant="outline"
                                        size="xs"
                                        className="bg-card"
                                        onClick={() =>
                                            onSetParticipation(
                                                [row.product.salesOrderLineId],
                                                true,
                                            )
                                        }
                                    >
                                        恢复本次下单
                                    </Button>
                                </div>
                            ) : null}

                            {/* 操作反馈轻量条 */}
                            {actionNotice ? (
                                <div
                                    role="status"
                                    className="flex shrink-0 items-center justify-between gap-2 border-b border-info-border/70 bg-info-soft/50 px-4 py-1.5 text-xs text-info-soft-foreground sm:px-6"
                                >
                                    <span>{actionNotice}</span>
                                    <button
                                        id={`sourcing-product-adjust-${productSeg}-notice-dismiss`}
                                        type="button"
                                        aria-label="关闭提示"
                                        className="cursor-pointer hover:text-foreground"
                                        onClick={() => setActionNotice(null)}
                                    >
                                        <XIcon className="size-3.5" />
                                    </button>
                                </div>
                            ) : null}

                            {/* 上区：供给来源与配量（专注地址模式时收拢为单行实时概览条，否则全部平铺同屏编辑） */}
                            <section
                                aria-label="供给来源列表"
                                className={cn(
                                    "flex min-h-0 flex-col bg-card",
                                    addressMaximized && hasDirectAddressWork
                                        ? "shrink-0 border-b border-border/70"
                                        : hasDirectAddressWork
                                          ? addPickerOpen
                                              ? "max-h-[64%] shrink-0 border-b-2 border-border/80"
                                              : "max-h-[48%] shrink-0 border-b-2 border-border/80"
                                          : "flex-1",
                                )}
                            >
                                {addressMaximized && hasDirectAddressWork ? (
                                    <div className="flex flex-wrap items-center justify-between gap-2 bg-surface-sunken/60 px-4 py-2 sm:px-6">
                                        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1.5">
                                            <span className="text-xs font-medium text-muted-foreground">
                                                供给来源：
                                            </span>
                                            {row.allocations.map(
                                                ({ line }, idx) => {
                                                    const opt =
                                                        findSourcingOption(
                                                            row.product,
                                                            line.basisId,
                                                        )
                                                    const isStock =
                                                        opt?.sourceType ===
                                                        "EXISTING_STOCK"
                                                    const label = isStock
                                                        ? opt.warehouseName ||
                                                          opt.supplierName
                                                        : opt?.supplierName ||
                                                          "未选供应商"
                                                    const route = opt
                                                        ? sourcingRouteLabel(
                                                              opt,
                                                          )
                                                        : "待选"
                                                    const accent =
                                                        sourceAccentForIndex(
                                                            idx,
                                                        )
                                                    const hasErr = Boolean(
                                                        errorsByRowKey.get(
                                                            line.rowKey,
                                                        )?.length,
                                                    )
                                                    return (
                                                        <button
                                                            key={line.rowKey}
                                                            id={`sourcing-source-${toAutomationIdSegment(line.rowKey)}-edit`}
                                                            type="button"
                                                            aria-label={`编辑 ${label} · ${route}，分配数量 ${line.quantity || "待填写"}${row.product.unit}${hasErr ? "，待调整" : ""}`}
                                                            onClick={() => {
                                                                setSelectedRowKey(
                                                                    line.rowKey,
                                                                )
                                                                setAddressMaximized(
                                                                    false,
                                                                )
                                                            }}
                                                            className={cn(
                                                                "inline-flex max-w-64 cursor-pointer items-center gap-1.5 rounded-md border px-2 py-1 text-xs transition-colors outline-none hover:bg-card focus-visible:ring-2 focus-visible:ring-ring",
                                                                hasErr
                                                                    ? "border-destructive/50 bg-destructive-soft/40 text-destructive-soft-foreground"
                                                                    : "border-border/70 bg-card text-foreground",
                                                                !line.selected &&
                                                                    "border-dashed opacity-65",
                                                            )}
                                                        >
                                                            <span
                                                                aria-hidden="true"
                                                                className={cn(
                                                                    "size-2 shrink-0 rounded-full",
                                                                    accent.barClass,
                                                                )}
                                                            />
                                                            <span className="num font-semibold">
                                                                源{idx + 1}
                                                            </span>
                                                            <Badge
                                                                variant={sourceRouteBadgeVariant(
                                                                    opt,
                                                                )}
                                                                className="h-4.5 px-1 text-tiny"
                                                            >
                                                                {route}
                                                            </Badge>
                                                            <span className="truncate">
                                                                {label}
                                                            </span>
                                                            <span className="num font-semibold">
                                                                {line.quantity ||
                                                                    "0"}{" "}
                                                                {
                                                                    row.product
                                                                        .unit
                                                                }
                                                            </span>
                                                        </button>
                                                    )
                                                },
                                            )}
                                        </div>
                                        <div className="flex shrink-0 items-center gap-1.5">
                                            <Button
                                                id={`sourcing-product-${productSeg}-split`}
                                                type="button"
                                                variant="outline"
                                                size="xs"
                                                className="bg-card"
                                                disabled={Boolean(
                                                    addDisabledReason,
                                                )}
                                                title={
                                                    addDisabledReason ??
                                                    `共 ${row.product.options.length} 个可用方案`
                                                }
                                                onClick={handleAddSourceClick}
                                            >
                                                <PlusIcon aria-hidden="true" />
                                                添加来源
                                            </Button>
                                            <Button
                                                id={`sourcing-product-${productSeg}-expand-sources`}
                                                type="button"
                                                variant="ghost"
                                                size="xs"
                                                onClick={() =>
                                                    setAddressMaximized(false)
                                                }
                                            >
                                                展开编辑来源
                                            </Button>
                                        </div>
                                    </div>
                                ) : (
                                    <>
                                        {/* 来源区小标题栏 */}
                                        <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border/60 bg-sidebar/60 px-4 py-2 sm:px-6">
                                            <div className="flex flex-wrap items-center gap-2">
                                                <h3 className="flex items-center gap-1.5 text-body-compact font-semibold text-foreground">
                                                    <LayersIcon
                                                        aria-hidden="true"
                                                        className="size-3.5 text-primary"
                                                    />
                                                    供货来源与数量分配
                                                </h3>
                                                <span className="num rounded-sm bg-muted px-1.5 py-0.5 text-tiny font-medium text-muted-foreground">
                                                    {row.allocations.length > 1
                                                        ? `已拆 ${row.allocations.length} 源`
                                                        : `共 ${row.product.options.length} 个可选方案`}
                                                </span>
                                                {row.allocations.length > 1 ? (
                                                    <span className="hidden text-xs text-muted-foreground lg:inline">
                                                        支持现有库存、集采入仓、代发/集采直发同屏混配，实时守恒
                                                    </span>
                                                ) : null}
                                            </div>
                                            <div className="flex shrink-0 items-center gap-1.5">
                                                <Button
                                                    id={`sourcing-product-${productSeg}-split`}
                                                    type="button"
                                                    variant={
                                                        addPickerOpen
                                                            ? "secondary"
                                                            : "outline"
                                                    }
                                                    size="xs"
                                                    className={
                                                        !addPickerOpen
                                                            ? "bg-card"
                                                            : undefined
                                                    }
                                                    disabled={Boolean(
                                                        addDisabledReason,
                                                    )}
                                                    aria-expanded={
                                                        addPickerOpen
                                                    }
                                                    title={
                                                        addDisabledReason ??
                                                        `还可添加 ${unusedOptions.length} 个供给方案`
                                                    }
                                                    onClick={
                                                        handleAddSourceClick
                                                    }
                                                >
                                                    <PlusIcon aria-hidden="true" />
                                                    {addPickerOpen
                                                        ? "收起候选方案"
                                                        : row.allocations
                                                                .length === 1
                                                          ? "拆单添加来源"
                                                          : `添加来源（余 ${unusedOptions.length}）`}
                                                </Button>
                                            </div>
                                        </div>

                                        {/* 来源行平铺滚动区 */}
                                        <div
                                            id="sourcing-product-adjust-body"
                                            ref={sourcesScrollRef}
                                            className="@container/sourcing min-h-0 flex-1 space-y-2.5 overflow-y-auto overscroll-contain px-4 py-3 sm:px-6"
                                        >
                                            {row.allocations.map(
                                                ({ line, index }, idx) => (
                                                    <SourcingSourceRow
                                                        key={line.rowKey}
                                                        form={props.form}
                                                        order={props.order}
                                                        product={row.product}
                                                        row={row}
                                                        line={line}
                                                        index={index}
                                                        sourceNumber={idx + 1}
                                                        isActive={
                                                            line.rowKey ===
                                                            activeRowKey
                                                        }
                                                        isMultiSource={
                                                            row.allocations
                                                                .length > 1
                                                        }
                                                        canRemove={
                                                            row.allocations
                                                                .length > 1
                                                        }
                                                        errors={
                                                            errorsByRowKey.get(
                                                                line.rowKey,
                                                            ) ?? []
                                                        }
                                                        addressState={
                                                            addressState
                                                        }
                                                        isAddressFiltered={
                                                            addressSourceFilter ===
                                                            line.rowKey
                                                        }
                                                        defaultCompareOpen={
                                                            initialFocusSection ===
                                                                "compare" &&
                                                            idx === 0
                                                        }
                                                        onActivate={() =>
                                                            setSelectedRowKey(
                                                                line.rowKey,
                                                            )
                                                        }
                                                        onRemove={() =>
                                                            removeSource(
                                                                line.rowKey,
                                                            )
                                                        }
                                                        onToggleAddressFilter={
                                                            hasDirectAddressWork
                                                                ? () =>
                                                                      setAddressSourceFilter(
                                                                          (
                                                                              curr,
                                                                          ) =>
                                                                              curr ===
                                                                              line.rowKey
                                                                                  ? null
                                                                                  : line.rowKey,
                                                                      )
                                                                : undefined
                                                        }
                                                        onTrimDirectExcessForSource={(
                                                            rk,
                                                            excessQty,
                                                        ) =>
                                                            handleTrimDirectExcess(
                                                                rk,
                                                                excessQty,
                                                            )
                                                        }
                                                        onFillDirectShortfallForSource={(
                                                            rk,
                                                            shortfallQty,
                                                        ) =>
                                                            handleFillDirectShortfall(
                                                                rk,
                                                                shortfallQty,
                                                            )
                                                        }
                                                        onApplyPatches={
                                                            applyPatches
                                                        }
                                                    />
                                                ),
                                            )}

                                            {/* 按履约方式分类的新增来源比价面板 */}
                                            {addPickerOpen ? (
                                                <div className="space-y-2 rounded-lg border border-primary/35 bg-info-soft/20 p-3">
                                                    <div className="flex flex-wrap items-center justify-between gap-2">
                                                        <div>
                                                            <p className="text-body-compact font-semibold text-foreground">
                                                                选择要添加的新来源（按履约方式筛选）
                                                            </p>
                                                            <p className="text-xs text-muted-foreground">
                                                                点击任意未添加方案即可加入拆单；若有未覆盖缺口将自动带入余量。
                                                            </p>
                                                        </div>
                                                        <Button
                                                            id={`sourcing-product-${productSeg}-add-picker-close`}
                                                            type="button"
                                                            variant="ghost"
                                                            size="xs"
                                                            onClick={() =>
                                                                setAddPickerOpen(
                                                                    false,
                                                                )
                                                            }
                                                        >
                                                            <XIcon aria-hidden="true" />
                                                            关闭
                                                        </Button>
                                                    </div>
                                                    <SourcingOptionCompareMatrix
                                                        idPrefix={`sourcing-add-${productSeg}`}
                                                        order={props.order}
                                                        product={row.product}
                                                        selectedBasisId=""
                                                        usedBasisIds={
                                                            usedBasisIdsAll
                                                        }
                                                        recommendedBasisId={
                                                            recommendedBasisId
                                                        }
                                                        mode="add"
                                                        onSelect={(option) =>
                                                            handleAddSourceByOption(
                                                                option.basisId,
                                                            )
                                                        }
                                                    />
                                                </div>
                                            ) : null}

                                            {/* 纯仓发/库存且存在客户地址时，底部折叠展示只读收货地址要求 */}
                                            {!hasDirectAddressWork &&
                                            row.product.deliveryAllocations
                                                .length > 0 ? (
                                                <Collapsible className="rounded-lg border border-border/70 bg-surface-sunken/50">
                                                    <div className="flex flex-wrap items-center justify-between gap-2 px-3.5 py-2">
                                                        <span className="flex items-center gap-2 text-xs text-muted-foreground">
                                                            <MapPinIcon
                                                                aria-hidden="true"
                                                                className="size-3.5 shrink-0 text-muted-foreground"
                                                            />
                                                            <span>
                                                                客户收货地址共{" "}
                                                                <strong className="num font-medium text-foreground">
                                                                    {
                                                                        row
                                                                            .product
                                                                            .deliveryAllocations
                                                                            .length
                                                                    }
                                                                </strong>{" "}
                                                                处（
                                                                <QuantityValue
                                                                    value={
                                                                        row
                                                                            .product
                                                                            .remainingQuantity
                                                                    }
                                                                    unit={
                                                                        row
                                                                            .product
                                                                            .unit
                                                                    }
                                                                    className="font-medium text-foreground"
                                                                />
                                                                ）·
                                                                当前由仓库/库存出库时统一安排，如需部分直发请添加代发/集采直发来源。
                                                            </span>
                                                        </span>
                                                        <CollapsibleTrigger
                                                            id={`sourcing-product-${productSeg}-readonly-addr-toggle`}
                                                            render={
                                                                <Button
                                                                    type="button"
                                                                    variant="ghost"
                                                                    size="xs"
                                                                    className="group"
                                                                />
                                                            }
                                                        >
                                                            查看收货地址
                                                            <ChevronDownIcon className="size-3.5 transition-transform group-data-[panel-open]:rotate-180" />
                                                        </CollapsibleTrigger>
                                                    </div>
                                                    <CollapsibleContent>
                                                        <div className="border-t border-border/60 bg-card">
                                                            <SourcingDeliveryRequirementSection
                                                                product={
                                                                    row.product
                                                                }
                                                                condensed
                                                            />
                                                        </div>
                                                    </CollapsibleContent>
                                                </Collapsible>
                                            ) : null}
                                        </div>
                                    </>
                                )}
                            </section>

                            {/* 下区：交付地址分配工作区（含直发地址时同屏实时联动） */}
                            {hasDirectAddressWork ? (
                                <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
                                    <SourcingAddressEditor
                                        form={props.form}
                                        row={row}
                                        canSplitNewSource={!addDisabledReason}
                                        sourceFilter={addressSourceFilter}
                                        onSourceFilterChange={
                                            setAddressSourceFilter
                                        }
                                        isMaximized={addressMaximized}
                                        onToggleMaximize={() =>
                                            setAddressMaximized((prev) => !prev)
                                        }
                                        onSplitResidual={
                                            !addDisabledReason
                                                ? () => splitResidual()
                                                : undefined
                                        }
                                        onSplitAddressesToNewSource={
                                            !addDisabledReason
                                                ? (keys) => splitResidual(keys)
                                                : undefined
                                        }
                                    />
                                </div>
                            ) : null}
                        </DialogBody>

                        {/* 精简底栏 */}
                        <DialogFooter className="items-center border-border/60 bg-card sm:justify-between">
                            <div className="flex flex-wrap items-center gap-2.5">
                                {onRestoreProductLines &&
                                snapshotState &&
                                isDirtyForProduct ? (
                                    <Button
                                        id="sourcing-product-adjust-restore"
                                        type="button"
                                        variant="ghost"
                                        size="sm"
                                        onClick={() => {
                                            onRestoreProductLines(
                                                snapshotState.productId,
                                                snapshotState.lines,
                                            )
                                            setShowErrors(false)
                                            setActionNotice(null)
                                            setSelectedRowKey(
                                                snapshotState.lines[0]
                                                    ?.rowKey ?? null,
                                            )
                                        }}
                                    >
                                        <RotateCcwIcon aria-hidden="true" />
                                        撤销此商品修改
                                    </Button>
                                ) : null}
                                {showErrors && row.issues.length ? (
                                    <StatusBadge
                                        tone="destructive"
                                        label={`还有 ${row.issues.length} 项需调整`}
                                    />
                                ) : (
                                    <p className="text-xs text-muted-foreground">
                                        修改自动存入草稿，整单预览后统一提交
                                    </p>
                                )}
                            </div>

                            <div className="flex flex-wrap items-center gap-2">
                                {onNavigate && navigation.length > 1 ? (
                                    <>
                                        <div className="inline-flex h-control items-center overflow-hidden rounded-md border border-border bg-card">
                                            <Button
                                                id="sourcing-product-adjust-previous"
                                                type="button"
                                                variant="ghost"
                                                size="sm"
                                                className="h-full rounded-none px-2.5 text-xs"
                                                disabled={!previous}
                                                onClick={() =>
                                                    previous &&
                                                    navigateFreely(
                                                        previous.product
                                                            .salesOrderLineId,
                                                    )
                                                }
                                            >
                                                <ArrowLeftIcon />
                                                上一商品
                                            </Button>
                                            <span className="num inline-flex h-full items-center border-x border-border/60 bg-surface-sunken px-2.5 text-xs font-medium text-muted-foreground">
                                                {position + 1} /{" "}
                                                {navigation.length}
                                            </span>
                                            <Button
                                                id="sourcing-product-adjust-next"
                                                type="button"
                                                variant="ghost"
                                                size="sm"
                                                className="h-full rounded-none px-2.5 text-xs"
                                                disabled={!next}
                                                onClick={() =>
                                                    next &&
                                                    navigateFreely(
                                                        next.product
                                                            .salesOrderLineId,
                                                    )
                                                }
                                            >
                                                下一商品
                                                <ArrowRightIcon />
                                            </Button>
                                        </div>
                                        {pendingItems.length > 0 &&
                                        nextPending ? (
                                            <Button
                                                id="sourcing-product-adjust-next-pending"
                                                type="button"
                                                variant="outline"
                                                onClick={() =>
                                                    navigateFreely(
                                                        nextPending.product
                                                            .salesOrderLineId,
                                                    )
                                                }
                                            >
                                                下一待调整
                                                <span className="num rounded-sm bg-warning-soft px-1.5 py-0 text-tiny font-medium text-warning-soft-foreground">
                                                    {pendingItems.length}
                                                </span>
                                            </Button>
                                        ) : null}
                                        <span
                                            aria-hidden="true"
                                            className="mx-0.5 hidden h-4 w-px bg-border sm:block"
                                        />
                                    </>
                                ) : null}
                                <Button
                                    id="sourcing-product-adjust-done"
                                    type="button"
                                    className="px-4"
                                    onClick={() => void finish()}
                                >
                                    完成调整
                                </Button>
                            </div>
                        </DialogFooter>
                    </>
                ) : null}
            </DialogContent>
        </Dialog>
    )
}
