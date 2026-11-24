"use client"

import { useEffect, useState } from "react"
import {
    CheckCheckIcon,
    CircleAlertIcon,
    EraserIcon,
    SearchIcon,
    SplitIcon,
    XIcon,
} from "lucide-react"
import { QuantityValue } from "@/components/business"
import { toFieldErrors } from "@/components/form"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { FieldError } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { fullDeliveryAddress } from "@/lib/delivery-address"
import {
    compactFixed,
    compareDecimal,
    subtractFixed,
} from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { PurchaseOrderCreateFormApi } from "../lib/purchase-order-create-form-types"
import {
    sourcingQuantityStep,
    sourcingUnitQuantityError,
} from "../lib/sourcing-quantity"
import {
    deliveryKey,
    remainingDeliveryAllocations,
} from "../lib/sourcing/delivery"
import {
    availableDeliveryRows,
    deliveryQuantity,
    selectedDeliveryRows,
} from "../lib/sourcing/delivery-editor"
import {
    addressDeliverable,
    addressDeliveryTerm,
    deliveryTermLabel,
} from "../lib/sourcing/delivery-terms"
import { findSourcingOption } from "../lib/sourcing/options"
import type {
    SourcingLineInput,
    SourcingProductLine,
} from "../lib/sourcing/types"
import type { PurchaseDeliveryAllocation } from "../types"

type AddressFilter = "all" | "assigned" | "unassigned" | "issues"

/** 地址以收件资料作为稳定业务键，中文资料也保留在自动化 ID 中。 */
function addressId(row: PurchaseDeliveryAllocation): string {
    return toAutomationIdSegment(encodeURIComponent(deliveryKey(row)))
}

function isPositiveQuantity(raw: string): boolean {
    try {
        return compareDecimal(compactFixed(raw.trim() || "0"), "0", 6) > 0
    } catch {
        return false
    }
}

/**
 * 一体化多地址分配矩阵：直接在弹窗正文展示全部剩余地址，彻底消除弹窗内嵌套 Popover；
 * 勾选即按可分配量填入，输 0 仅取消勾选绝不突然删行，并支持一键填满、一键清空和剩余地址拆给新来源。
 */
export function SourcingAddressEditor({
    form,
    product,
    line,
    index,
    onSplitResidual,
}: {
    form: PurchaseOrderCreateFormApi
    product: SourcingProductLine
    line: SourcingLineInput
    index: number
    /** 存在尚未分配的剩余地址时，支持一键拆出新来源并承接剩余地址。 */
    onSplitResidual?: () => void
}) {
    const [query, setQuery] = useState("")
    const [filter, setFilter] = useState<AddressFilter>("all")
    const id = `sourcing-delivery-${toAutomationIdSegment(line.rowKey)}`
    const option = findSourcingOption(product, line.basisId)

    // 打开来源或切换方案时冻结默认分配，并清理旧草稿中的零数量行。
    useEffect(() => {
        const current = form.state.values.lines[index]
        if (!current || current.rowKey !== line.rowKey) return
        const rows = selectedDeliveryRows(product, current)
        const quantity = deliveryQuantity(rows)
        if (
            quantity !== current.quantity ||
            JSON.stringify(rows) !== JSON.stringify(current.deliveryAllocations)
        ) {
            form.setFieldValue(`lines[${index}]`, {
                ...current,
                deliveryAllocations: rows,
                quantity,
            })
        }
    }, [form, index, line.rowKey, line.basisId, product])

    const updateAssignedRows = (rows: PurchaseDeliveryAllocation[]) => {
        const current = form.state.values.lines[index]
        if (!current || current.rowKey !== line.rowKey) return
        form.setFieldValue(`lines[${index}]`, {
            ...current,
            deliveryAllocations: rows,
            quantity: deliveryQuantity(rows),
        })
    }

    return (
        <form.AppField name={`lines[${index}]`}>
            {(field) => {
                const assignedRows: PurchaseDeliveryAllocation[] =
                    field.state.value.deliveryAllocations ??
                    selectedDeliveryRows(product, line)
                const assignedByKey = new Map(
                    assignedRows.map((row) => [deliveryKey(row), row]),
                )

                const baseRemaining = remainingDeliveryAllocations(
                    product,
                ).filter((row) => isPositiveQuantity(row.quantity))
                const available = availableDeliveryRows(
                    product,
                    line,
                    form.state.values.lines,
                )
                const capacities = new Map(
                    available.map((row) => [deliveryKey(row), row.quantity]),
                )

                // 合并展示全部剩余地址 + 任何不在 baseRemaining 中的异常旧草稿地址
                const baseKeys = new Set(baseRemaining.map(deliveryKey))
                const allAddresses: PurchaseDeliveryAllocation[] = [
                    ...baseRemaining,
                    ...assignedRows.filter(
                        (row) => !baseKeys.has(deliveryKey(row)),
                    ),
                ]

                const entries = allAddresses.map((baseRow) => {
                    const key = deliveryKey(baseRow)
                    const assigned = assignedByKey.get(key)
                    const currentQty = assigned?.quantity ?? ""
                    const hasAssigned =
                        assigned !== undefined &&
                        (currentQty.trim() === "" ||
                            isPositiveQuantity(currentQty))
                    const positive = isPositiveQuantity(currentQty)
                    const cap = capacities.get(key) ?? "0"
                    const capPositive = isPositiveQuantity(cap)
                    const term = addressDeliveryTerm(
                        option,
                        baseRow.region_code,
                    )
                    const deliverable = addressDeliverable(
                        option,
                        baseRow.region_code,
                    )

                    // 计算本地址扣除本源后的仍待分配量
                    let stillUnassigned = cap
                    try {
                        const qty = compactFixed(currentQty.trim() || "0")
                        const diff = subtractFixed(cap, qty, {
                            maxScale: 6,
                            outputScale: 6,
                        })
                        stillUnassigned =
                            compareDecimal(diff, "0", 6) > 0
                                ? compactFixed(diff)
                                : "0"
                    } catch {
                        stillUnassigned = cap
                    }

                    let error: string | undefined
                    if (assigned && currentQty.trim() !== "") {
                        const trimmed = compactFixed(currentQty.trim())
                        if (trimmed !== "0") {
                            error = sourcingUnitQuantityError(
                                trimmed,
                                product.quantityScale,
                            )
                            if (!error && compareDecimal(trimmed, cap, 6) > 0) {
                                error = `超过可分配数量 ${cap} ${product.unit}，请减少数量或调整其他来源`
                            }
                            if (!deliverable) {
                                error =
                                    "该来源不配送此地址，请取消勾选或拆给其他来源"
                            }
                        }
                    }

                    return {
                        baseRow,
                        key,
                        currentQty,
                        hasAssigned,
                        positive,
                        cap,
                        capPositive,
                        stillUnassigned,
                        term,
                        deliverable,
                        error,
                        fullAddress: fullDeliveryAddress(baseRow),
                    }
                })

                const assignedCount = entries.filter((e) => e.positive).length
                const unassignedCount = entries.filter((e) =>
                    isPositiveQuantity(e.stillUnassigned),
                ).length
                const issueCount = entries.filter(
                    (e) =>
                        Boolean(e.error) ||
                        e.term === "NOT_DELIVERABLE" ||
                        e.term === "FREIGHT_EXTRA",
                ).length
                const deliverableFillableCount = entries.filter(
                    (e) =>
                        e.deliverable &&
                        e.capPositive &&
                        (!e.positive ||
                            compareDecimal(
                                compactFixed(e.currentQty.trim() || "0"),
                                e.cap,
                                6,
                            ) < 0),
                ).length
                const total = deliveryQuantity(assignedRows)
                const invalid =
                    field.state.meta.isTouched && !field.state.meta.isValid

                const keyword = query.trim().toLocaleLowerCase()
                const visibleEntries = entries.filter((entry) => {
                    if (filter === "assigned" && !entry.hasAssigned)
                        return false
                    if (
                        filter === "unassigned" &&
                        !isPositiveQuantity(entry.stillUnassigned)
                    )
                        return false
                    if (
                        filter === "issues" &&
                        !entry.error &&
                        entry.term !== "NOT_DELIVERABLE" &&
                        entry.term !== "FREIGHT_EXTRA"
                    )
                        return false
                    if (!keyword) return true
                    return `${entry.baseRow.recipient} ${entry.baseRow.phone} ${entry.fullAddress}`
                        .toLocaleLowerCase()
                        .includes(keyword)
                })

                const setAddressQuantity = (
                    baseRow: PurchaseDeliveryAllocation,
                    nextValue: string,
                    removeIfZero = false,
                ) => {
                    const key = deliveryKey(baseRow)
                    const trimmed = nextValue.trim()
                    const exists = assignedByKey.has(key)

                    if (
                        removeIfZero &&
                        (trimmed === "" ||
                            (trimmed !== "" &&
                                (() => {
                                    try {
                                        return (
                                            compareDecimal(
                                                compactFixed(trimmed),
                                                "0",
                                                6,
                                            ) === 0
                                        )
                                    } catch {
                                        return false
                                    }
                                })()))
                    ) {
                        updateAssignedRows(
                            assignedRows.filter(
                                (item) => deliveryKey(item) !== key,
                            ),
                        )
                        return
                    }

                    if (!exists) {
                        updateAssignedRows([
                            ...assignedRows,
                            { ...baseRow, quantity: nextValue },
                        ])
                        return
                    }

                    updateAssignedRows(
                        assignedRows.map((item) =>
                            deliveryKey(item) === key
                                ? { ...item, quantity: nextValue }
                                : item,
                        ),
                    )
                }

                const fillAllDeliverable = () => {
                    const nextRows: PurchaseDeliveryAllocation[] = []
                    for (const entry of entries) {
                        if (entry.deliverable && entry.capPositive) {
                            nextRows.push({
                                ...entry.baseRow,
                                quantity: entry.cap,
                            })
                        }
                    }
                    updateAssignedRows(nextRows)
                    field.handleBlur()
                }

                const clearAll = () => {
                    updateAssignedRows([])
                    field.handleBlur()
                }

                return (
                    <section
                        className="border-t border-border/60"
                        aria-label="本来源交付地址"
                    >
                        <div className="flex flex-wrap items-center justify-between gap-2.5 px-4 pt-3.5 pb-2.5 @min-[560px]/sourcing:px-6">
                            <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                                <h4 className="text-body-compact font-semibold text-foreground">
                                    交付地址分配
                                </h4>
                                <span className="num rounded-sm bg-muted px-1.5 py-0.5 text-tiny font-medium text-muted-foreground">
                                    已分 {assignedCount} / {entries.length}
                                </span>
                                <span
                                    aria-hidden="true"
                                    className="text-muted-foreground/40"
                                >
                                    ·
                                </span>
                                <p
                                    className="text-xs text-muted-foreground"
                                    aria-live="polite"
                                >
                                    本源合计{" "}
                                    <QuantityValue
                                        className="font-semibold text-foreground"
                                        value={total || "0"}
                                        unit={product.unit}
                                    />
                                </p>
                                {unassignedCount > 0 ? (
                                    <span className="text-xs text-warning-soft-foreground">
                                        · {unassignedCount} 处有余量
                                    </span>
                                ) : null}
                            </div>

                            <div className="flex flex-wrap items-center gap-1.5">
                                <Button
                                    id={`${id}-fill-all`}
                                    type="button"
                                    variant="outline"
                                    size="xs"
                                    disabled={deliverableFillableCount === 0}
                                    onClick={fillAllDeliverable}
                                >
                                    <CheckCheckIcon aria-hidden="true" />
                                    填满可配送
                                </Button>
                                <Button
                                    id={`${id}-clear-all`}
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    disabled={assignedRows.length === 0}
                                    onClick={clearAll}
                                >
                                    <EraserIcon aria-hidden="true" />
                                    清空本源
                                </Button>
                                {onSplitResidual && unassignedCount > 0 ? (
                                    <Button
                                        id={`${id}-split-residual`}
                                        type="button"
                                        variant="secondary"
                                        size="xs"
                                        onClick={onSplitResidual}
                                    >
                                        <SplitIcon aria-hidden="true" />
                                        剩余 {unassignedCount} 处拆给新来源
                                    </Button>
                                ) : null}
                            </div>
                        </div>

                        <div className="flex flex-wrap items-center justify-between gap-2 border-y border-border/60 bg-surface-sunken px-4 py-2 @min-[560px]/sourcing:px-6">
                            <div
                                role="group"
                                aria-label="筛选交付地址"
                                className="flex flex-wrap items-center gap-1"
                            >
                                {(
                                    [
                                        ["all", `全部 ${entries.length}`],
                                        [
                                            "assigned",
                                            `本源已分 ${assignedCount}`,
                                        ],
                                        [
                                            "unassigned",
                                            `待分配 ${unassignedCount}`,
                                        ],
                                        ...(issueCount > 0
                                            ? ([
                                                  [
                                                      "issues",
                                                      `运费/异常 ${issueCount}`,
                                                  ],
                                              ] as const)
                                            : []),
                                    ] as const
                                ).map(([value, label]) => (
                                    <Button
                                        key={value}
                                        id={`${id}-filter-${value}`}
                                        type="button"
                                        size="xs"
                                        variant={
                                            filter === value
                                                ? "secondary"
                                                : "ghost"
                                        }
                                        aria-pressed={filter === value}
                                        onClick={() => setFilter(value)}
                                    >
                                        {label}
                                    </Button>
                                ))}
                            </div>

                            {entries.length >= 5 || keyword.length > 0 ? (
                                <div className="relative w-48">
                                    <SearchIcon
                                        aria-hidden="true"
                                        className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                                    />
                                    <Input
                                        id={`${id}-search`}
                                        aria-label="筛选交付地址"
                                        placeholder="搜收件人、电话或地区"
                                        className="h-control-sm bg-card pr-7 pl-8 text-xs"
                                        value={query}
                                        onChange={(event) =>
                                            setQuery(event.target.value)
                                        }
                                    />
                                    {query ? (
                                        <Button
                                            id={`${id}-search-clear`}
                                            type="button"
                                            variant="ghost"
                                            size="icon-xs"
                                            aria-label="清除地址筛选"
                                            className="absolute top-1/2 right-0.5 -translate-y-1/2 text-muted-foreground"
                                            onClick={() => setQuery("")}
                                        >
                                            <XIcon />
                                        </Button>
                                    ) : null}
                                </div>
                            ) : null}
                        </div>

                        <div className="sticky top-0 z-10 hidden grid-cols-[1.5rem_minmax(0,1fr)_5.5rem_9rem] items-center gap-3 border-b border-border/60 bg-table-header px-4 py-1.5 text-xs font-medium text-muted-foreground @min-[540px]/sourcing:grid @min-[560px]/sourcing:px-6">
                            <span className="sr-only">分配</span>
                            <span>收件人 / 交付地址</span>
                            <span className="text-right">本源可分</span>
                            <span className="text-right">本源分配量</span>
                        </div>

                        {visibleEntries.length === 0 ? (
                            <div className="flex items-center justify-center gap-2 border-b border-border/60 px-6 py-7 text-xs text-muted-foreground">
                                <span>当前筛选条件下没有交付地址</span>
                                {filter !== "all" || query ? (
                                    <Button
                                        id={`${id}-search-reset`}
                                        type="button"
                                        variant="link"
                                        size="xs"
                                        onClick={() => {
                                            setFilter("all")
                                            setQuery("")
                                        }}
                                    >
                                        查看全部
                                    </Button>
                                ) : null}
                            </div>
                        ) : (
                            <div className="divide-y divide-grid border-b border-border/60">
                                {visibleEntries.map((entry) => {
                                    const rowId = `${id}-${addressId(entry.baseRow)}`
                                    const disabledForThisSource =
                                        !entry.deliverable && !entry.positive
                                    const canQuickFill =
                                        entry.deliverable &&
                                        entry.capPositive &&
                                        (!entry.positive ||
                                            compareDecimal(
                                                compactFixed(
                                                    entry.currentQty.trim() ||
                                                        "0",
                                                ),
                                                entry.cap,
                                                6,
                                            ) < 0)

                                    return (
                                        <div
                                            key={entry.key}
                                            className={cn(
                                                "grid grid-cols-[1.5rem_minmax(0,1fr)] items-center gap-x-3 gap-y-1.5 px-4 py-2 transition-colors hover:bg-row-hover @min-[540px]/sourcing:grid-cols-[1.5rem_minmax(0,1fr)_5.5rem_9rem] @min-[560px]/sourcing:px-6",
                                                entry.error &&
                                                    "bg-destructive-soft/35 hover:bg-destructive-soft/50",
                                                !entry.positive &&
                                                    !entry.error &&
                                                    "bg-surface-sunken/35",
                                            )}
                                        >
                                            <Checkbox
                                                id={`${rowId}-toggle`}
                                                checked={entry.positive}
                                                disabled={
                                                    disabledForThisSource ||
                                                    (!entry.positive &&
                                                        !entry.capPositive)
                                                }
                                                aria-label={`为 ${entry.baseRow.recipient} 分配数量`}
                                                onCheckedChange={(checked) => {
                                                    if (checked === true) {
                                                        setAddressQuantity(
                                                            entry.baseRow,
                                                            entry.cap,
                                                        )
                                                    } else {
                                                        setAddressQuantity(
                                                            entry.baseRow,
                                                            "0",
                                                            true,
                                                        )
                                                    }
                                                    field.handleBlur()
                                                }}
                                            />

                                            <label
                                                htmlFor={`${rowId}-quantity`}
                                                className="min-w-0 cursor-pointer"
                                            >
                                                <span className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
                                                    <span
                                                        className={cn(
                                                            "text-body-compact font-medium",
                                                            entry.positive
                                                                ? "text-foreground"
                                                                : "text-muted-foreground",
                                                        )}
                                                    >
                                                        {
                                                            entry.baseRow
                                                                .recipient
                                                        }
                                                    </span>
                                                    <span className="num text-xs text-muted-foreground">
                                                        {entry.baseRow.phone}
                                                    </span>
                                                    {option &&
                                                    entry.term ===
                                                        "FREIGHT_EXTRA" ? (
                                                        <Badge variant="warning">
                                                            {deliveryTermLabel(
                                                                option,
                                                                entry.term,
                                                            )}
                                                        </Badge>
                                                    ) : null}
                                                    {option &&
                                                    entry.term ===
                                                        "NOT_DELIVERABLE" ? (
                                                        <Badge variant="destructive">
                                                            {deliveryTermLabel(
                                                                option,
                                                                entry.term,
                                                            )}
                                                        </Badge>
                                                    ) : null}
                                                    {!entry.capPositive &&
                                                    !entry.positive ? (
                                                        <Badge variant="neutral">
                                                            他源已分完
                                                        </Badge>
                                                    ) : null}
                                                </span>
                                                <span className="mt-0.5 block break-words text-xs text-muted-foreground">
                                                    {entry.fullAddress}
                                                </span>
                                            </label>

                                            <div className="col-start-2 flex items-baseline gap-1 text-xs text-muted-foreground @min-[540px]/sourcing:col-start-auto @min-[540px]/sourcing:justify-end">
                                                <span className="@min-[540px]/sourcing:hidden">
                                                    可分
                                                </span>
                                                <QuantityValue
                                                    value={entry.cap}
                                                    unit={product.unit}
                                                    className="text-xs [&_.num]:text-muted-foreground"
                                                />
                                            </div>

                                            <div className="col-start-2 flex items-center justify-end gap-1 @min-[540px]/sourcing:col-start-auto">
                                                {disabledForThisSource ? (
                                                    <span className="text-xs text-muted-foreground">
                                                        不可配送
                                                    </span>
                                                ) : (
                                                    <>
                                                        <Input
                                                            id={`${rowId}-quantity`}
                                                            className="num h-control-sm w-18 bg-card text-right"
                                                            type="number"
                                                            min="0"
                                                            max={entry.cap}
                                                            step={
                                                                sourcingQuantityStep(
                                                                    product.quantityScale,
                                                                ) ?? "1"
                                                            }
                                                            inputMode="decimal"
                                                            placeholder="0"
                                                            aria-label={`${entry.baseRow.recipient}，${entry.fullAddress}，本次交付数量（${product.unit}）`}
                                                            aria-invalid={Boolean(
                                                                entry.error,
                                                            )}
                                                            aria-describedby={
                                                                entry.error
                                                                    ? `${rowId}-error`
                                                                    : undefined
                                                            }
                                                            value={
                                                                entry.currentQty
                                                            }
                                                            onChange={(event) =>
                                                                setAddressQuantity(
                                                                    entry.baseRow,
                                                                    event.target
                                                                        .value,
                                                                )
                                                            }
                                                            onBlur={() => {
                                                                const trimmed =
                                                                    entry.currentQty.trim()
                                                                if (
                                                                    trimmed ===
                                                                    ""
                                                                ) {
                                                                    setAddressQuantity(
                                                                        entry.baseRow,
                                                                        "0",
                                                                        true,
                                                                    )
                                                                } else {
                                                                    try {
                                                                        const normalized =
                                                                            compactFixed(
                                                                                trimmed,
                                                                            )
                                                                        setAddressQuantity(
                                                                            entry.baseRow,
                                                                            normalized,
                                                                            compareDecimal(
                                                                                normalized,
                                                                                "0",
                                                                                6,
                                                                            ) ===
                                                                                0,
                                                                        )
                                                                    } catch {
                                                                        // 保留非法文本交由校验提示
                                                                    }
                                                                }
                                                                field.handleBlur()
                                                            }}
                                                        />
                                                        <span className="min-w-4 shrink-0 text-xs text-muted-foreground">
                                                            {product.unit}
                                                        </span>
                                                        {canQuickFill ? (
                                                            <Button
                                                                id={`${rowId}-fill`}
                                                                type="button"
                                                                variant="ghost"
                                                                size="xs"
                                                                className="h-6 px-1.5 text-tiny text-primary hover:text-primary"
                                                                title={`填满可分配量 ${entry.cap} ${product.unit}`}
                                                                onClick={() => {
                                                                    setAddressQuantity(
                                                                        entry.baseRow,
                                                                        entry.cap,
                                                                    )
                                                                    field.handleBlur()
                                                                }}
                                                            >
                                                                满
                                                            </Button>
                                                        ) : (
                                                            <span
                                                                aria-hidden="true"
                                                                className="w-7 shrink-0"
                                                            />
                                                        )}
                                                    </>
                                                )}
                                            </div>

                                            {entry.error ? (
                                                <p
                                                    id={`${rowId}-error`}
                                                    role="alert"
                                                    className="col-span-full flex items-center gap-1.5 pt-0.5 text-xs text-destructive"
                                                >
                                                    <CircleAlertIcon
                                                        aria-hidden="true"
                                                        className="size-3.5 shrink-0"
                                                    />
                                                    <span>{entry.error}</span>
                                                </p>
                                            ) : null}
                                        </div>
                                    )
                                })}
                            </div>
                        )}

                        {invalid ? (
                            <div className="px-4 py-2.5 @min-[560px]/sourcing:px-6">
                                <FieldError
                                    errors={toFieldErrors(
                                        field.state.meta.errors,
                                    )}
                                />
                            </div>
                        ) : null}
                    </section>
                )
            }}
        </form.AppField>
    )
}
