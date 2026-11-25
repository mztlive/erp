"use client"

import { useEffect, useMemo, useRef, useState } from "react"
import {
    ArrowLeftIcon,
    ArrowRightIcon,
    InfoIcon,
    MapPinIcon,
    RotateCcwIcon,
    TriangleAlertIcon,
    WarehouseIcon,
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
    findLowestPurchaseCost,
    summarizeWeightedProcurementCost,
} from "../lib/sourcing/pricing"
import type {
    SourcingLineInput,
    SourcingSupplierOption,
} from "../lib/sourcing/types"
import type {
    SourcingEditorProps,
    SourcingParticipationProps,
} from "./create-sourcing/types"
import {
    SourcingAddressEditor,
    type AddressSourceFilter,
} from "./sourcing-address-editor"
import { SourcingAllocationSummary } from "./sourcing-allocation-summary"
import { SourcingCandidatePool } from "./sourcing-candidate-pool"
import { SourcingDeliveryRequirementSection } from "./sourcing-delivery-plan"
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
 * 全宽高左右双栏供给分配工作面（方案 B：左供给比价池 → 右常驻地址路由表）：
 * - 左栏一体化呈现「配平进度 + 已选用来源 + 其他候选方案（默认按低价优先排序）」，
 *   彻底消除“先点添加来源 → 再去右侧切 Tab 比价”的多余路径；
 * - 右栏常驻「交付地址分配」（默认全局路由表一步改派并自动守恒配平仓库数量；
 *   点击左侧任一来源即可筛选右侧只看该源自己的地址，再次点击恢复全部）；
 * - 若商品无交付地址要求，左栏展示已选来源、右栏全宽铺开候选比价池，无留白浪费。
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
    const [addressSourceFilter, setAddressSourceFilter] =
        useState<AddressSourceFilter>(null)
    const [previewDirectRowKey, setPreviewDirectRowKey] = useState<
        string | null
    >(null)
    const [rescueAddressScope, setRescueAddressScope] = useState<{
        label: string
        keys: ReadonlySet<string>
    } | null>(null)
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

    const hasDeliveryAddresses = Boolean(
        row && row.product.deliveryAllocations.length > 0,
    )
    const hasDirectAddressWork = Boolean(
        row &&
        hasDeliveryAddresses &&
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

    const unusedCount = useMemo(
        () =>
            (row?.product.options ?? []).filter(
                (opt) => !usedBasisIdsAll.has(opt.basisId),
            ).length,
        [row, usedBasisIdsAll],
    )

    const lowestPurchaseCost = useMemo(
        () => (row ? findLowestPurchaseCost(row.product.options) : undefined),
        [row],
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

    const canSplit = Boolean(
        row &&
        unusedCount > 0 &&
        canSplitSourcingProduct(row.product, row.allocations.length),
    )

    const weightedSummary = useMemo(
        () => (row ? summarizeWeightedProcurementCost(row) : null),
        [row],
    )

    const headerPricing = useMemo(() => {
        if (!row || !weightedSummary) return null
        const sourceCount = row.allocations.filter(
            ({ line }) => line.selected,
        ).length
        const prefix =
            row.allocations.length > 1
                ? `${sourceCount || row.allocations.length} 路组合`
                : "单源供货"
        if (
            weightedSummary.purchaseSourceCount === 0 &&
            weightedSummary.stockSourceCount > 0
        ) {
            return { text: `${prefix} · 库存核算`, inverted: false }
        }
        if (weightedSummary.weightedUnitCostGross) {
            const priceLabel = weightedSummary.hasMixedPrices
                ? `综合均价 ¥${weightedSummary.weightedUnitCostGross}`
                : `¥${weightedSummary.weightedUnitCostGross}`
            const marginText = weightedSummary.hasInvertedSource
                ? "含价格倒挂"
                : weightedSummary.margin?.marginRateText
                  ? `毛利 ${weightedSummary.margin.marginRateText}`
                  : ""
            const totalText =
                weightedSummary.totalProcurementCost &&
                row.allocations.length > 1
                    ? `预估总额 ¥${weightedSummary.totalProcurementCost}`
                    : ""
            return {
                text: [prefix, priceLabel, marginText, totalText]
                    .filter(Boolean)
                    .join(" · "),
                inverted: weightedSummary.hasInvertedSource,
            }
        }
        return { text: `${prefix} · 待选方案`, inverted: false }
    }, [row, weightedSummary])

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

    const productSeg = row
        ? toAutomationIdSegment(row.product.salesOrderLineId)
        : ""

    const scrollToCandidatePool = () => {
        requestAnimationFrame(() => {
            document
                .getElementById(`sourcing-pool-${productSeg}-candidate-pool`)
                ?.scrollIntoView({ behavior: "smooth", block: "nearest" })
        })
    }

    // 打开或切换商品时重置局部筛选并拍摄还原快照
    useEffect(() => {
        if (!productId) {
            setSnapshotState(null)
            setAddressSourceFilter(null)
            setPreviewDirectRowKey(null)
            setRescueAddressScope(null)
            setActionNotice(null)
            return
        }
        setAddressSourceFilter(null)
        setPreviewDirectRowKey(null)
        setRescueAddressScope(null)
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
        if (initialFocusSection === "compare") {
            const seg = toAutomationIdSegment(productId)
            requestAnimationFrame(() => {
                document
                    .getElementById(`sourcing-pool-${seg}-candidate-pool`)
                    ?.scrollIntoView({ behavior: "smooth", block: "nearest" })
            })
        }
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
        setAddressSourceFilter(null)
        setPreviewDirectRowKey(null)
        setRescueAddressScope(null)
        setActionNotice(null)
        onClose()
    }

    const navigateFreely = (targetId: string) => {
        if (!onNavigate) return
        setShowErrors(false)
        setSelectedRowKey(null)
        setAddressSourceFilter(null)
        setPreviewDirectRowKey(null)
        setRescueAddressScope(null)
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
                      `procurement-orders-create-row-${seg}-warehouse`,
                  ) ??
                  document.getElementById(
                      `procurement-orders-create-row-${seg}-quantity`,
                  ) ??
                  document.getElementById(
                      `procurement-orders-create-row-${seg}-sourcing-option`,
                  ))
            if (!target) return
            target.scrollIntoView({ block: "nearest", inline: "nearest" })
            target.focus({ preventScroll: true })
        })
    }

    const handleAddCandidateOption = (option: SourcingSupplierOption) => {
        if (!row) return
        let addedKey: string | undefined
        if (props.onAddSplitWithOption) {
            addedKey = props.onAddSplitWithOption(
                row.product.salesOrderLineId,
                option.basisId,
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
            // 防呆：刚选入新来源时右侧保持「全部地址」视图，便于直接把地址划给它，不切入空筛选
            setAddressSourceFilter(null)
            setRescueAddressScope(null)
            const isDirect =
                option.sourceType === "PURCHASE" &&
                option.fulfillmentResponsibility === "SUPPLIER_DIRECT"
            setPreviewDirectRowKey(isDirect ? addedKey : null)
            focusRowControl(addedKey)
        }
    }

    const handleReplaceActiveOption = (option: SourcingSupplierOption) => {
        if (!row || !active) return
        applySourcingOptionToLine(props.form, row.product, active.index, option)
        setRescueAddressScope(null)
        setPreviewDirectRowKey(null)
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
        if (previewDirectRowKey === rowKey) setPreviewDirectRowKey(null)
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
                setAddressSourceFilter(
                    addressState && addressState.counts.issues > 0
                        ? "__issues__"
                        : null,
                )
            } else {
                focusRowControl(firstInvalid.line.rowKey, true)
            }
            return
        }
        close()
    }

    const salesUnitPrice = row?.product.salesPrices?.unit_price_gross
    const included = Boolean(row && (row.selected || row.partiallySelected))

    const candidatePoolNode = row ? (
        <SourcingCandidatePool
            idPrefix={`sourcing-pool-${productSeg}`}
            order={props.order}
            product={row.product}
            usedBasisIds={usedBasisIdsAll}
            lowestPurchaseCost={lowestPurchaseCost}
            recommendedBasisId={recommendedBasisId}
            activeSourceNumber={activeSourceNumber}
            canSplit={canSplit}
            showReplaceAction={row.allocations.length === 1}
            rescueAddressScope={rescueAddressScope}
            onClearRescueScope={() => setRescueAddressScope(null)}
            onAddOption={handleAddCandidateOption}
            onReplaceActive={handleReplaceActiveOption}
        />
    ) : null

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
                        {/* 全宽顶栏：商品身份 + 加权采购成本/毛利 + 本次下单开关 */}
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
                                    {headerPricing ? (
                                        <span
                                            className={cn(
                                                "num hidden text-xs md:inline",
                                                headerPricing.inverted
                                                    ? "font-medium text-destructive"
                                                    : "text-muted-foreground",
                                            )}
                                        >
                                            {headerPricing.text}
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
                                左侧直接按低价比较并选用供给来源，右侧常驻交付地址路由表进行一步分配或按源核对。修改实时存入草稿。
                            </DialogDescription>
                        </DialogHeader>

                        {/* 全局校验错误条 / 暂缓条 / 操作提示条 */}
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

                        {/* 主工作面（方案 B）：左栏供给比价池 + 右栏常驻地址路由（无地址时右栏铺开候选池） */}
                        <DialogBody className="flex min-h-0 flex-1 flex-col overflow-y-auto p-0 lg:flex-row lg:overflow-hidden">
                            {/* 左栏：配平进度 + 已选来源 +（有地址时直接内嵌候选比价池） */}
                            <aside
                                className={cn(
                                    "flex w-full shrink-0 flex-col border-b border-border/70 bg-sidebar lg:h-full lg:overflow-hidden lg:border-r lg:border-b-0",
                                    hasDeliveryAddresses
                                        ? "lg:w-[30rem] xl:w-[35rem] 2xl:w-[38rem]"
                                        : "lg:w-[29rem] xl:w-[32rem]",
                                )}
                            >
                                <div className="shrink-0 p-4 pb-2.5">
                                    <SourcingAllocationSummary
                                        row={row}
                                        addressState={addressState}
                                        onTrimDirectExcess={() =>
                                            handleTrimDirectExcess()
                                        }
                                        onFillDirectShortfall={() =>
                                            handleFillDirectShortfall()
                                        }
                                    />
                                </div>

                                <section
                                    aria-label="供给来源列表"
                                    className="flex min-h-0 flex-1 flex-col"
                                >
                                    <div className="flex shrink-0 items-center justify-between gap-2 px-4 py-1">
                                        <h3 className="text-xs font-semibold text-muted-foreground">
                                            已选用来源（{row.allocations.length}
                                            ）
                                        </h3>
                                        {hasDeliveryAddresses &&
                                        hasDirectAddressWork ? (
                                            <span className="text-tiny text-muted-foreground">
                                                点「源号」或「地址数」可只看该源地址
                                            </span>
                                        ) : null}
                                    </div>

                                    <div
                                        id="sourcing-product-adjust-body"
                                        ref={sourcesScrollRef}
                                        className="min-h-0 flex-1 space-y-2 overflow-y-auto overscroll-contain px-4 pt-1 pb-5"
                                    >
                                        {row.allocations.map(
                                            ({ line, index }, idx) => {
                                                const isDirectLine =
                                                    usesAddressQuantity(
                                                        row.product,
                                                        line,
                                                    )
                                                const filterTarget: AddressSourceFilter =
                                                    isDirectLine
                                                        ? line.rowKey
                                                        : "__warehouse__"
                                                const isFiltered =
                                                    hasDirectAddressWork &&
                                                    addressSourceFilter ===
                                                        filterTarget
                                                return (
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
                                                        canRemove={
                                                            row.allocations
                                                                .length > 1
                                                        }
                                                        errors={
                                                            errorsByRowKey.get(
                                                                line.rowKey,
                                                            ) ?? []
                                                        }
                                                        lowestPurchaseCost={
                                                            lowestPurchaseCost
                                                        }
                                                        addressState={
                                                            addressState
                                                        }
                                                        isAddressFiltered={
                                                            isFiltered
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
                                                                              filterTarget
                                                                                  ? null
                                                                                  : filterTarget,
                                                                      )
                                                                : undefined
                                                        }
                                                        onFocusCheapestCandidate={
                                                            unusedCount > 0
                                                                ? scrollToCandidatePool
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
                                                    />
                                                )
                                            },
                                        )}

                                        {/* 有交付地址时，全部候选比价池紧贴已选来源下方常驻，默认低价优先 */}
                                        {hasDeliveryAddresses ? (
                                            <div className="border-t border-border/60 pt-2.5">
                                                {candidatePoolNode}
                                            </div>
                                        ) : null}
                                    </div>
                                </section>
                            </aside>

                            {/* 右栏：有直发源时为常驻地址路由表；纯仓库/库存时为清晰客户地址概览；无地址时为全宽候选比价池 */}
                            <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-card">
                                {hasDirectAddressWork ? (
                                    <SourcingAddressEditor
                                        form={props.form}
                                        row={row}
                                        sourceFilter={addressSourceFilter}
                                        onSourceFilterChange={(nextFilter) => {
                                            setAddressSourceFilter(nextFilter)
                                            if (nextFilter !== null) {
                                                setPreviewDirectRowKey(null)
                                            }
                                        }}
                                        previewDirectRowKey={
                                            previewDirectRowKey
                                        }
                                        hasUnusedCandidates={unusedCount > 0}
                                        onRescueAddresses={(label, keys) => {
                                            setRescueAddressScope({
                                                label,
                                                keys,
                                            })
                                            scrollToCandidatePool()
                                        }}
                                    />
                                ) : hasDeliveryAddresses ? (
                                    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
                                        <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border/70 bg-surface-sunken/50 px-5 py-2.5 text-xs">
                                            <div className="flex items-center gap-2 font-medium text-foreground">
                                                <WarehouseIcon
                                                    aria-hidden="true"
                                                    className="size-4 text-orange-soft-foreground"
                                                />
                                                <span>
                                                    客户交付地址（
                                                    {
                                                        row.product
                                                            .deliveryAllocations
                                                            .length
                                                    }{" "}
                                                    处）·
                                                    当前全部由仓库/库存统筹
                                                </span>
                                            </div>
                                            <span className="inline-flex items-center gap-1 text-muted-foreground">
                                                <MapPinIcon
                                                    aria-hidden="true"
                                                    className="size-3.5"
                                                />
                                                在左侧选用「代发/集采直发」后即可按地址分派
                                            </span>
                                        </div>
                                        <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
                                            <SourcingDeliveryRequirementSection
                                                product={row.product}
                                                condensed
                                                note="当前已选来源均为现有库存或集采入仓，货物出库时由仓库统一安排发往以下客户地址。"
                                            />
                                        </div>
                                    </div>
                                ) : (
                                    <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain bg-card px-5 py-4">
                                        {candidatePoolNode}
                                    </div>
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
                                            setAddressSourceFilter(null)
                                            setPreviewDirectRowKey(null)
                                            setRescueAddressScope(null)
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
