"use client"

import { FulfillmentPackingForm } from "./fulfillment-packing-form"
import { useImperativeHandle, useState, type Ref } from "react"
import {
    TableToolbar,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { compactFixed, sumFixed } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { displayText, lineItemTitle } from "../../lib/readable-label"
import type { FulfillmentDraft, FulfillmentOperation } from "../../types"
import {
    FulfillmentDeliveryRow,
    type DeliveryRowData,
} from "./fulfillment-delivery-row"
import { FulfillmentTrackingInput } from "./fulfillment-tracking-input"

type DeliveryDraft = Extract<
    FulfillmentDraft,
    { type: "WAREHOUSE_SHIP" | "SUPPLIER_DIRECT" }
>

export type FulfillmentDeliveryFormHandle = {
    /** 清除筛选、展开对应编辑区，再将焦点交给原校验目标。 */
    locate: (targetId: string) => boolean
}

type DeliveryFormProps = {
    operation: FulfillmentOperation
    draft: DeliveryDraft
    onChange: (draft: FulfillmentDraft) => void
    disabled?: boolean
    compact?: boolean
    ref?: Ref<FulfillmentDeliveryFormHandle>
}

/** 换任务时清空选择和未提交输入；同一任务内的折叠、筛选不卸载录入表单。 */
export function FulfillmentDeliveryForm(props: DeliveryFormProps) {
    if (props.draft.packing && props.draft.addressSnapshot) {
        return (
            <FulfillmentPackingForm
                key={props.operation.operationId}
                {...props}
                packing={props.draft.packing}
                source={props.draft.addressSnapshot}
            />
        )
    }
    return (
        <div className="space-y-4">
            {props.draft.packingUnavailableReason && (
                <p
                    id="fulfillment-packing-source"
                    tabIndex={-1}
                    className="rounded-lg border p-4 text-body-compact"
                >
                    {props.draft.packingUnavailableReason}
                </p>
            )}
            <DeliveryForm key={props.operation.operationId} {...props} />
        </div>
    )
}

function DeliveryForm({
    operation,
    draft,
    onChange,
    disabled,
    compact = false,
    ref,
}: DeliveryFormProps) {
    const kind = draft.type === "WAREHOUSE_SHIP" ? "ship" : "direct"
    const prefix = `fulfillment-operations-${kind}-form`
    const entries = draft.trackingEntries ?? []
    const actualIds = [
        ...new Set(draft.lines.map((line) => line.salesOrderLineId)),
    ]
    const actualIdSet = new Set(actualIds)
    const orphaned = entries.filter(
        (entry) => !actualIdSet.has(entry.salesOrderLineId),
    )
    const [activeId, setActiveId] = useState<string | null>(
        actualIds.length === 1 ? actualIds[0] : null,
    )
    const [search, setSearch] = useState("")
    const [onlyMissing, setOnlyMissing] = useState(false)
    const [selected, setSelected] = useState<Set<string>>(() => new Set())
    const [batchOpen, setBatchOpen] = useState(false)
    const selectedIds = actualIds.filter((id) => selected.has(id))
    const rows: DeliveryRowData[] = actualIds.map((salesOrderLineId, index) => {
        const source = operation.lines.find(
            (line) => line.salesOrderLineId === salesOrderLineId,
        )
        const packages = entries.filter(
            (entry) => entry.salesOrderLineId === salesOrderLineId,
        )
        return {
            salesOrderLineId,
            title: lineItemTitle(source?.itemName, index),
            sku: displayText(source?.skuCode),
            quantity: compactFixed(
                sumFixed(
                    draft.lines
                        .filter(
                            (line) =>
                                line.salesOrderLineId === salesOrderLineId,
                        )
                        .map((line) => line.quantity),
                    { maxScale: 6, outputScale: 6 },
                ),
            ),
            unit: displayText(source?.unitName),
            packages,
            pending: Boolean(
                draft.pendingTrackingLineIds?.includes(salesOrderLineId),
            ),
            linePrefix: `${prefix}-line-${toAutomationIdSegment(salesOrderLineId)}`,
        }
    })
    const missingRows = rows.filter((row) => !row.packages.length)
    const query = search.trim().toLocaleLowerCase()
    const visibleRows = rows.filter(
        (row) =>
            (!onlyMissing ||
                !row.packages.length ||
                row.salesOrderLineId === activeId) &&
            (!query ||
                `${row.title} ${row.sku}`.toLocaleLowerCase().includes(query)),
    )
    const visibleIds = new Set(visibleRows.map((row) => row.salesOrderLineId))
    const visibleSelected = selectedIds.filter((id) =>
        visibleIds.has(id),
    ).length
    const hiddenSelected = selectedIds.length - visibleSelected
    const pendingRows = rows.filter((row) => row.pending)

    function focusTarget(targetId: string) {
        requestAnimationFrame(() => {
            const target = document.getElementById(targetId)
            target?.scrollIntoView({ block: "nearest" })
            target?.focus({ preventScroll: true })
        })
    }

    function openLine(lineId: string, targetId?: string) {
        setSearch("")
        setActiveId(lineId)
        setBatchOpen(false)
        focusTarget(
            targetId ??
                `${prefix}-line-${toAutomationIdSegment(lineId)}-tracking-no`,
        )
    }

    useImperativeHandle(ref, () => ({
        locate(targetId) {
            if (targetId.startsWith(`${prefix}-batch-`)) {
                setBatchOpen(true)
                setActiveId(null)
                focusTarget(targetId)
                return true
            }
            const row = rows.find((item) =>
                targetId.startsWith(`${item.linePrefix}-`),
            )
            if (!row) return false
            setOnlyMissing(false)
            openLine(row.salesOrderLineId, targetId)
            return true
        },
    }))

    function toggleVisible(checked: boolean) {
        setSelected((current) => {
            const next = new Set(current)
            for (const row of visibleRows) {
                if (checked) next.add(row.salesOrderLineId)
                else next.delete(row.salesOrderLineId)
            }
            return next
        })
    }

    return (
        <div
            className="min-w-0 space-y-4"
            aria-label={
                draft.type === "WAREHOUSE_SHIP"
                    ? "公司仓发表单"
                    : "供应商直发表单"
            }
        >
            {draft.type === "WAREHOUSE_SHIP" &&
            displayText(draft.warehouseLabel, draft.warehouseId) ? (
                <div className="space-y-1.5">
                    <Label htmlFor={`${prefix}-warehouse`}>发货仓</Label>
                    <Input
                        id={`${prefix}-warehouse`}
                        value={displayText(
                            draft.warehouseLabel,
                            draft.warehouseId,
                        )}
                        disabled
                        readOnly
                    />
                </div>
            ) : null}
            <section
                className="min-w-0 space-y-3"
                aria-labelledby={`${prefix}-title`}
            >
                <div className="flex flex-wrap items-baseline justify-between gap-2">
                    <h3 id={`${prefix}-title`} className="font-semibold">
                        {compact ? "商品与物流" : "发货明细与物流号"}
                    </h3>
                    <p
                        className="num text-xs text-muted-foreground"
                        aria-live="polite"
                    >
                        共 {rows.length} 项 · 已登记{" "}
                        {rows.length - missingRows.length} 项 · 待登记{" "}
                        {missingRows.length} 项
                    </p>
                </div>
                {rows.length > 1 ? (
                    <div className="flex flex-wrap items-center gap-3">
                        <Input
                            id={`${prefix}-search`}
                            aria-label="搜索商品名称或 SKU"
                            placeholder="搜索商品名称 / SKU"
                            value={search}
                            onChange={(event) => setSearch(event.target.value)}
                            className="w-full sm:w-64"
                        />
                        <Label
                            htmlFor={`${prefix}-only-missing`}
                            className="text-xs"
                        >
                            <Checkbox
                                id={`${prefix}-only-missing`}
                                checked={onlyMissing}
                                onCheckedChange={setOnlyMissing}
                            />
                            只看未登记
                        </Label>
                    </div>
                ) : null}
                <TableToolbarScope>
                    <TableToolbar>
                        <div className="flex flex-wrap items-center gap-3">
                            <span className="text-xs text-muted-foreground">
                                {selectedIds.length
                                    ? `已选 ${selectedIds.length} 项${hiddenSelected ? `（含筛选外 ${hiddenSelected} 项）` : ""}`
                                    : "勾选商品，可批量登记同一物流"}
                            </span>
                            {selectedIds.length ? (
                                <Button
                                    id={`${prefix}-clear-selection`}
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    disabled={disabled}
                                    onClick={() => setSelected(new Set())}
                                >
                                    清空选择
                                </Button>
                            ) : null}
                            <Button
                                id={`${prefix}-batch-toggle`}
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={
                                    disabled ||
                                    (!selectedIds.length &&
                                        !draft.pendingBatchTrackingInput)
                                }
                                aria-expanded={batchOpen}
                                aria-controls={`${prefix}-batch-editor`}
                                onClick={() => {
                                    setBatchOpen(!batchOpen)
                                    setActiveId(null)
                                }}
                            >
                                批量登记物流
                                {draft.pendingBatchTrackingInput
                                    ? " · 有待添加输入"
                                    : ""}
                            </Button>
                        </div>
                    </TableToolbar>
                    <div id={`${prefix}-batch-editor`} hidden={!batchOpen}>
                        <div className="space-y-3 rounded-lg border border-border bg-muted/20 p-3">
                            <div className="flex flex-wrap items-center justify-between gap-2">
                                <p className="text-body-compact font-medium">
                                    {selectedIds.length
                                        ? `为已选 ${selectedIds.length} 项商品追加物流`
                                        : "请先勾选要登记物流的商品"}
                                </p>
                                <Button
                                    id={`${prefix}-batch-close`}
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    onClick={() => {
                                        setBatchOpen(false)
                                        focusTarget(`${prefix}-batch-toggle`)
                                    }}
                                >
                                    收起
                                </Button>
                            </div>
                            {selectedIds.length ? (
                                <p className="text-xs text-muted-foreground">
                                    {rows
                                        .filter((row) =>
                                            selected.has(row.salesOrderLineId),
                                        )
                                        .slice(0, 3)
                                        .map((row) => row.title)
                                        .join("、")}
                                    {selectedIds.length > 3
                                        ? `等 ${selectedIds.length} 项`
                                        : ""}
                                    。保留已有物流，重复记录自动跳过。
                                </p>
                            ) : null}
                            <FulfillmentTrackingInput
                                id={`${prefix}-batch`}
                                batch
                                pendingInput={Boolean(
                                    draft.pendingBatchTrackingInput,
                                )}
                                salesOrderLineIds={selectedIds}
                                entries={entries}
                                disabled={disabled}
                                onChange={(trackingEntries) =>
                                    onChange({
                                        ...draft,
                                        trackingEntries,
                                        pendingBatchTrackingInput: false,
                                    })
                                }
                                onPendingInputChange={(
                                    pendingBatchTrackingInput,
                                ) =>
                                    onChange({
                                        ...draft,
                                        pendingBatchTrackingInput,
                                    })
                                }
                            />
                        </div>
                    </div>
                    <div className="overflow-hidden rounded-lg border border-grid">
                        <Table className="min-w-lg table-fixed">
                            <TableHeader>
                                <TableRow>
                                    <TableHead
                                        scope="col"
                                        className="w-10 px-3"
                                    >
                                        <Checkbox
                                            id={`${prefix}-select-visible`}
                                            aria-label="选择当前显示的全部商品"
                                            checked={
                                                visibleRows.length > 0 &&
                                                visibleSelected ===
                                                    visibleRows.length
                                            }
                                            indeterminate={
                                                visibleSelected > 0 &&
                                                visibleSelected <
                                                    visibleRows.length
                                            }
                                            disabled={
                                                disabled || !visibleRows.length
                                            }
                                            onCheckedChange={toggleVisible}
                                        />
                                    </TableHead>
                                    <TableHead scope="col" className="w-1/3">
                                        商品 / 本次发货
                                    </TableHead>
                                    <TableHead scope="col">
                                        已登记物流
                                    </TableHead>
                                    <TableHead
                                        scope="col"
                                        className="w-28 text-right"
                                    >
                                        操作
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {rows.map((row, index) => {
                                    const nextMissing = [
                                        ...rows.slice(index + 1),
                                        ...rows.slice(0, index),
                                    ].find(
                                        (item) =>
                                            !item.packages.length ||
                                            item.pending,
                                    )
                                    return (
                                        <FulfillmentDeliveryRow
                                            key={row.salesOrderLineId}
                                            row={row}
                                            visible={visibleIds.has(
                                                row.salesOrderLineId,
                                            )}
                                            expanded={
                                                activeId ===
                                                row.salesOrderLineId
                                            }
                                            selected={selected.has(
                                                row.salesOrderLineId,
                                            )}
                                            disabled={disabled}
                                            draft={draft}
                                            onChange={onChange}
                                            onSelect={(checked) =>
                                                setSelected((current) => {
                                                    const next = new Set(
                                                        current,
                                                    )
                                                    if (checked)
                                                        next.add(
                                                            row.salesOrderLineId,
                                                        )
                                                    else
                                                        next.delete(
                                                            row.salesOrderLineId,
                                                        )
                                                    return next
                                                })
                                            }
                                            onOpen={() => {
                                                setActiveId(
                                                    row.salesOrderLineId,
                                                )
                                                setBatchOpen(false)
                                                focusTarget(
                                                    `${row.linePrefix}-tracking-no`,
                                                )
                                            }}
                                            onClose={() => {
                                                setActiveId(null)
                                                focusTarget(
                                                    onlyMissing &&
                                                        row.packages.length
                                                        ? `${prefix}-only-missing`
                                                        : `${row.linePrefix}-toggle`,
                                                )
                                            }}
                                            onNext={
                                                nextMissing
                                                    ? () =>
                                                          openLine(
                                                              nextMissing.salesOrderLineId,
                                                          )
                                                    : undefined
                                            }
                                        />
                                    )
                                })}
                                {!visibleRows.length ? (
                                    <TableRow>
                                        <TableCell
                                            colSpan={4}
                                            className="py-6 text-center text-body-compact text-muted-foreground"
                                        >
                                            {rows.length
                                                ? "没有符合条件的商品"
                                                : "暂无发货商品"}
                                            {rows.length ? (
                                                <Button
                                                    id={`${prefix}-reset-filters`}
                                                    type="button"
                                                    variant="link"
                                                    size="sm"
                                                    onClick={() => {
                                                        setSearch("")
                                                        setOnlyMissing(false)
                                                    }}
                                                >
                                                    显示全部
                                                </Button>
                                            ) : null}
                                        </TableCell>
                                    </TableRow>
                                ) : null}
                            </TableBody>
                        </Table>
                    </div>
                </TableToolbarScope>
                <p className="text-xs text-muted-foreground">
                    按 Enter
                    或点击“添加”；支持一次粘贴多个单号。同一商品可登记多个物流号，同一包裹包含多个商品时可勾选后批量登记。
                </p>
                {pendingRows.length ? (
                    <div
                        className="flex flex-wrap items-center gap-2 text-xs text-destructive"
                        role="status"
                    >
                        <span>
                            {pendingRows.length}{" "}
                            项商品有尚未添加的输入，收起或筛选后仍会保留。
                        </span>
                        <Button
                            id={`${prefix}-locate-pending`}
                            type="button"
                            variant="link"
                            size="sm"
                            onClick={() => {
                                setOnlyMissing(false)
                                openLine(pendingRows[0].salesOrderLineId)
                            }}
                        >
                            继续填写
                        </Button>
                    </div>
                ) : null}
            </section>
            {orphaned.length ? (
                <div className="space-y-2 rounded-lg border border-destructive p-3">
                    <p
                        role="alert"
                        className="text-body-compact text-destructive"
                    >
                        有 {orphaned.length}{" "}
                        个物流号不属于当前发货明细。请移除后重新登记，其他明细的物流号保持原归属。
                    </p>
                    <Button
                        id={`${prefix}-remove-unselected-tracking`}
                        type="button"
                        variant="outline"
                        disabled={disabled}
                        onClick={() =>
                            onChange({
                                ...draft,
                                trackingEntries: entries.filter((entry) =>
                                    actualIdSet.has(entry.salesOrderLineId),
                                ),
                            })
                        }
                    >
                        移除不属于当前明细的物流号
                    </Button>
                </div>
            ) : null}
        </div>
    )
}
