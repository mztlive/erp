"use client"

import { useEffect, useMemo, useRef, useState } from "react"
import {
    ArrowLeftIcon,
    ArrowRightIcon,
    CheckCircle2Icon,
    InfoIcon,
    MapPinIcon,
    PlusIcon,
    RotateCcwIcon,
    ScaleIcon,
    TriangleAlertIcon,
    XIcon,
} from "lucide-react"
import { DetailWorkspaceMeta, MoneyValue } from "@/components/business"
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
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { pickBestSourcingOption } from "../lib/purchase-order-create-model"
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
import { buildSourcingEditorRows } from "../lib/sourcing/editor-rows"
import {
    applySourcingOptionToLine,
    summarizeRowPricing,
} from "../lib/sourcing/pricing"
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

type RightPaneTab = "addresses" | "compare" | "readonly_addresses"
type CompareIntent = "replace" | "add"

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
 * 全宽高左右双栏供给分配工作面（Full-Viewport Split Sheet）：
 * - 左栏常驻「配平总览卡 + 全部已选来源卡片」，现有库存 / 集采入仓 / 代发直发 / 集采直发同屏编辑、实时守恒；
 * - 右栏独享整屏高度，在「交付地址分配」与「候选方案比价（支持换源 & 一键拆单加入）」间零损耗切换，
 *   彻底告别上下挤压与多层盒中套盒。
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
    const [rightTab, setRightTab] = useState<RightPaneTab>("addresses")
    const [compareIntent, setCompareIntent] = useState<CompareIntent>("replace")
    const [addressSourceFilter, setAddressSourceFilter] = useState<
        string | null
    >(null)
    const [actionNotice, setActionNotice] = useState<string | null>(null)
    const [lastOpenProductId, setLastOpenProductId] = useState<string | null>(
        productId,
    )
    const [snapshotState, setSnapshotState] = useState<{
        productId: string
        lines: SourcingLineInput[]
    } | null>(null)

    if (productId && lastOpenProductId !== productId) {
        setLastOpenProductId(productId)
    }

    const lastProductId = useRef(productId)
    const sourcesScrollRef = useRef<HTMLDivElement>(null)
    if (productId) lastProductId.current = productId

    const rows = buildSourcingEditorRows(
        props.order,
        props.form.state.values.lines,
    )
    const liveRow = rows.find(
        (item) => item.product.salesOrderLineId === productId,
    )
    // 关闭收起动画期间保留上一次打开的商品内容，避免内容瞬间清空导致滑出动画白屏
    const row =
        liveRow ??
        (lastOpenProductId
            ? rows.find(
                  (item) => item.product.salesOrderLineId === lastOpenProductId,
              )
            : undefined)
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
    const activeSourceNumber =
        (row?.allocations.findIndex(
            ({ line }) => line.rowKey === activeRowKey,
        ) ?? 0) + 1

    const hasDirectAddressWork = Boolean(
        row &&
        row.product.deliveryAllocations.length > 0 &&
        row.allocations.some(({ line }) =>
            usesAddressQuantity(row.product, line),
        ),
    )
    const hasReadonlyAddresses = Boolean(
        row &&
        !hasDirectAddressWork &&
        row.product.deliveryAllocations.length > 0,
    )

    const resolvedRightTab: RightPaneTab = (() => {
        if (rightTab === "addresses" && !hasDirectAddressWork) {
            return "compare"
        }
        if (rightTab === "readonly_addresses" && !hasReadonlyAddresses) {
            return hasDirectAddressWork ? "addresses" : "compare"
        }
        return rightTab
    })()

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

    const usedBasisIdsOtherThanActive = useMemo(
        () =>
            new Set(
                (row?.allocations ?? [])
                    .filter(
                        ({ line }) =>
                            line.rowKey !== activeRowKey &&
                            line.selected &&
                            line.basisId,
                    )
                    .map(({ line }) => line.basisId),
            ),
        [activeRowKey, row],
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

    const pricingSummary = useMemo(
        () => (row ? summarizeRowPricing(row) : null),
        [row],
    )

    const headerPricingText = (() => {
        if (!row || !pricingSummary) return ""
        const count = row.allocations.filter(({ line }) => line.selected).length
        const base =
            row.allocations.length > 1
                ? `${count || row.allocations.length} 家组合`
                : "单源供货"
        if (pricingSummary.kind === "stock") return `${base} · 库存核算`
        if (
            pricingSummary.kind === "purchase" &&
            pricingSummary.unitCostGross
        ) {
            const marginText = pricingSummary.hasInvertedSource
                ? "价格倒挂"
                : pricingSummary.margin?.marginRateText
                  ? `毛利 ${pricingSummary.margin.marginRateText}`
                  : ""
            return [base, `¥${pricingSummary.unitCostGross}`, marginText]
                .filter(Boolean)
                .join(" · ")
        }
        if (pricingSummary.kind === "multiple") {
            return pricingSummary.hasInvertedSource
                ? `${base} · 多价格 · 含倒挂`
                : `${base} · 多价格`
        }
        return `${base} · 待选方案`
    })()

    const addressTabBadge = (() => {
        if (!addressState || !row) return null
        if (addressState.counts.issues > 0) {
            return {
                tone: "destructive" as const,
                text: `${addressState.counts.issues} 处异常`,
            }
        }
        if (isPositiveQuantity(addressState.directOverTargetQty)) {
            return {
                tone: "destructive" as const,
                text: `超出 ${addressState.directOverTargetQty} ${row.product.unit}`,
            }
        }
        if (isPositiveQuantity(addressState.directShortfallQty)) {
            return {
                tone: "warning" as const,
                text: addressState.hasNonDirectSources
                    ? `尚缺 ${addressState.directShortfallQty} ${row.product.unit}`
                    : `待分 ${addressState.counts.unassigned} 处`,
            }
        }
        if (addressState.hasNonDirectSources) {
            return {
                tone: "success" as const,
                text: `直发已齐 ${addressState.directAssignedTotalQty}/${addressState.directTargetQty}`,
            }
        }
        return {
            tone: "success" as const,
            text: `已全分 ${addressState.counts.total} 处`,
        }
    })()

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

    // 打开或切换商品时，根据入口语义设定右栏默认页签并拍摄还原快照
    useEffect(() => {
        if (!productId) {
            setSnapshotState(null)
            setRightTab("addresses")
            setCompareIntent("replace")
            setAddressSourceFilter(null)
            setActionNotice(null)
            return
        }
        setCompareIntent("replace")
        setRightTab(initialFocusSection === "compare" ? "compare" : "addresses")
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
        setRightTab("addresses")
        setCompareIntent("replace")
        setAddressSourceFilter(null)
        setActionNotice(null)
        onClose()
    }

    const navigateFreely = (targetId: string) => {
        if (!onNavigate) return
        setShowErrors(false)
        setSelectedRowKey(null)
        setRightTab("addresses")
        setCompareIntent("replace")
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
        if (addedKey) {
            setSelectedRowKey(addedKey)
            focusRowControl(addedKey)
        }
    }

    const handleAddSourceClick = () => {
        if (!row || addDisabledReason) return
        if (unusedOptions.length === 1 && props.onAddSplitWithOption) {
            handleAddSourceByOption(unusedOptions[0]!.basisId)
            return
        }
        if (props.onAddSplitWithOption && unusedOptions.length > 1) {
            setCompareIntent("add")
            setRightTab("compare")
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
                setSelectedRowKey(addedKey)
                focusRowControl(addedKey)
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
            `已自动缩减直发 ${res.trimmedQty} ${row.product.unit}（释放 ${res.releasedAddressCount} 处地址），总量已配平。`,
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
                "当前暂无可继续直发分配的剩余地址（可能存在不配送地区），请在右侧地址表核对。",
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
            if (onlyAddressErrors) {
                setRightTab("addresses")
            } else {
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
            open={Boolean(liveRow)}
            onOpenChange={(open) => {
                if (!open) close()
            }}
        >
            <DialogContent
                size="full"
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
                        {/* 全宽顶栏：商品身份 + 综合价格摘要 + 本次下单开关 */}
                        <DialogHeader className="border-border/70 bg-card py-2.5">
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

                                <div className="flex shrink-0 items-center gap-3">
                                    {headerPricingText ? (
                                        <span
                                            className={cn(
                                                "hidden text-xs md:inline",
                                                pricingSummary?.hasInvertedSource
                                                    ? "font-medium text-destructive"
                                                    : "text-muted-foreground",
                                            )}
                                        >
                                            {headerPricingText}
                                        </span>
                                    ) : null}
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
                                左侧管理全部供给来源与数量配平，右侧全屏处理交付地址分配与候选方案比价。修改实时存入草稿。
                            </DialogDescription>
                        </DialogHeader>

                        {/* 全局校验错误条与暂缓提示条 */}
                        {allProductErrors.length > 0 ? (
                            <div
                                id="sourcing-product-adjust-source-errors"
                                role="alert"
                                tabIndex={-1}
                                className="flex shrink-0 items-start gap-2 border-b border-destructive-border bg-destructive-soft px-5 py-2 text-xs text-destructive-soft-foreground outline-none"
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

                        {!included ? (
                            <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border/60 bg-surface-sunken px-5 py-2 text-xs text-muted-foreground">
                                <span className="flex items-center gap-2">
                                    <InfoIcon
                                        aria-hidden="true"
                                        className="size-3.5 shrink-0"
                                    />
                                    <span>
                                        当前已暂缓下单：本次提交不会包含此商品，所配方案保留在草稿中。
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

                        {actionNotice ? (
                            <div
                                role="status"
                                className="flex shrink-0 items-center justify-between gap-2 border-b border-info-border/70 bg-info-soft/50 px-5 py-1.5 text-xs text-info-soft-foreground"
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

                        {/* 主工作面：全高左右双栏（左栏供给与配量，右栏地址矩阵 / 比价选源） */}
                        <DialogBody className="flex min-h-0 flex-1 flex-col overflow-y-auto p-0 lg:flex-row lg:overflow-hidden">
                            {/* 左栏：供给方案与配量（恒定可见，宽 430–490px） */}
                            <aside className="flex w-full shrink-0 flex-col border-b border-border/70 bg-sidebar lg:h-full lg:w-[27rem] lg:overflow-hidden lg:border-r lg:border-b-0 xl:w-[30.5rem]">
                                <div className="shrink-0 p-4 pb-2.5">
                                    <SourcingAllocationSummary
                                        row={row}
                                        addressState={addressState}
                                        canSplitNewSource={!addDisabledReason}
                                        onTrimDirectExcess={() =>
                                            handleTrimDirectExcess()
                                        }
                                        onFillDirectShortfall={() =>
                                            handleFillDirectShortfall()
                                        }
                                        onSplitResidual={
                                            !addDisabledReason
                                                ? () => splitResidual()
                                                : undefined
                                        }
                                    />
                                </div>

                                <section
                                    aria-label="供给来源列表"
                                    className="flex min-h-0 flex-1 flex-col"
                                >
                                    <div className="flex shrink-0 items-center justify-between gap-2 px-4 py-1.5">
                                        <h3 className="text-xs font-semibold text-muted-foreground">
                                            供给来源（{row.allocations.length}）
                                        </h3>
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
                                                `还可添加 ${unusedOptions.length} 个供给方案`
                                            }
                                            onClick={handleAddSourceClick}
                                        >
                                            <PlusIcon aria-hidden="true" />
                                            添加来源
                                        </Button>
                                    </div>

                                    <div
                                        id="sourcing-product-adjust-body"
                                        ref={sourcesScrollRef}
                                        className="min-h-0 flex-1 space-y-2.5 overflow-y-auto overscroll-contain px-4 pt-1 pb-4"
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
                                                        row.allocations.length >
                                                        1
                                                    }
                                                    canRemove={
                                                        row.allocations.length >
                                                        1
                                                    }
                                                    errors={
                                                        errorsByRowKey.get(
                                                            line.rowKey,
                                                        ) ?? []
                                                    }
                                                    addressState={addressState}
                                                    isAddressFiltered={
                                                        addressSourceFilter ===
                                                        line.rowKey
                                                    }
                                                    isCompareTarget={
                                                        resolvedRightTab ===
                                                            "compare" &&
                                                        compareIntent ===
                                                            "replace" &&
                                                        line.rowKey ===
                                                            activeRowKey
                                                    }
                                                    onActivate={() =>
                                                        setSelectedRowKey(
                                                            line.rowKey,
                                                        )
                                                    }
                                                    onOpenCompare={() => {
                                                        setSelectedRowKey(
                                                            line.rowKey,
                                                        )
                                                        setCompareIntent(
                                                            "replace",
                                                        )
                                                        setRightTab("compare")
                                                    }}
                                                    onRemove={() =>
                                                        removeSource(
                                                            line.rowKey,
                                                        )
                                                    }
                                                    onToggleAddressFilter={
                                                        hasDirectAddressWork
                                                            ? () => {
                                                                  setRightTab(
                                                                      "addresses",
                                                                  )
                                                                  setAddressSourceFilter(
                                                                      (curr) =>
                                                                          curr ===
                                                                          line.rowKey
                                                                              ? null
                                                                              : line.rowKey,
                                                                  )
                                                              }
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

                                        {!addDisabledReason ? (
                                            <button
                                                id={`sourcing-product-${productSeg}-split-footer`}
                                                type="button"
                                                onClick={handleAddSourceClick}
                                                className="flex w-full cursor-pointer items-center justify-center gap-1.5 rounded-lg border border-dashed border-border bg-card/60 px-3 py-2.5 text-xs font-medium text-muted-foreground transition-colors outline-none hover:border-primary/45 hover:bg-card hover:text-primary focus-visible:ring-2 focus-visible:ring-ring"
                                            >
                                                <PlusIcon
                                                    aria-hidden="true"
                                                    className="size-3.5"
                                                />
                                                添加新来源（支持库存 / 集采入仓
                                                / 代发直发混配，余{" "}
                                                {unusedOptions.length} 个方案）
                                            </button>
                                        ) : null}
                                    </div>
                                </section>
                            </aside>

                            {/* 右栏：全高主工作区（交付地址分配 / 候选方案比价 / 客户收货地址） */}
                            <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-card">
                                {/* 飞书底线页签栏 */}
                                <div className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-border/70 bg-card px-5">
                                    <Tabs
                                        value={resolvedRightTab}
                                        onValueChange={(nextTab) => {
                                            if (
                                                nextTab === "addresses" ||
                                                nextTab === "compare" ||
                                                nextTab === "readonly_addresses"
                                            ) {
                                                setRightTab(nextTab)
                                                if (nextTab === "compare") {
                                                    setCompareIntent("replace")
                                                }
                                            }
                                        }}
                                    >
                                        <TabsList
                                            variant="line"
                                            aria-label="右侧供给工作区视图"
                                            className="gap-6"
                                        >
                                            {hasDirectAddressWork ? (
                                                <TabsTrigger
                                                    id="sourcing-product-adjust-tab-addresses"
                                                    value="addresses"
                                                    className="cursor-pointer gap-2 text-foreground/75 hover:text-primary after:bottom-0 hover:after:opacity-35 data-active:font-semibold data-active:text-primary"
                                                >
                                                    <MapPinIcon
                                                        aria-hidden="true"
                                                        className="size-3.5"
                                                    />
                                                    <span>交付地址分配</span>
                                                    {addressTabBadge ? (
                                                        <span
                                                            className={cn(
                                                                "num inline-flex shrink-0 items-center gap-1 rounded-sm px-1.5 py-0.5 text-tiny font-medium",
                                                                addressTabBadge.tone ===
                                                                    "destructive"
                                                                    ? "bg-destructive-soft text-destructive-soft-foreground"
                                                                    : addressTabBadge.tone ===
                                                                        "warning"
                                                                      ? "bg-warning-soft text-warning-soft-foreground"
                                                                      : "bg-success-soft text-success-soft-foreground",
                                                            )}
                                                        >
                                                            {addressTabBadge.tone ===
                                                            "success" ? (
                                                                <CheckCircle2Icon
                                                                    aria-hidden="true"
                                                                    className="size-3"
                                                                />
                                                            ) : null}
                                                            {
                                                                addressTabBadge.text
                                                            }
                                                        </span>
                                                    ) : null}
                                                </TabsTrigger>
                                            ) : null}

                                            <TabsTrigger
                                                id="sourcing-product-adjust-tab-compare"
                                                value="compare"
                                                className="cursor-pointer gap-2 text-foreground/75 hover:text-primary after:bottom-0 hover:after:opacity-35 data-active:font-semibold data-active:text-primary"
                                            >
                                                <ScaleIcon
                                                    aria-hidden="true"
                                                    className="size-3.5"
                                                />
                                                <span>
                                                    {compareIntent === "add" &&
                                                    resolvedRightTab ===
                                                        "compare"
                                                        ? "选择并添加新来源"
                                                        : "候选方案比价"}
                                                </span>
                                                <span className="num rounded-sm bg-muted px-1.5 py-0 text-tiny font-medium text-muted-foreground">
                                                    {row.product.options.length}
                                                </span>
                                            </TabsTrigger>

                                            {hasReadonlyAddresses ? (
                                                <TabsTrigger
                                                    id="sourcing-product-adjust-tab-readonly-addresses"
                                                    value="readonly_addresses"
                                                    className="cursor-pointer gap-2 text-foreground/75 hover:text-primary after:bottom-0 hover:after:opacity-35 data-active:font-semibold data-active:text-primary"
                                                >
                                                    <MapPinIcon
                                                        aria-hidden="true"
                                                        className="size-3.5"
                                                    />
                                                    <span>客户收货地址</span>
                                                    <span className="num rounded-sm bg-muted px-1.5 py-0 text-tiny font-medium text-muted-foreground">
                                                        {
                                                            row.product
                                                                .deliveryAllocations
                                                                .length
                                                        }{" "}
                                                        处
                                                    </span>
                                                </TabsTrigger>
                                            ) : null}
                                        </TabsList>
                                    </Tabs>

                                    {resolvedRightTab === "compare" ? (
                                        <p className="hidden text-xs text-muted-foreground xl:block">
                                            {compareIntent === "add"
                                                ? "点击任意未加入方案直接新增到左侧拆单，若有缺口自动带入余量"
                                                : `可直接替换左侧 源${activeSourceNumber}，或点「+ 拆单」新增为独立来源`}
                                        </p>
                                    ) : null}
                                </div>

                                {/* 右栏正文 */}
                                {resolvedRightTab === "addresses" &&
                                hasDirectAddressWork ? (
                                    <SourcingAddressEditor
                                        form={props.form}
                                        row={row}
                                        activeRowKey={activeRowKey}
                                        canSplitNewSource={!addDisabledReason}
                                        sourceFilter={addressSourceFilter}
                                        onSourceFilterChange={
                                            setAddressSourceFilter
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
                                ) : resolvedRightTab === "readonly_addresses" &&
                                  hasReadonlyAddresses ? (
                                    <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
                                        <SourcingDeliveryRequirementSection
                                            product={row.product}
                                            condensed
                                            note="当前已选来源均由仓库/库存出库时统一安排客户地址；如需部分直发，请切到「候选方案比价」拆单加入代发或集采直发来源。"
                                        />
                                    </div>
                                ) : (
                                    <SourcingOptionCompareMatrix
                                        idPrefix={`sourcing-compare-${productSeg}`}
                                        order={props.order}
                                        product={row.product}
                                        selectedBasisId={
                                            compareIntent === "replace"
                                                ? (active?.line.basisId ?? "")
                                                : ""
                                        }
                                        usedBasisIds={
                                            compareIntent === "add"
                                                ? usedBasisIdsAll
                                                : usedBasisIdsOtherThanActive
                                        }
                                        recommendedBasisId={recommendedBasisId}
                                        activeSourceNumber={
                                            compareIntent === "replace"
                                                ? activeSourceNumber
                                                : undefined
                                        }
                                        mode={compareIntent}
                                        fullHeight
                                        canAddAsNewSource={!addDisabledReason}
                                        onSelect={(option) => {
                                            if (compareIntent === "add") {
                                                handleAddSourceByOption(
                                                    option.basisId,
                                                )
                                                return
                                            }
                                            if (!active) return
                                            applySourcingOptionToLine(
                                                props.form,
                                                row.product,
                                                active.index,
                                                option,
                                            )
                                        }}
                                        onAddAsNewSource={
                                            !addDisabledReason
                                                ? (option) =>
                                                      handleAddSourceByOption(
                                                          option.basisId,
                                                      )
                                                : undefined
                                        }
                                    />
                                )}
                            </main>
                        </DialogBody>

                        {/* 全宽底栏 */}
                        <DialogFooter className="items-center border-border/70 bg-card sm:justify-between">
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
