"use client"

import { useImperativeHandle, useState, type Ref } from "react"
import { ChevronLeftIcon, ChevronRightIcon, SearchIcon } from "lucide-react"
import { OptionCombobox, type ValidationIssue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { QUALITY_RESULT_OPTIONS } from "@/lib/business-options"
import { cn } from "@/lib/utils"
import { changeDispatch } from "../../lib/packing-allocation"
import { lineItemTitle } from "../../lib/readable-label"
import { withDerivedQualified } from "../../lib/validation"
import type {
    FulfillmentDraft,
    FulfillmentOperation,
    FulfillmentSourceLine,
    ReceiptDraftLine,
} from "../../types"
import {
    positiveQuantity,
    type fulfillmentBatchQuantities,
} from "./fulfillment-task-quantities"

const FILTER_THRESHOLD = 8
const PAGE_SIZE = 20
const QUALITY_LABELS: Readonly<Record<string, string>> = {
    PASSED: "合格",
    PARTIAL: "部分合格",
    REJECTED: "不合格",
    合格: "合格",
    部分合格: "部分合格",
    不合格: "不合格",
    待检: "待检",
}
const inlineInputClassName =
    "num min-w-0 border-0 bg-surface-sunken px-2 text-right shadow-none hover:bg-card focus-visible:bg-card"

type BatchRow = {
    id: string
    targetId: string
    name: string
    sku: string
    unit: string
    quantity: string
    limit?: string
    receipt?: ReceiptDraftLine
    lineIndex?: number
    address?: string
    addressKey?: string
    pendingAddress?: boolean
    allocation?: { destination: string; line: string; limit: string }
    editableKind: "dispatch" | "receipt" | "electronic" | "service" | "readonly"
}

function batchRows(
    operation: FulfillmentOperation,
    draft: FulfillmentDraft,
    prefix: string,
): BatchRow[] {
    function item(source: FulfillmentSourceLine | undefined, index: number) {
        return {
            name: lineItemTitle(source?.itemName, index),
            sku: source?.skuCode ?? "",
            unit: source?.unitName ?? "",
        }
    }

    if (draft.type === "RECEIPT") {
        return draft.lines.map((line, index) => {
            const source = operation.lines.find(
                (entry) =>
                    entry.purchaseRevisionLineId ===
                    line.purchaseRevisionLineId,
            )
            const id = `fulfillment-operations-receipt-form-received-quantity-${toAutomationIdSegment(line.purchaseRevisionLineId)}`
            return {
                ...item(source, index),
                id,
                targetId: id,
                quantity: line.receivedQuantity,
                limit: source?.remainingQuantity,
                receipt: line,
                lineIndex: index,
                editableKind: "receipt",
            }
        })
    }

    if (draft.type === "WAREHOUSE_SHIP" || draft.type === "SUPPLIER_DIRECT") {
        const packing = draft.packing
        if (!packing || !draft.addressSnapshot) return []
        return draft.addressSnapshot.destinations.flatMap((destination) => {
            const items =
                packing.dispatch.find(
                    (row) => row.destination_key === destination.key,
                )?.items ?? []
            return items.map((line, index) => {
                const source = operation.lines.find(
                    (entry) => entry.lineId === line.delivery_line_id,
                )
                const suffix = `${toAutomationIdSegment(destination.key)}-${toAutomationIdSegment(line.delivery_line_id)}-quantity`
                const limit = destination.items.find(
                    (entry) => entry.delivery_line_id === line.delivery_line_id,
                )?.quantity
                const editable = !destination.pending_address && limit != null
                return {
                    ...item(source, index),
                    id: `${prefix}-${suffix}`,
                    targetId: `fulfillment-packing-${suffix}`,
                    quantity: line.quantity,
                    limit,
                    addressKey: destination.key,
                    pendingAddress: destination.pending_address,
                    address: destination.pending_address
                        ? "待补收货地址"
                        : [destination.recipient, destination.address]
                              .filter(Boolean)
                              .join(" · "),
                    allocation: editable
                        ? {
                              destination: destination.key,
                              line: line.delivery_line_id,
                              limit,
                          }
                        : undefined,
                    editableKind: editable ? "dispatch" : "readonly",
                }
            })
        })
    }

    const kind = draft.type === "ELECTRONIC" ? "electronic" : "service"
    return draft.lines.map((line, index) => {
        const source = operation.lines.find(
            (entry) => entry.salesOrderLineId === line.salesOrderLineId,
        )
        const id = `fulfillment-operations-${kind}-form-quantity-${toAutomationIdSegment(line.salesOrderLineId)}`
        return {
            ...item(source, index),
            id,
            targetId: id,
            quantity: line.quantity,
            limit: source?.remainingQuantity,
            lineIndex: index,
            editableKind: kind,
        }
    })
}

function receiptFieldIds(line: ReceiptDraftLine) {
    const key = toAutomationIdSegment(line.purchaseRevisionLineId)
    return {
        qualified: `fulfillment-operations-receipt-form-qualified-quantity-${key}`,
        rejected: `fulfillment-operations-receipt-form-rejected-quantity-${key}`,
        result: `fulfillment-operations-receipt-form-quality-result-${key}`,
    }
}

function rowTargets(row: BatchRow) {
    return row.receipt
        ? [row.targetId, ...Object.values(receiptFieldIds(row.receipt))]
        : [row.targetId]
}

export type FulfillmentBatchTableHandle = {
    reveal: (targetId?: string) => string | undefined
}

export function FulfillmentBatchTable({
    id,
    operation,
    draft,
    quantities,
    disabled,
    issues,
    onChange,
    ref,
}: {
    id: string
    operation: FulfillmentOperation
    draft: FulfillmentDraft
    quantities?: ReturnType<typeof fulfillmentBatchQuantities>
    disabled: boolean
    issues: readonly ValidationIssue[]
    onChange: (draft: FulfillmentDraft) => void
    ref: Ref<FulfillmentBatchTableHandle>
}) {
    const [checked, setChecked] = useState<Set<string>>(() => new Set())
    const [query, setQuery] = useState("")
    const [onlyIssues, setOnlyIssues] = useState(false)
    const [page, setPage] = useState(0)
    const rows = batchRows(operation, draft, id)
    const search = query.trim().toLocaleLowerCase()
    const messagesByTarget = new Map<string, string[]>()
    for (const { targetId, message } of issues) {
        if (!targetId) continue
        const list = messagesByTarget.get(targetId) ?? []
        list.push(message)
        messagesByTarget.set(targetId, list)
    }
    const problems = (row: BatchRow) =>
        rowTargets(row).flatMap((target) => messagesByTarget.get(target) ?? [])
    const addressIssues = new Set<string>()
    if (
        (draft.type === "WAREHOUSE_SHIP" || draft.type === "SUPPLIER_DIRECT") &&
        draft.addressSnapshot &&
        draft.packing
    ) {
        const targets = [
            ...draft.addressSnapshot.destinations.map(({ key }) => ({
                key,
                prefix: `fulfillment-packing-address-${toAutomationIdSegment(key)}-`,
            })),
            ...draft.packing.packages.map(({ id: pkgId, destination_key }) => ({
                key: destination_key,
                prefix: `fulfillment-packing-package-${toAutomationIdSegment(pkgId)}-`,
            })),
        ]
        for (const { targetId } of issues) {
            const address = targets.find(({ prefix }) =>
                targetId?.startsWith(prefix),
            )
            if (address) addressIssues.add(address.key)
        }
    }

    const rowHasIssue = (row: BatchRow) =>
        problems(row).length > 0 ||
        Boolean(row.pendingAddress) ||
        Boolean(row.addressKey && addressIssues.has(row.addressKey))
    const issueRowCount = rows.filter(rowHasIssue).length
    const showFilters =
        rows.length > FILTER_THRESHOLD ||
        issueRowCount > 0 ||
        onlyIssues ||
        Boolean(search)

    const filtered = showFilters
        ? rows.filter(
              (row) =>
                  (!onlyIssues || rowHasIssue(row)) &&
                  `${row.name} ${row.sku} ${row.address ?? ""}`
                      .toLocaleLowerCase()
                      .includes(search),
          )
        : rows
    const pageCount = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE))
    const currentPage = Math.min(page, pageCount - 1)
    const visible = filtered.slice(
        currentPage * PAGE_SIZE,
        (currentPage + 1) * PAGE_SIZE,
    )
    const receipt = draft.type === "RECEIPT"
    const isDispatch =
        draft.type === "WAREHOUSE_SHIP" || draft.type === "SUPPLIER_DIRECT"
    const quantityLabel = receipt
        ? "本次到货"
        : draft.type === "SERVICE"
          ? "本次完成"
          : draft.type === "ELECTRONIC"
            ? "本次交付"
            : "本次发货"
    const remainingHeaderLabel = receipt
        ? "待收余量"
        : draft.type === "SERVICE"
          ? "待完成余量"
          : draft.type === "ELECTRONIC"
            ? "待交付余量"
            : "待履约余量"

    useImperativeHandle(ref, () => ({
        reveal(targetId) {
            const index = targetId
                ? rows.findIndex((row) => rowTargets(row).includes(targetId))
                : Math.max(0, rows.findIndex(rowHasIssue))
            const row = rows[index]
            if (!row) return undefined
            setQuery("")
            setOnlyIssues(false)
            setPage(Math.floor(index / PAGE_SIZE))
            return !targetId || targetId === row.targetId ? row.id : targetId
        },
    }))

    function setCheckedRows(targetRows: readonly BatchRow[], next: boolean) {
        setChecked((current) => {
            const changed = new Set(current)
            for (const row of targetRows) {
                if (next) changed.add(row.id)
                else changed.delete(row.id)
            }
            return changed
        })
    }

    function applyQuantities(
        values: Array<{ row: BatchRow; quantity: string }>,
    ) {
        if (!values.length) return
        if (isDispatch && draft.packing) {
            let packing = draft.packing
            for (const { row, quantity } of values) {
                if (row.allocation) {
                    packing = changeDispatch(
                        packing,
                        row.allocation.destination,
                        row.allocation.line,
                        quantity,
                    )
                }
            }
            onChange({ ...draft, packing })
            return
        }
        if (draft.type === "RECEIPT") {
            const byIndex = new Map<number, string>()
            for (const { row, quantity } of values) {
                if (row.lineIndex !== undefined)
                    byIndex.set(row.lineIndex, quantity)
            }
            const lines = draft.lines.map((line, index) => {
                const quantity = byIndex.get(index)
                if (quantity === undefined) return line
                return withDerivedQualified({
                    ...line,
                    receivedQuantity: quantity,
                })
            })
            onChange({ ...draft, lines })
            return
        }
        if (draft.type === "ELECTRONIC" || draft.type === "SERVICE") {
            const byIndex = new Map<number, string>()
            for (const { row, quantity } of values) {
                if (row.lineIndex !== undefined)
                    byIndex.set(row.lineIndex, quantity)
            }
            const lines = draft.lines.map((line, index) => {
                const quantity = byIndex.get(index)
                if (quantity === undefined) return line
                return { ...line, quantity }
            })
            onChange({ ...draft, lines })
        }
    }

    function updateReceiptLine(
        lineIndex: number,
        patch: Partial<ReceiptDraftLine>,
        deriveQualified = false,
    ) {
        if (draft.type !== "RECEIPT") return
        const lines = draft.lines.map((line, index) => {
            if (index !== lineIndex) return line
            const next = { ...line, ...patch }
            return deriveQualified ? withDerivedQualified(next) : next
        })
        onChange({ ...draft, lines })
    }

    const fillableRows = (targetRows: readonly BatchRow[]) =>
        targetRows.flatMap((row) =>
            row.editableKind !== "readonly" &&
            row.limit &&
            positiveQuantity(row.limit)
                ? [{ row, quantity: row.limit }]
                : [],
        )

    const clearableRows = (targetRows: readonly BatchRow[]) =>
        targetRows.flatMap((row) =>
            row.editableKind !== "readonly" ? [{ row, quantity: "0" }] : [],
        )

    const fillableAll = fillableRows(rows)
    const checkedRows = rows.filter((row) => checked.has(row.id))
    const checkedCount = checkedRows.length
    const visibleCheckedCount = visible.filter((row) =>
        checked.has(row.id),
    ).length
    const isFiltered = filtered.length < rows.length
    const showPagination = filtered.length > PAGE_SIZE && pageCount > 1
    const colSpan = receipt ? 4 : 3

    if (!rows.length) return null

    return (
        <section
            id={id}
            tabIndex={-1}
            className="space-y-2 outline-none"
            aria-label="本次履约明细"
        >
            <div
                className={cn(
                    "flex min-h-10 min-w-0 flex-wrap items-center justify-between gap-2 rounded-md border px-2 py-1 transition-colors",
                    checkedCount > 0
                        ? "border-info-border/70 bg-info-soft/50"
                        : "border-transparent px-0",
                )}
            >
                {checkedCount > 0 ? (
                    <>
                        <div className="flex flex-wrap items-center gap-2">
                            <span className="num text-xs font-medium text-info-soft-foreground">
                                已勾选 {checkedCount} 行
                            </span>
                            {checkedCount < filtered.length ? (
                                <Button
                                    id={`${id}-check-all`}
                                    type="button"
                                    size="xs"
                                    variant="ghost"
                                    onClick={() =>
                                        setCheckedRows(filtered, true)
                                    }
                                >
                                    全选 ({filtered.length})
                                </Button>
                            ) : null}
                            <Button
                                id={`${id}-uncheck`}
                                type="button"
                                size="xs"
                                variant="ghost"
                                onClick={() => setChecked(new Set())}
                            >
                                取消勾选
                            </Button>
                        </div>
                        <div className="flex flex-wrap items-center gap-1">
                            <Button
                                id={`${id}-fill-checked`}
                                type="button"
                                size="xs"
                                variant="secondary"
                                disabled={
                                    disabled ||
                                    !fillableRows(checkedRows).length
                                }
                                onClick={() =>
                                    applyQuantities(fillableRows(checkedRows))
                                }
                            >
                                勾选项填余量
                            </Button>
                            <Button
                                id={`${id}-clear-checked`}
                                type="button"
                                size="xs"
                                variant="ghost"
                                disabled={
                                    disabled ||
                                    !clearableRows(checkedRows).length
                                }
                                onClick={() =>
                                    applyQuantities(clearableRows(checkedRows))
                                }
                            >
                                清空勾选项数量
                            </Button>
                        </div>
                    </>
                ) : (
                    <>
                        <div className="flex min-w-0 flex-wrap items-center gap-1.5">
                            <h3 className="text-body-compact font-semibold">
                                履约明细
                                {!showFilters ? (
                                    <span className="num ml-1.5 text-xs font-normal text-muted-foreground">
                                        {rows.length} 行
                                    </span>
                                ) : null}
                            </h3>
                            {showFilters ? (
                                <>
                                    <Button
                                        id={`${id}-filter-all`}
                                        type="button"
                                        variant={
                                            !onlyIssues ? "secondary" : "ghost"
                                        }
                                        aria-pressed={!onlyIssues}
                                        size="xs"
                                        onClick={() => {
                                            setOnlyIssues(false)
                                            setPage(0)
                                        }}
                                    >
                                        全部{" "}
                                        <span className="num">
                                            {rows.length}
                                        </span>
                                    </Button>
                                    {issueRowCount > 0 || onlyIssues ? (
                                        <Button
                                            id={`${id}-issues`}
                                            type="button"
                                            variant={
                                                onlyIssues
                                                    ? "secondary"
                                                    : "ghost"
                                            }
                                            aria-pressed={onlyIssues}
                                            size="xs"
                                            onClick={() => {
                                                setOnlyIssues(!onlyIssues)
                                                setPage(0)
                                            }}
                                        >
                                            待完善{" "}
                                            <span className="num">
                                                {issueRowCount}
                                            </span>
                                        </Button>
                                    ) : null}
                                    <div className="relative min-w-28 flex-[1_1_9rem]">
                                        <SearchIcon
                                            aria-hidden="true"
                                            className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground"
                                        />
                                        <Input
                                            id={`${id}-search`}
                                            type="search"
                                            size="sm"
                                            aria-label="搜索商品、编码或收货地址"
                                            placeholder="搜索履约明细"
                                            className="pl-7"
                                            value={query}
                                            onChange={(event) => {
                                                setQuery(event.target.value)
                                                setPage(0)
                                            }}
                                        />
                                    </div>
                                </>
                            ) : null}
                        </div>
                        <div className="flex flex-wrap items-center gap-1">
                            {fillableAll.length > 0 ? (
                                <Button
                                    id={`${id}-fill-all`}
                                    type="button"
                                    size="xs"
                                    variant="outline"
                                    disabled={disabled}
                                    onClick={() => applyQuantities(fillableAll)}
                                >
                                    全量填入
                                </Button>
                            ) : null}
                        </div>
                    </>
                )}
            </div>

            <Table className="table-fixed [&_td]:px-2 [&_th]:bg-surface-sunken [&_th]:px-2 [&_th:first-child]:rounded-l-md [&_th:last-child]:rounded-r-md">
                <colgroup>
                    <col className="w-9" />
                    <col />
                    <col className={receipt ? "w-[28%]" : "w-[38%]"} />
                    {receipt ? <col className="w-[34%]" /> : null}
                </colgroup>
                <TableHeader className="bg-transparent [&_tr]:border-0">
                    <TableRow className="border-0 hover:bg-transparent">
                        <TableHead>
                            <NativeCheckbox
                                id={`${id}-check-visible`}
                                aria-label="勾选当前页履约明细"
                                checked={
                                    visible.length > 0 &&
                                    visibleCheckedCount === visible.length
                                }
                                indeterminate={
                                    visibleCheckedCount > 0 &&
                                    visibleCheckedCount < visible.length
                                }
                                disabled={disabled || visible.length === 0}
                                onCheckedChange={(value) =>
                                    setCheckedRows(visible, value)
                                }
                            />
                        </TableHead>
                        <TableHead>商品／项目</TableHead>
                        <TableHead className="whitespace-normal">
                            {quantityLabel} / {remainingHeaderLabel}
                        </TableHead>
                        {receipt ? <TableHead>质检</TableHead> : null}
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {visible.map((row) => {
                        const messages = problems(row)
                        const fill =
                            row.editableKind !== "readonly" &&
                            row.limit &&
                            positiveQuantity(row.limit)
                                ? { row, quantity: row.limit }
                                : null
                        const isZeroQuantity = !positiveQuantity(row.quantity)
                        return (
                            <TableRow
                                key={row.id}
                                tabIndex={-1}
                                data-state={
                                    checked.has(row.id) ? "selected" : undefined
                                }
                                className={cn(isZeroQuantity && "opacity-60")}
                            >
                                <TableCell className="align-top">
                                    <NativeCheckbox
                                        id={`${row.id}-check`}
                                        aria-label={`勾选 ${row.name} 批量处理`}
                                        checked={checked.has(row.id)}
                                        disabled={disabled}
                                        onCheckedChange={(value) =>
                                            setCheckedRows([row], value)
                                        }
                                    />
                                </TableCell>
                                <TableCell className="align-top whitespace-normal">
                                    <p className="wrap-anywhere font-medium">
                                        {row.name}
                                    </p>
                                    {row.sku || row.address ? (
                                        <p className="mt-0.5 wrap-anywhere text-xs text-muted-foreground">
                                            {[row.sku, row.address]
                                                .filter(Boolean)
                                                .join(" · ")}
                                        </p>
                                    ) : null}
                                    {messages.length > 0 ? (
                                        <p
                                            id={`${row.id}-issues`}
                                            className="mt-1 wrap-anywhere text-xs text-destructive"
                                        >
                                            {[...new Set(messages)].join("；")}
                                        </p>
                                    ) : null}
                                </TableCell>
                                <TableCell className="align-top whitespace-normal">
                                    <div className="flex min-w-0 flex-wrap items-center gap-x-1.5 gap-y-1">
                                        {row.editableKind !== "readonly" ? (
                                            <Input
                                                id={row.id}
                                                size="sm"
                                                aria-label={`${row.name}${row.address ? ` · ${row.address}` : ""} · ${quantityLabel}`}
                                                aria-invalid={
                                                    messagesByTarget.has(
                                                        row.targetId,
                                                    ) || undefined
                                                }
                                                aria-describedby={
                                                    messages.length
                                                        ? `${row.id}-issues`
                                                        : undefined
                                                }
                                                inputMode="decimal"
                                                value={row.quantity}
                                                disabled={disabled}
                                                className={cn(
                                                    inlineInputClassName,
                                                    "w-16 max-w-full",
                                                )}
                                                onChange={(event) =>
                                                    applyQuantities([
                                                        {
                                                            row,
                                                            quantity:
                                                                event.target
                                                                    .value,
                                                        },
                                                    ])
                                                }
                                            />
                                        ) : (
                                            <span
                                                id={row.id}
                                                tabIndex={-1}
                                                className="num wrap-anywhere font-medium"
                                            >
                                                {row.quantity}
                                            </span>
                                        )}
                                        <span className="num wrap-anywhere text-tiny text-muted-foreground">
                                            {row.limit != null
                                                ? `/ ${row.limit}`
                                                : ""}{" "}
                                            {row.unit}
                                        </span>
                                        {fill ? (
                                            <Button
                                                id={`${row.id}-fill`}
                                                className="h-auto px-1"
                                                type="button"
                                                size="xs"
                                                variant="ghost"
                                                disabled={disabled}
                                                onClick={() =>
                                                    applyQuantities([fill])
                                                }
                                            >
                                                填余量
                                            </Button>
                                        ) : null}
                                    </div>
                                </TableCell>
                                {row.receipt && row.lineIndex !== undefined ? (
                                    <ReceiptQualityCell
                                        line={row.receipt}
                                        itemName={row.name}
                                        disabled={disabled}
                                        messagesByTarget={messagesByTarget}
                                        issueId={
                                            messages.length
                                                ? `${row.id}-issues`
                                                : undefined
                                        }
                                        onUpdate={(patch, derive) =>
                                            updateReceiptLine(
                                                row.lineIndex!,
                                                patch,
                                                derive,
                                            )
                                        }
                                    />
                                ) : null}
                            </TableRow>
                        )
                    })}
                    {!visible.length ? (
                        <TableRow>
                            <TableCell
                                colSpan={colSpan}
                                className="py-4 text-center text-xs text-muted-foreground"
                            >
                                没有符合条件的明细
                            </TableCell>
                        </TableRow>
                    ) : null}
                </TableBody>
                <TableFooter className="border-0 bg-transparent">
                    <TableRow className="border-0 hover:bg-transparent">
                        <TableCell
                            colSpan={colSpan}
                            className="rounded-md bg-surface-sunken whitespace-normal"
                        >
                            <div className="flex flex-wrap items-center justify-between gap-2 text-xs">
                                <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
                                    <span>
                                        整单合计 ·{" "}
                                        <span className="num">
                                            {quantities
                                                ? `本次 ${quantities.lineCount} 行 · 总数量 ${quantities.total}`
                                                : `共 ${rows.length} 行`}
                                        </span>
                                    </span>
                                    {isFiltered ? (
                                        <span className="text-tiny text-muted-foreground">
                                            （当前显示{" "}
                                            <span className="num">
                                                {filtered.length} /{" "}
                                                {rows.length}
                                            </span>{" "}
                                            项，合计按整单）
                                        </span>
                                    ) : null}
                                </div>
                                {showPagination ? (
                                    <div className="flex items-center gap-1">
                                        <Button
                                            id={`${id}-previous`}
                                            type="button"
                                            size="icon-xs"
                                            variant="ghost"
                                            aria-label="上一页"
                                            disabled={currentPage === 0}
                                            onClick={() =>
                                                setPage(currentPage - 1)
                                            }
                                        >
                                            <ChevronLeftIcon aria-hidden="true" />
                                        </Button>
                                        <span className="num">
                                            {currentPage + 1} / {pageCount}
                                        </span>
                                        <Button
                                            id={`${id}-next`}
                                            type="button"
                                            size="icon-xs"
                                            variant="ghost"
                                            aria-label="下一页"
                                            disabled={
                                                currentPage + 1 >= pageCount
                                            }
                                            onClick={() =>
                                                setPage(currentPage + 1)
                                            }
                                        >
                                            <ChevronRightIcon aria-hidden="true" />
                                        </Button>
                                    </div>
                                ) : null}
                            </div>
                        </TableCell>
                    </TableRow>
                </TableFooter>
            </Table>
        </section>
    )
}

function ReceiptQualityCell({
    line,
    itemName,
    disabled,
    messagesByTarget,
    issueId,
    onUpdate,
}: {
    line: ReceiptDraftLine
    itemName: string
    disabled: boolean
    messagesByTarget: ReadonlyMap<string, string[]>
    issueId?: string
    onUpdate: (
        patch: Partial<ReceiptDraftLine>,
        deriveQualified?: boolean,
    ) => void
}) {
    const ids = receiptFieldIds(line)
    const normalizedResult =
        QUALITY_LABELS[line.qualityResult] ?? line.qualityResult

    return (
        <TableCell className="align-top whitespace-normal">
            <div className="space-y-1.5">
                <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                    <label
                        htmlFor={ids.qualified}
                        className="inline-flex items-center gap-1 text-tiny text-muted-foreground"
                    >
                        <span>合格</span>
                        <Input
                            id={ids.qualified}
                            size="sm"
                            aria-label={`${itemName} 合格数量`}
                            aria-invalid={
                                messagesByTarget.has(ids.qualified) || undefined
                            }
                            aria-describedby={issueId}
                            inputMode="decimal"
                            disabled={disabled}
                            value={line.qualifiedQuantity}
                            className={cn(inlineInputClassName, "w-14")}
                            onChange={(event) =>
                                onUpdate({
                                    qualifiedQuantity: event.target.value,
                                })
                            }
                        />
                    </label>
                    <label
                        htmlFor={ids.rejected}
                        className="inline-flex items-center gap-1 text-tiny text-muted-foreground"
                    >
                        <span>不合格</span>
                        <Input
                            id={ids.rejected}
                            size="sm"
                            aria-label={`${itemName} 不合格数量`}
                            aria-invalid={
                                messagesByTarget.has(ids.rejected) || undefined
                            }
                            aria-describedby={issueId}
                            inputMode="decimal"
                            disabled={disabled}
                            value={line.rejectedQuantity}
                            className={cn(inlineInputClassName, "w-14")}
                            onChange={(event) =>
                                onUpdate(
                                    { rejectedQuantity: event.target.value },
                                    true,
                                )
                            }
                        />
                    </label>
                </div>
                <OptionCombobox
                    id={ids.result}
                    size="sm"
                    options={QUALITY_RESULT_OPTIONS}
                    value={normalizedResult || null}
                    disabled={disabled}
                    allowClear={false}
                    placeholder="选择质量结果"
                    aria-label={`${itemName} 质量结果`}
                    aria-invalid={messagesByTarget.has(ids.result) || undefined}
                    aria-describedby={issueId}
                    inputClassName="border-0 bg-surface-sunken hover:bg-card"
                    onValueChange={(value) =>
                        onUpdate({ qualityResult: value ?? "" })
                    }
                />
                {line.evidenceNote ? (
                    <p className="wrap-anywhere text-xs text-muted-foreground">
                        {line.evidenceNote}
                    </p>
                ) : null}
            </div>
        </TableCell>
    )
}
