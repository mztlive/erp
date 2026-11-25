"use client"

import { useState } from "react"
import { ChevronLeftIcon, ChevronRightIcon, SearchIcon } from "lucide-react"

import { OptionCombobox } from "@/components/business"
import { clearApiFieldError } from "@/components/form/api-errors"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { regionFullName, useRegionIndex } from "@/features/regions"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compactFixed, compareDecimal } from "@/lib/fixed-decimal"
import type { DeliveryCoverageHint as CoverageCode } from "../api/delivery-coverage"
import {
    inquiryDeliveryQuantity,
    sameInquiryAddress,
    type InquiryDeliveryOption,
} from "../api/inquiry-basis"
import type { SalesOrderCreateFormApi } from "../lib/sales-order-create-form-types"
import type {
    SalesDeliveryAllocation,
    SalesOrderDraftLineInput,
} from "../types"
import { DeliveryCoverageHint } from "./delivery-coverage-hint"

function quantityIssue(value: string, maxQuantity: string | undefined) {
    try {
        if (compareDecimal(value, "0", 6) <= 0) return "请填写大于 0 的数量"
        if (maxQuantity && compareDecimal(value, maxQuantity, 6) > 0)
            return "超过该地址的可转数量"
    } catch {
        return "请填写有效数量，最多 6 位小数"
    }
}

type InquiryDeliveryAddressProps = {
    form: SalesOrderCreateFormApi
    line: SalesOrderDraftLineInput
    displayUnit: string
    options: readonly InquiryDeliveryOption[]
    excludedQuantities: Map<string, string>
    coverageHint: (sku: string, regionCode: string) => CoverageCode
}

export function InquiryDeliveryPane({
    lines,
    onSelectLine,
    ...props
}: InquiryDeliveryAddressProps & {
    lines: readonly SalesOrderDraftLineInput[]
    onSelectLine: (rowKey: string) => void
}) {
    const { line } = props
    const lineIndex = lines.findIndex((item) => item.rowKey === line.rowKey)
    const previousLine = lines[lineIndex - 1]
    const nextLine = lines[lineIndex + 1]

    return (
        <div className="flex min-h-0 flex-1 flex-col gap-3 px-6 py-4">
            <div className="flex shrink-0 flex-wrap items-center gap-2">
                <div className="min-w-48 flex-1">
                    <OptionCombobox
                        id="sales-delivery-workspace-product-jump"
                        aria-label="切换商品"
                        allowClear={false}
                        options={lines.map((item) => ({
                            value: item.rowKey,
                            label: item.name || "未命名商品",
                        }))}
                        value={line.rowKey}
                        onValueChange={(value) => {
                            if (value) onSelectLine(value)
                        }}
                    />
                </div>
                <div className="flex shrink-0 items-center gap-2">
                    <Button
                        id="sales-delivery-workspace-prev-product"
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        aria-label="上一个商品"
                        disabled={!previousLine}
                        onClick={() =>
                            previousLine && onSelectLine(previousLine.rowKey)
                        }
                    >
                        <ChevronLeftIcon aria-hidden="true" />
                    </Button>
                    <span className="num text-xs text-muted-foreground">
                        {lineIndex + 1} / {lines.length}
                    </span>
                    <Button
                        id="sales-delivery-workspace-next-product"
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        aria-label="下一个商品"
                        disabled={!nextLine}
                        onClick={() =>
                            nextLine && onSelectLine(nextLine.rowKey)
                        }
                    >
                        <ChevronRightIcon aria-hidden="true" />
                    </Button>
                </div>
            </div>

            <InquiryDeliveryAddresses key={line.rowKey} {...props} />
        </div>
    )
}

function InquiryDeliveryAddresses({
    form,
    line,
    displayUnit,
    options,
    excludedQuantities,
    coverageHint,
}: InquiryDeliveryAddressProps) {
    const [search, setSearch] = useState("")
    const [filter, setFilter] = useState<"all" | "excluded" | "issues">("all")
    const { data: regions } = useRegionIndex()
    const prefix = `inquiry-sales-delivery-${toAutomationIdSegment(line.rowKey)}`
    const allocations = line.deliveryAllocations ?? []
    const rows = options.map((option) => {
        const current = allocations.find((allocation) =>
            sameInquiryAddress(option.delivery, allocation),
        )
        const region =
            option.delivery.region_name ||
            (regions
                ? regionFullName(regions, option.delivery.region_code)
                : "")
        return {
            ...option,
            current,
            address: `${region}${option.delivery.address}`,
            issue:
                current && quantityIssue(current.quantity, option.maxQuantity),
        }
    })
    const unmatchedCount = allocations.filter(
        (allocation) =>
            !options.some((option) =>
                sameInquiryAddress(option.delivery, allocation),
            ),
    ).length
    const selectedCount = rows.filter((row) => row.current).length
    const issueCount = rows.filter((row) => row.issue).length
    const needle = search.trim().toLowerCase()
    const visible = rows.filter((row) => {
        if (filter === "excluded" && row.current) return false
        if (filter === "issues" && !row.issue) return false
        return [row.delivery.recipient, row.delivery.phone, row.address].some(
            (value) => value.toLowerCase().includes(needle),
        )
    })

    const update = (
        edit: (
            current: readonly SalesDeliveryAllocation[],
        ) => SalesDeliveryAllocation[],
    ) => {
        const items = form.getFieldValue("lineItems")
        const index = items.findIndex((item) => item.rowKey === line.rowKey)
        const current = items[index]
        if (!current) return
        const next = edit(current.deliveryAllocations ?? [])
        const quantity = options.length
            ? inquiryDeliveryQuantity(next)
            : current.quantity
        const field = `lineItems[${index}]` as const
        clearApiFieldError(form, `${field}.quantity`)
        clearApiFieldError(form, `${field}.deliveryAllocations`)
        form.setFieldValue(field, {
            ...current,
            quantity,
            deliveryAllocations: next,
        })
    }

    const setSelected = (
        targets: readonly InquiryDeliveryOption[],
        checked: boolean,
    ) => {
        update((current) => {
            if (checked) {
                const added = targets
                    .filter(
                        (target) =>
                            !current.some((row) =>
                                sameInquiryAddress(target.delivery, row),
                            ),
                    )
                    .map(({ id, delivery }) => {
                        const previous = excludedQuantities.get(
                            JSON.stringify([line.rowKey, id]),
                        )
                        return {
                            ...delivery,
                            quantity:
                                previous ??
                                (quantityIssue(delivery.quantity, undefined)
                                    ? ""
                                    : compactFixed(delivery.quantity)),
                        }
                    })
                return [...current, ...added]
            }
            for (const target of targets) {
                const allocation = current.find((row) =>
                    sameInquiryAddress(target.delivery, row),
                )
                if (allocation)
                    excludedQuantities.set(
                        JSON.stringify([line.rowKey, target.id]),
                        allocation.quantity,
                    )
            }
            return current.filter(
                (row) =>
                    !targets.some((target) =>
                        sameInquiryAddress(target.delivery, row),
                    ),
            )
        })
    }

    return (
        <>
            {options.length > 0 ? (
                <div className="flex shrink-0 flex-wrap items-center gap-x-4 gap-y-1 text-body-compact">
                    <p className="num font-medium" aria-live="polite">
                        本次转入 {selectedCount} 个地址 ·{" "}
                        {line.quantity || "数量待核对"}{" "}
                        {line.quantity ? displayUnit : ""}
                    </p>
                    {selectedCount === 0 ? (
                        <p className="text-xs text-destructive">
                            请至少勾选一个地址
                        </p>
                    ) : null}
                </div>
            ) : null}

            {unmatchedCount > 0 ? (
                <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 rounded-md bg-warning-soft px-3 py-2 text-xs text-warning-soft-foreground">
                    <p role="alert">
                        有 {unmatchedCount} 个已分配地址超出当前询价的可转范围。
                    </p>
                    <Button
                        id={`${prefix}-remove-unavailable`}
                        type="button"
                        variant="outline"
                        size="xs"
                        onClick={() =>
                            update((current) =>
                                current.filter((row) =>
                                    options.some((option) =>
                                        sameInquiryAddress(
                                            option.delivery,
                                            row,
                                        ),
                                    ),
                                ),
                            )
                        }
                    >
                        移除不可转地址
                    </Button>
                </div>
            ) : null}

            {options.length === 0 ? (
                <p className="py-8 text-center text-body-compact text-muted-foreground">
                    当前商品没有可转入的收货地址。
                </p>
            ) : (
                <>
                    <div className="flex shrink-0 flex-wrap items-center gap-2">
                        <div className="relative min-w-48 flex-1">
                            <SearchIcon
                                aria-hidden="true"
                                className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                            />
                            <Input
                                id={`${prefix}-search`}
                                size="sm"
                                aria-label="按收货人、电话或地址搜索"
                                placeholder="搜索收货人、电话或地址"
                                value={search}
                                onChange={(event) =>
                                    setSearch(event.target.value)
                                }
                                className="pl-8"
                            />
                        </div>
                        <Button
                            id={`${prefix}-filter-excluded`}
                            type="button"
                            variant={
                                filter === "excluded" ? "secondary" : "outline"
                            }
                            size="sm"
                            aria-pressed={filter === "excluded"}
                            onClick={() =>
                                setFilter(
                                    filter === "excluded" ? "all" : "excluded",
                                )
                            }
                        >
                            只看暂不转入
                        </Button>
                        <Button
                            id={`${prefix}-filter-issues`}
                            type="button"
                            variant={
                                filter === "issues" ? "secondary" : "outline"
                            }
                            size="sm"
                            aria-pressed={filter === "issues"}
                            onClick={() =>
                                setFilter(
                                    filter === "issues" ? "all" : "issues",
                                )
                            }
                        >
                            只看有问题（{issueCount}）
                        </Button>
                    </div>

                    <div className="flex shrink-0 flex-wrap items-center gap-2">
                        <Button
                            id={`${prefix}-include-visible`}
                            type="button"
                            variant="outline"
                            size="xs"
                            disabled={!visible.some((row) => !row.current)}
                            onClick={() => setSelected(visible, true)}
                        >
                            勾选筛选结果
                        </Button>
                        <Button
                            id={`${prefix}-exclude-visible`}
                            type="button"
                            variant="outline"
                            size="xs"
                            disabled={!visible.some((row) => row.current)}
                            onClick={() => setSelected(visible, false)}
                        >
                            取消勾选筛选结果
                        </Button>
                        <span className="num ml-auto text-xs text-muted-foreground">
                            显示 {visible.length} / {rows.length} 个地址
                        </span>
                    </div>

                    <div
                        id={`${prefix}-list`}
                        role="region"
                        aria-label="询价收货地址列表"
                        className="min-h-0 flex-1 overflow-auto overscroll-contain rounded-lg border border-border/75 [&>[data-slot=table-container]]:overflow-visible"
                    >
                        <Table className="table-fixed">
                            <TableHeader className="sticky top-0 z-10">
                                <TableRow>
                                    <TableHead
                                        className="w-12"
                                        data-align="center"
                                    >
                                        转入
                                    </TableHead>
                                    <TableHead>收货信息</TableHead>
                                    <TableHead
                                        className="w-32"
                                        data-align="end"
                                    >
                                        本次转入
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {visible.map((row) => {
                                    const id = `${prefix}-${toAutomationIdSegment(row.id)}`
                                    const descriptionId = `${id}-quantity-description`
                                    const maxLabel = row.maxQuantity
                                        ? `最多可转 ${compactFixed(row.maxQuantity)} ${displayUnit}`.trim()
                                        : undefined
                                    const coverage = row.current
                                        ? coverageHint(
                                              line.sku,
                                              row.delivery.region_code,
                                          )
                                        : "NONE"
                                    return (
                                        <TableRow key={row.id}>
                                            <TableCell data-align="center">
                                                <NativeCheckbox
                                                    id={`${id}-include`}
                                                    aria-label={`转入 ${row.delivery.recipient} 的地址`}
                                                    checked={Boolean(
                                                        row.current,
                                                    )}
                                                    onCheckedChange={(
                                                        checked,
                                                    ) =>
                                                        setSelected(
                                                            [row],
                                                            checked,
                                                        )
                                                    }
                                                />
                                            </TableCell>
                                            <TableCell className="whitespace-normal">
                                                <p className="break-words font-medium">
                                                    {row.delivery.recipient}
                                                    <span className="num ml-2 font-normal text-muted-foreground">
                                                        {row.delivery.phone}
                                                    </span>
                                                </p>
                                                <p className="break-words text-xs text-muted-foreground">
                                                    {row.address}
                                                </p>
                                                {coverage !== "NONE" ? (
                                                    <DeliveryCoverageHint
                                                        id={`${id}-coverage`}
                                                        hint={coverage}
                                                    />
                                                ) : null}
                                            </TableCell>
                                            <TableCell
                                                data-align="end"
                                                className="whitespace-normal"
                                            >
                                                {row.current ? (
                                                    <div className="flex items-center justify-end gap-1">
                                                        <Input
                                                            id={`${id}-quantity`}
                                                            size="sm"
                                                            aria-label={`${row.delivery.recipient} 的转入数量`}
                                                            aria-invalid={
                                                                Boolean(
                                                                    row.issue,
                                                                ) || undefined
                                                            }
                                                            aria-describedby={
                                                                row.issue ||
                                                                maxLabel
                                                                    ? descriptionId
                                                                    : undefined
                                                            }
                                                            type="text"
                                                            inputMode="decimal"
                                                            value={
                                                                row.current
                                                                    .quantity
                                                            }
                                                            className="num w-16 text-right"
                                                            onChange={(
                                                                event,
                                                            ) => {
                                                                const quantity =
                                                                    event.target
                                                                        .value
                                                                update(
                                                                    (current) =>
                                                                        current.map(
                                                                            (
                                                                                allocation,
                                                                            ) =>
                                                                                sameInquiryAddress(
                                                                                    row.delivery,
                                                                                    allocation,
                                                                                )
                                                                                    ? {
                                                                                          ...allocation,
                                                                                          quantity,
                                                                                      }
                                                                                    : allocation,
                                                                        ),
                                                                )
                                                            }}
                                                        />
                                                        {displayUnit ? (
                                                            <span className="text-xs text-muted-foreground">
                                                                {displayUnit}
                                                            </span>
                                                        ) : null}
                                                    </div>
                                                ) : (
                                                    <p className="text-xs text-muted-foreground">
                                                        暂不转入
                                                    </p>
                                                )}
                                                {row.issue || maxLabel ? (
                                                    <div
                                                        id={descriptionId}
                                                        className="mt-1 text-xs text-muted-foreground"
                                                    >
                                                        {maxLabel}
                                                        {row.issue ? (
                                                            <p className="text-destructive">
                                                                {row.issue}
                                                            </p>
                                                        ) : null}
                                                    </div>
                                                ) : null}
                                            </TableCell>
                                        </TableRow>
                                    )
                                })}
                                {visible.length === 0 ? (
                                    <TableRow>
                                        <TableCell
                                            colSpan={3}
                                            className="py-8 text-center text-muted-foreground"
                                        >
                                            没有符合条件的地址
                                        </TableCell>
                                    </TableRow>
                                ) : null}
                            </TableBody>
                        </Table>
                    </div>
                </>
            )}
        </>
    )
}
