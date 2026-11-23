"use client"

import { useEffect, useRef, useState } from "react"
import {
    ArrowLeftIcon,
    ArrowRightIcon,
    PlusIcon,
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
import { findSourcingOption } from "../lib/purchase-order-create-model"
import { sourcingFormValidationError } from "../lib/purchase-order-create-validation"
import { sourcingRouteLabel } from "../lib/sourcing/delivery-terms"
import { buildSourcingEditorRows } from "../lib/sourcing/editor-rows"
import { canSplitSourcingProduct } from "../lib/sourcing-quantity"
import type {
    SourcingParticipationProps,
    SourcingEditorProps,
} from "./create-sourcing/types"
import { SourcingParticipationControl } from "./sourcing-participation-control"
import { SourcingProductEditor } from "./sourcing-product-editor"
import { SourcingSourceList } from "./sourcing-source-list"
import { SourcingAllocationSummary } from "./sourcing-allocation-summary"

function statusTone(status: string): StatusTone {
    if (status === "已就绪") return "success"
    if (status === "部分分配" || status === "缺目标仓") return "warning"
    if (status === "待调整") return "destructive"
    return "neutral"
}

/** 左侧比较全部来源，右侧编辑一个来源；全部字段继续使用整单共享草稿。 */
export function SourcingProductAdjustDialog({
    productId,
    onClose,
    onSetParticipation,
    navigationIds,
    onNavigate,
    returnFocusId,
    ...props
}: SourcingEditorProps &
    SourcingParticipationProps & {
        productId: string | null
        onClose: () => void
        navigationIds?: readonly string[]
        onNavigate?: (id: string) => void
        returnFocusId?: string
    }) {
    const [showErrors, setShowErrors] = useState(false)
    const [selectedRowKey, setSelectedRowKey] = useState<string | null>(null)
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
    const addDisabledReason =
        !row || (!row.selected && !row.partiallySelected)
            ? "恢复本次分配后可添加来源"
            : row.allocations.length >= row.product.options.length
              ? "可用来源均已添加"
              : !canSplitSourcingProduct(row.product, row.allocations.length)
                ? "剩余需求不足以继续拆分，或商品单位不支持拆分"
                : undefined
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

    useEffect(() => {
        bodyRef.current?.scrollTo({ top: 0 })
        if (!activeRowKey) return
        const target = document.getElementById(
            `sourcing-source-${toAutomationIdSegment(activeRowKey)}-edit`,
        )
        if (target && listRef.current?.contains(target))
            target.scrollIntoView({ block: "nearest", inline: "nearest" })
    }, [activeRowKey])

    const close = () => {
        setShowErrors(false)
        setSelectedRowKey(null)
        onClose()
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
                      `sourcing-delivery-${toAutomationIdSegment(rowKey)}-add`,
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
    const finish = async (nextId?: string) => {
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
            instance.setMeta((previous) => ({ ...previous, isTouched: true }))
        }
        await props.form.validate("change")
        // 从整单值校验，不能只检查右侧当前挂载的字段。
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
        if (nextId && onNavigate) {
            setShowErrors(false)
            setSelectedRowKey(null)
            onNavigate(nextId)
        } else close()
    }

    const supplyRefPrice =
        row?.product.salesPrices?.procurement_supply_price_gross

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
                              `sourcing-summary-${toAutomationIdSegment(lastProductId.current)}-adjust`,
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
                                左侧比较并选择来源，右侧调整当前来源。修改保留在草稿中，整单预览后统一提交。
                            </DialogDescription>
                        </DialogHeader>

                        <SourcingAllocationSummary row={row} />

                        <DialogBody className="grid h-[min(74dvh,46rem)] min-h-0 flex-auto grid-rows-[minmax(6rem,auto)_minmax(0,1fr)] overflow-hidden p-0 lg:grid-cols-[18rem_minmax(0,1fr)] lg:grid-rows-1">
                            <section
                                className="flex max-h-44 min-h-0 min-w-0 flex-col border-b border-border/60 bg-sidebar lg:max-h-none lg:border-r lg:border-b-0"
                                aria-label="供给来源列表"
                            >
                                <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border/60 px-4 py-2.5">
                                    <div className="flex items-center gap-1.5">
                                        <h3 className="text-body-compact font-semibold text-foreground">
                                            供给来源
                                        </h3>
                                        <span className="num rounded-sm bg-muted px-1.5 py-0.5 text-tiny font-medium text-muted-foreground">
                                            {row.allocations.length}
                                        </span>
                                    </div>
                                    <Button
                                        id={`sourcing-product-${toAutomationIdSegment(row.product.salesOrderLineId)}-split`}
                                        type="button"
                                        variant="outline"
                                        size="xs"
                                        disabled={Boolean(addDisabledReason)}
                                        onClick={addSplit}
                                        title={addDisabledReason}
                                    >
                                        <PlusIcon />
                                        添加来源
                                    </Button>
                                </div>

                                <div
                                    ref={listRef}
                                    className="min-h-0 flex-1 overflow-y-auto overscroll-contain"
                                >
                                    <SourcingSourceList
                                        row={row}
                                        activeRowKey={activeRowKey}
                                        onSelect={setSelectedRowKey}
                                        onRemove={removeSource}
                                        errors={errors}
                                    />
                                </div>

                                <div className="shrink-0 border-t border-border/60 px-4 py-2 text-xs text-muted-foreground">
                                    {addDisabledReason
                                        ? addDisabledReason
                                        : `共 ${row.product.options.length} 个可用供给方案，支持多源拆分`}
                                </div>
                            </section>

                            <section
                                id="sourcing-product-adjust-detail"
                                className="flex min-h-0 min-w-0 flex-col bg-card"
                                aria-label="当前来源编辑"
                            >
                                {active && row.allocations.length > 1 ? (
                                    <div className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-border/60 px-4 py-2.5 lg:px-6">
                                        <div className="flex items-center gap-2">
                                            <h3 className="text-body-compact font-semibold text-foreground">
                                                来源 {activeIndex + 1}
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
                                {showErrors && row.issues.length ? (
                                    <StatusBadge
                                        tone="destructive"
                                        label={`还有 ${row.issues.length} 项需调整`}
                                    />
                                ) : null}
                                <p className="text-xs text-muted-foreground">
                                    仅更新草稿，整单预览后统一提交
                                </p>
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
                                                    void finish(
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
                                                    void finish(
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
                                                void finish(
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
