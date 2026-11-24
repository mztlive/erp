"use client"

import { useEffect, useMemo, useRef, useState } from "react"
import {
    ArrowLeftIcon,
    ArrowRightIcon,
    CheckCircle2Icon,
    PlusIcon,
    RotateCcwIcon,
    SplitIcon,
    TriangleAlertIcon,
} from "lucide-react"
import { DetailWorkspaceMeta, MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
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
import { compareDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import { findSourcingOption } from "../lib/purchase-order-create-model"
import { sourcingFormValidationError } from "../lib/purchase-order-create-validation"
import { canSplitSourcingProduct } from "../lib/sourcing-quantity"
import { computeGlobalAddressState } from "../lib/sourcing/address-grouping"
import { usesAddressQuantity } from "../lib/sourcing/delivery-editor"
import { sourcingRouteLabel } from "../lib/sourcing/delivery-terms"
import { buildSourcingEditorRows } from "../lib/sourcing/editor-rows"
import { summarizeRowPricing } from "../lib/sourcing/pricing"
import type { SourcingLineInput } from "../lib/sourcing/types"
import type {
    SourcingEditorProps,
    SourcingParticipationProps,
} from "./create-sourcing/types"
import { SourcingAddressEditor } from "./sourcing-address-editor"
import { SourcingAllocationSummary } from "./sourcing-allocation-summary"
import { SourcingParticipationControl } from "./sourcing-participation-control"
import { SourcingProductEditor } from "./sourcing-product-editor"
import { SourcingSourceList } from "./sourcing-source-list"

type DialogStep = "source" | "addresses"

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
 * 全宽单商品供给精调弹窗（两步分段式）：
 * - 页头收拢商品身份、售价/最晚交期、紧凑进度胶囊与「本次下单」开关；
 * - ① 供货方案与拆单：聚焦供应商选择、毛利比价、交期/目标仓与多源拆分；
 * - ② 交付地址分配（含直发地址时出现）：独占全宽，提供省市状态树 + 全局地址表 + 批量改派与按量凑单。
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
    const [activeStep, setActiveStep] = useState<DialogStep>("source")
    const [snapshotState, setSnapshotState] = useState<{
        productId: string
        lines: SourcingLineInput[]
    } | null>(null)
    const lastProductId = useRef(productId)
    const bodyRef = useRef<HTMLDivElement>(null)
    const listRef = useRef<HTMLDivElement>(null)
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
    const activeIndex =
        row?.allocations.findIndex(
            ({ line }) => line.rowKey === activeRowKey,
        ) ?? -1
    const activeOption =
        row && active
            ? findSourcingOption(row.product, active.line.basisId)
            : undefined

    const hasDirectAddressStep = Boolean(
        row &&
        row.product.deliveryAllocations.length > 0 &&
        row.allocations.some(({ line }) =>
            usesAddressQuantity(row.product, line),
        ),
    )

    const resolvedStep: DialogStep = hasDirectAddressStep
        ? activeStep
        : "source"

    const addressState = useMemo(
        () =>
            row && hasDirectAddressStep
                ? computeGlobalAddressState(row.product, row.allocations)
                : null,
        [hasDirectAddressStep, row],
    )

    const pricingSummary = useMemo(
        () => (row ? summarizeRowPricing(row) : null),
        [row],
    )

    const addDisabledReason = !row
        ? undefined
        : row.allocations.length >= row.product.options.length
          ? "可用来源均已添加"
          : !canSplitSourcingProduct(row.product, row.allocations.length)
            ? "剩余需求不足以继续拆分，或商品单位不支持拆分"
            : undefined

    const hasResidualDemand = (() => {
        if (!row) return false
        try {
            return (
                compareDecimal(row.quantity, row.product.remainingQuantity, 6) <
                0
            )
        } catch {
            return false
        }
    })()

    const fields =
        sourcingFormValidationError(props.order, props.form.state.values)
            ?.fields ?? {}
    const errors = new Map(
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
    const activeErrors = activeRowKey ? (errors.get(activeRowKey) ?? []) : []
    const allProductErrors = showErrors
        ? [
              ...new Set(
                  (row?.allocations ?? []).flatMap(
                      ({ line }) => errors.get(line.rowKey) ?? [],
                  ),
              ),
          ]
        : []

    // 打开或切换商品时，根据入口语义（compare / addresses）决定默认步骤并拍摄还原快照
    useEffect(() => {
        if (!productId) {
            setSnapshotState(null)
            setActiveStep("source")
            return
        }
        setActiveStep(
            initialFocusSection === "addresses" ? "addresses" : "source",
        )
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

    useEffect(() => {
        if (!activeRowKey || resolvedStep !== "source") return
        bodyRef.current?.scrollTo({ top: 0 })
        const target = document.getElementById(
            `sourcing-source-${toAutomationIdSegment(activeRowKey)}-edit`,
        )
        if (target && listRef.current?.contains(target))
            target.scrollIntoView({ block: "nearest", inline: "nearest" })
    }, [activeRowKey, resolvedStep])

    const close = () => {
        setShowErrors(false)
        setSelectedRowKey(null)
        setActiveStep("source")
        onClose()
    }

    const navigateFreely = (targetId: string) => {
        if (!onNavigate) return
        setShowErrors(false)
        setSelectedRowKey(null)
        setActiveStep("source")
        onNavigate(targetId)
    }

    const focusEditor = (rowKey: string, invalid = false) => {
        requestAnimationFrame(() => {
            const target = invalid
                ? bodyRef.current?.querySelector<HTMLElement>(
                      '[aria-invalid="true"], [role="alert"]',
                  )
                : (document.getElementById(
                      `procurement-orders-create-row-${toAutomationIdSegment(rowKey)}-quantity`,
                  ) ??
                  document.getElementById(
                      `procurement-orders-create-row-${toAutomationIdSegment(rowKey)}-sourcing-option`,
                  ))
            if (!target || !bodyRef.current?.contains(target)) return
            target.scrollIntoView({ block: "nearest", inline: "nearest" })
            target.focus({ preventScroll: true })
        })
    }

    const addSplit = () => {
        if (!row) return
        const previousKeys = new Set(
            row.allocations.map(({ line }) => line.rowKey),
        )
        props.onAddSplit(row.product.salesOrderLineId)
        const added = props.form.state.values.lines.find(
            (line) =>
                line.salesOrderLineId === row.product.salesOrderLineId &&
                !previousKeys.has(line.rowKey),
        )
        if (!added) return
        setActiveStep("source")
        setSelectedRowKey(added.rowKey)
        focusEditor(added.rowKey)
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
                if (resolvedStep === "source") {
                    focusEditor(addedKey)
                }
            }
            return
        }
        addSplit()
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
                hasDirectAddressStep &&
                invalidPaths.every((path) =>
                    path.endsWith(".deliveryAllocations"),
                )
            setSelectedRowKey(firstInvalid.line.rowKey)
            if (onlyAddressErrors) {
                setActiveStep("addresses")
            } else {
                setActiveStep("source")
                focusEditor(firstInvalid.line.rowKey, true)
            }
            return
        }
        close()
    }

    const salesUnitPrice = row?.product.salesPrices?.unit_price_gross

    const step1Summary = (() => {
        if (!row) return ""
        const count = row.allocations.filter(({ line }) => line.selected).length
        const base =
            row.allocations.length > 1
                ? `${count || row.allocations.length} 家组合`
                : "1 家供货"
        if (pricingSummary?.kind === "stock") return `${base} · 库存核算`
        if (
            pricingSummary?.kind === "purchase" &&
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
        if (pricingSummary?.kind === "multiple") return `${base} · 多价格`
        return `${base} · 待选方案`
    })()

    const step2Badge = (() => {
        if (!addressState) return null
        if (addressState.counts.issues > 0) {
            return {
                tone: "destructive" as const,
                text: `${addressState.counts.issues} 处异常`,
            }
        }
        if (addressState.counts.unassigned > 0) {
            return {
                tone: "warning" as const,
                text: `待分 ${addressState.counts.unassigned} 处`,
            }
        }
        return {
            tone: "success" as const,
            text: `已全分 ${addressState.counts.total} 处`,
        }
    })()

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
                        {/* 统一单层页头：商品身份 + 紧凑进度胶囊 + 本次下单开关 */}
                        <DialogHeader className="border-border/60 bg-card py-3">
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

                                <div className="flex shrink-0 flex-wrap items-center gap-2.5">
                                    <SourcingAllocationSummary row={row} />
                                    <SourcingParticipationControl
                                        idPrefix={`sourcing-product-adjust-${toAutomationIdSegment(row.product.salesOrderLineId)}`}
                                        itemName={row.product.itemName}
                                        included={
                                            row.selected ||
                                            row.partiallySelected
                                        }
                                        onChange={(included) =>
                                            onSetParticipation(
                                                [row.product.salesOrderLineId],
                                                included,
                                            )
                                        }
                                        showHint={false}
                                        variant="pill"
                                    />
                                </div>
                            </div>
                            <DialogDescription className="sr-only">
                                第一步选择供货方案、交期或拆单给多家供应商；若含直发收货地址，第二步在全局地址表中按地区、状态或数量批量分配。修改实时存入草稿。
                            </DialogDescription>
                        </DialogHeader>

                        {/* 两步分段标签栏（仅含直发收货地址时显示）：沿用全站飞书底线 Tab，清晰表达可切换 */}
                        {hasDirectAddressStep ? (
                            <div className="flex shrink-0 flex-wrap items-center justify-between gap-x-4 gap-y-1 border-b border-border/70 bg-card px-4 sm:px-6">
                                <Tabs
                                    value={resolvedStep}
                                    onValueChange={(nextStep) => {
                                        if (
                                            nextStep === "source" ||
                                            nextStep === "addresses"
                                        ) {
                                            setActiveStep(nextStep)
                                        }
                                    }}
                                >
                                    <TabsList
                                        variant="line"
                                        aria-label="供给分配步骤"
                                        className="gap-6"
                                    >
                                        <TabsTrigger
                                            id="sourcing-product-adjust-step-source"
                                            value="source"
                                            className="cursor-pointer gap-2 text-foreground/75 hover:text-primary after:bottom-0 hover:after:opacity-35 data-active:font-semibold data-active:text-primary"
                                        >
                                            <span
                                                className={cn(
                                                    "num inline-flex size-4.5 shrink-0 items-center justify-center rounded-full text-tiny font-semibold transition-colors",
                                                    resolvedStep === "source"
                                                        ? "bg-primary text-primary-foreground"
                                                        : "bg-muted text-muted-foreground",
                                                )}
                                            >
                                                1
                                            </span>
                                            <span>供货方案与拆单</span>
                                        </TabsTrigger>

                                        <TabsTrigger
                                            id="sourcing-product-adjust-step-addresses"
                                            value="addresses"
                                            className="cursor-pointer gap-2 text-foreground/75 hover:text-primary after:bottom-0 hover:after:opacity-35 data-active:font-semibold data-active:text-primary"
                                        >
                                            <span
                                                className={cn(
                                                    "num inline-flex size-4.5 shrink-0 items-center justify-center rounded-full text-tiny font-semibold transition-colors",
                                                    resolvedStep === "addresses"
                                                        ? "bg-primary text-primary-foreground"
                                                        : "bg-muted text-muted-foreground",
                                                )}
                                            >
                                                2
                                            </span>
                                            <span>交付地址分配</span>
                                            {step2Badge ? (
                                                <span
                                                    className={cn(
                                                        "num inline-flex shrink-0 items-center gap-1 rounded-sm px-1.5 py-0.5 text-tiny font-medium",
                                                        step2Badge.tone ===
                                                            "destructive"
                                                            ? "bg-destructive-soft text-destructive-soft-foreground"
                                                            : step2Badge.tone ===
                                                                "warning"
                                                              ? "bg-warning-soft text-warning-soft-foreground"
                                                              : "bg-success-soft text-success-soft-foreground",
                                                    )}
                                                >
                                                    {step2Badge.tone ===
                                                    "success" ? (
                                                        <CheckCircle2Icon
                                                            aria-hidden="true"
                                                            className="size-3"
                                                        />
                                                    ) : null}
                                                    {step2Badge.text}
                                                </span>
                                            ) : null}
                                        </TabsTrigger>
                                    </TabsList>
                                </Tabs>

                                <p className="hidden truncate text-xs text-muted-foreground md:block">
                                    {step1Summary}
                                </p>
                            </div>
                        ) : null}

                        <DialogBody className="flex h-[min(76dvh,48rem)] min-h-0 flex-auto flex-col overflow-hidden p-0">
                            {resolvedStep === "source" ? (
                                <>
                                    {/* 多来源时展示清晰来源切换条；单源时隐藏避免重复卡片 */}
                                    {row.allocations.length > 1 ? (
                                        <section
                                            ref={listRef}
                                            className="flex shrink-0 flex-wrap items-center justify-between gap-2.5 border-b border-border/60 bg-sidebar px-4 py-2.5 sm:px-6"
                                            aria-label="供给来源列表"
                                        >
                                            <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">
                                                <SourcingSourceList
                                                    row={row}
                                                    activeRowKey={activeRowKey}
                                                    onSelect={setSelectedRowKey}
                                                    onRemove={removeSource}
                                                    errors={errors}
                                                />
                                            </div>

                                            <div className="flex shrink-0 flex-wrap items-center gap-1.5">
                                                {hasResidualDemand &&
                                                !addDisabledReason ? (
                                                    <Button
                                                        id={`sourcing-product-${toAutomationIdSegment(row.product.salesOrderLineId)}-split-residual`}
                                                        type="button"
                                                        variant="secondary"
                                                        size="xs"
                                                        onClick={() =>
                                                            splitResidual()
                                                        }
                                                    >
                                                        <SplitIcon aria-hidden="true" />
                                                        剩余缺口拆给新来源
                                                    </Button>
                                                ) : null}
                                                <Button
                                                    id={`sourcing-product-${toAutomationIdSegment(row.product.salesOrderLineId)}-split`}
                                                    type="button"
                                                    variant="outline"
                                                    size="xs"
                                                    disabled={Boolean(
                                                        addDisabledReason,
                                                    )}
                                                    onClick={addSplit}
                                                    title={
                                                        addDisabledReason ??
                                                        `共 ${row.product.options.length} 个可用供给方案，支持多源拆分`
                                                    }
                                                >
                                                    <PlusIcon aria-hidden="true" />
                                                    添加来源
                                                </Button>
                                            </div>
                                        </section>
                                    ) : null}

                                    <section
                                        id="sourcing-product-adjust-detail"
                                        className="flex min-h-0 min-w-0 flex-1 flex-col bg-card"
                                        aria-label="当前来源编辑"
                                    >
                                        {active &&
                                        row.allocations.length > 1 ? (
                                            <div className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-border/60 bg-card px-4 py-2 sm:px-6">
                                                <div className="flex items-center gap-2">
                                                    <h3 className="text-body-compact font-semibold text-foreground">
                                                        正在编辑：来源{" "}
                                                        {activeIndex + 1}
                                                    </h3>
                                                    {activeOption ? (
                                                        <Badge
                                                            variant={
                                                                activeOption.sourceType ===
                                                                "EXISTING_STOCK"
                                                                    ? "info"
                                                                    : "neutral"
                                                            }
                                                        >
                                                            {sourcingRouteLabel(
                                                                activeOption,
                                                            )}
                                                        </Badge>
                                                    ) : null}
                                                </div>
                                                <props.form.AppField
                                                    name={`lines[${active.index}].selected`}
                                                >
                                                    {(field) => (
                                                        <label
                                                            className="flex cursor-pointer items-center gap-2 text-xs font-medium text-muted-foreground select-none hover:text-foreground"
                                                            htmlFor={`procurement-orders-create-row-${toAutomationIdSegment(active.line.rowKey)}-select`}
                                                        >
                                                            <Checkbox
                                                                id={`procurement-orders-create-row-${toAutomationIdSegment(active.line.rowKey)}-select`}
                                                                checked={
                                                                    field.state
                                                                        .value ===
                                                                    true
                                                                }
                                                                onCheckedChange={(
                                                                    checked,
                                                                ) =>
                                                                    field.handleChange(
                                                                        checked ===
                                                                            true,
                                                                    )
                                                                }
                                                            />
                                                            本次使用此来源
                                                        </label>
                                                    )}
                                                </props.form.AppField>
                                            </div>
                                        ) : null}

                                        <div
                                            id="sourcing-product-adjust-body"
                                            ref={bodyRef}
                                            className="@container/sourcing min-h-0 flex-1 overflow-y-auto overscroll-contain"
                                        >
                                            {activeErrors.length ? (
                                                <div
                                                    id="sourcing-product-adjust-source-errors"
                                                    role="alert"
                                                    tabIndex={-1}
                                                    className="mx-4 mt-3.5 flex items-start gap-2 rounded-lg border border-destructive-border bg-destructive-soft px-3.5 py-2.5 text-xs text-destructive-soft-foreground outline-none @min-[560px]/sourcing:mx-6"
                                                >
                                                    <TriangleAlertIcon
                                                        aria-hidden="true"
                                                        className="mt-0.5 size-3.5 shrink-0"
                                                    />
                                                    <div className="space-y-0.5">
                                                        {activeErrors.map(
                                                            (message) => (
                                                                <p
                                                                    key={
                                                                        message
                                                                    }
                                                                >
                                                                    {message}
                                                                </p>
                                                            ),
                                                        )}
                                                    </div>
                                                </div>
                                            ) : null}
                                            {active ? (
                                                <SourcingProductEditor
                                                    key={active.line.rowKey}
                                                    {...props}
                                                    condensed
                                                    hideAddressEditor={
                                                        hasDirectAddressStep
                                                    }
                                                    activeRowKey={
                                                        active.line.rowKey
                                                    }
                                                    defaultCompareOpen={
                                                        initialFocusSection ===
                                                        "compare"
                                                    }
                                                    onSplitResidual={
                                                        !addDisabledReason
                                                            ? () =>
                                                                  splitResidual()
                                                            : undefined
                                                    }
                                                    onAddSplitAction={addSplit}
                                                    addSplitDisabledReason={
                                                        addDisabledReason
                                                    }
                                                    onGoToAddresses={
                                                        hasDirectAddressStep
                                                            ? () =>
                                                                  setActiveStep(
                                                                      "addresses",
                                                                  )
                                                            : undefined
                                                    }
                                                    onSetParticipation={
                                                        onSetParticipation
                                                    }
                                                    showParticipation={false}
                                                    row={row}
                                                />
                                            ) : (
                                                <p className="p-6 text-xs text-muted-foreground">
                                                    尚未添加供给来源
                                                </p>
                                            )}
                                        </div>
                                    </section>
                                </>
                            ) : (
                                <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
                                    {allProductErrors.length > 0 ? (
                                        <div
                                            id="sourcing-product-adjust-source-errors"
                                            role="alert"
                                            tabIndex={-1}
                                            className="flex shrink-0 items-start gap-2 border-b border-destructive-border bg-destructive-soft px-5 py-2.5 text-xs text-destructive-soft-foreground outline-none"
                                        >
                                            <TriangleAlertIcon
                                                aria-hidden="true"
                                                className="mt-0.5 size-3.5 shrink-0"
                                            />
                                            <div className="space-y-0.5">
                                                {allProductErrors.map(
                                                    (message) => (
                                                        <p key={message}>
                                                            {message}
                                                        </p>
                                                    ),
                                                )}
                                            </div>
                                        </div>
                                    ) : null}
                                    <SourcingAddressEditor
                                        form={props.form}
                                        row={row}
                                        canSplitNewSource={!addDisabledReason}
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
                            )}
                        </DialogBody>

                        {/* 精简底栏：只保留必要操作，无待调整项时隐藏无效按钮 */}
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
