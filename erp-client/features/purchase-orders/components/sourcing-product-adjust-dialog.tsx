"use client"

import { useEffect, useRef, useState } from "react"
import {
    ArrowLeftIcon,
    ArrowRightIcon,
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
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal } from "@/lib/fixed-decimal"
import { findSourcingOption } from "../lib/purchase-order-create-model"
import { sourcingFormValidationError } from "../lib/purchase-order-create-validation"
import { canSplitSourcingProduct } from "../lib/sourcing-quantity"
import { sourcingRouteLabel } from "../lib/sourcing/delivery-terms"
import { buildSourcingEditorRows } from "../lib/sourcing/editor-rows"
import type { SourcingLineInput } from "../lib/sourcing/types"
import type {
    SourcingEditorProps,
    SourcingParticipationProps,
} from "./create-sourcing/types"
import { SourcingAllocationSummary } from "./sourcing-allocation-summary"
import { SourcingParticipationControl } from "./sourcing-participation-control"
import { SourcingProductEditor } from "./sourcing-product-editor"
import { SourcingSourceList } from "./sourcing-source-list"

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
 * 全宽单商品供给精调弹窗：
 * 顶部动态来源条取代固定左栏，正文完整释放给「比价选源矩阵 + 一体化地址分配」；
 * 支持自由切品比对、单商品修改还原与一键把剩余缺口拆给新来源。
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

    // 记录进入每个商品时的草稿快照，支持一键“还原此商品修改”
    useEffect(() => {
        if (!productId) {
            setSnapshotState(null)
            return
        }
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
    }, [productId, props.form.state.values.lines])

    const currentProductLines = productId
        ? cloneProductLines(props.form.state.values.lines, productId)
        : []
    const isDirtyForProduct =
        Boolean(snapshotState && snapshotState.productId === productId) &&
        JSON.stringify(currentProductLines) !==
            JSON.stringify(snapshotState?.lines ?? [])

    useEffect(() => {
        if (!activeRowKey) return
        if (initialFocusSection === "addresses") {
            requestAnimationFrame(() => {
                bodyRef.current
                    ?.querySelector('[aria-label="本来源交付地址"]')
                    ?.scrollIntoView({ block: "start", behavior: "smooth" })
            })
        } else {
            bodyRef.current?.scrollTo({ top: 0 })
        }
        const target = document.getElementById(
            `sourcing-source-${toAutomationIdSegment(activeRowKey)}-edit`,
        )
        if (target && listRef.current?.contains(target))
            target.scrollIntoView({ block: "nearest", inline: "nearest" })
    }, [activeRowKey, initialFocusSection])

    const close = () => {
        setShowErrors(false)
        setSelectedRowKey(null)
        onClose()
    }

    const navigateFreely = (targetId: string) => {
        if (!onNavigate) return
        setShowErrors(false)
        setSelectedRowKey(null)
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
        setSelectedRowKey(added.rowKey)
        focusEditor(added.rowKey)
    }

    const splitResidual = () => {
        if (!row) return
        if (props.onAddResidualSplit) {
            const addedKey = props.onAddResidualSplit(
                row.product.salesOrderLineId,
            )
            if (addedKey) {
                setSelectedRowKey(addedKey)
                focusEditor(addedKey)
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
        const invalid = row.allocations.find(({ index }) =>
            Object.keys(currentErrors).some((path) =>
                path.startsWith(`lines[${index}].`),
            ),
        )
        if (invalid) {
            setShowErrors(true)
            setSelectedRowKey(invalid.line.rowKey)
            focusEditor(invalid.line.rowKey, true)
            return
        }
        close()
    }

    const supplyRefPrice =
        row?.product.salesPrices?.procurement_supply_price_gross
    const salesUnitPrice = row?.product.salesPrices?.unit_price_gross

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
                                            supplyRefPrice ? (
                                                <span key="ref-price">
                                                    参考供货价{" "}
                                                    <MoneyValue
                                                        display="exact"
                                                        value={supplyRefPrice}
                                                        className="font-medium [&_[data-slot=money-value-number]]:text-foreground/85"
                                                    />
                                                </span>
                                            ) : null,
                                            row.product.deliveryAllocations
                                                .length > 0
                                                ? `共 ${row.product.deliveryAllocations.length} 个收货地址`
                                                : null,
                                        ]}
                                    />
                                </div>
                                <div className="shrink-0">
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
                                顶部切换或拆分供给来源，正文比价选源并分配数量与收货地址。修改保留在草稿中，整单预览后统一提交。
                            </DialogDescription>
                        </DialogHeader>

                        <SourcingAllocationSummary row={row} />

                        <DialogBody className="flex h-[min(74dvh,48rem)] min-h-0 flex-auto flex-col overflow-hidden p-0">
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
                                    {hasResidualDemand && !addDisabledReason ? (
                                        <Button
                                            id={`sourcing-product-${toAutomationIdSegment(row.product.salesOrderLineId)}-split-residual`}
                                            type="button"
                                            variant="secondary"
                                            size="xs"
                                            onClick={splitResidual}
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
                                        disabled={Boolean(addDisabledReason)}
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

                            <section
                                id="sourcing-product-adjust-detail"
                                className="flex min-h-0 min-w-0 flex-1 flex-col bg-card"
                                aria-label="当前来源编辑"
                            >
                                {active && row.allocations.length > 1 ? (
                                    <div className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-border/60 bg-card px-4 py-2 sm:px-6">
                                        <div className="flex items-center gap-2">
                                            <h3 className="text-body-compact font-semibold text-foreground">
                                                正在编辑：来源 {activeIndex + 1}
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
                                                                .value === true
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
                                                {activeErrors.map((message) => (
                                                    <p key={message}>
                                                        {message}
                                                    </p>
                                                ))}
                                            </div>
                                        </div>
                                    ) : null}
                                    {active ? (
                                        <SourcingProductEditor
                                            key={active.line.rowKey}
                                            {...props}
                                            condensed
                                            activeRowKey={active.line.rowKey}
                                            defaultCompareOpen={
                                                initialFocusSection ===
                                                "compare"
                                            }
                                            onSplitResidual={
                                                !addDisabledReason
                                                    ? splitResidual
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
                        </DialogBody>

                        <DialogFooter className="items-center border-border/60 bg-card sm:justify-between">
                            <div className="flex flex-wrap items-center gap-2.5">
                                {onRestoreProductLines && snapshotState ? (
                                    <Button
                                        id="sourcing-product-adjust-restore"
                                        type="button"
                                        variant="ghost"
                                        size="sm"
                                        disabled={!isDirtyForProduct}
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
                                        还原此商品修改
                                    </Button>
                                ) : null}
                                {showErrors && row.issues.length ? (
                                    <StatusBadge
                                        tone="destructive"
                                        label={`还有 ${row.issues.length} 项需调整`}
                                    />
                                ) : (
                                    <p className="text-xs text-muted-foreground">
                                        实时存入草稿，整单预览后统一提交
                                    </p>
                                )}
                            </div>
                            <div className="flex flex-wrap items-center gap-2">
                                {onNavigate ? (
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
                                        <Button
                                            id="sourcing-product-adjust-next-pending"
                                            type="button"
                                            variant="outline"
                                            disabled={!nextPending}
                                            onClick={() =>
                                                nextPending &&
                                                navigateFreely(
                                                    nextPending.product
                                                        .salesOrderLineId,
                                                )
                                            }
                                        >
                                            下一待调整
                                            {pendingItems.length > 0 ? (
                                                <span className="num rounded-sm bg-warning-soft px-1.5 py-0 text-tiny font-medium text-warning-soft-foreground">
                                                    {pendingItems.length}
                                                </span>
                                            ) : null}
                                        </Button>
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
