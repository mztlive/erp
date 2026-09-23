"use client"

import * as React from "react"

import { OptionCombobox } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { WarehouseSearchCombobox } from "@/features/entity-selectors"
import {
    DUE_FILTER_OPTIONS,
    GATE_FILTER_OPTIONS,
    type DueFilter,
    type GateFilter,
} from "@/features/fulfillment-operations/lib/filters"
import {
    OPERATION_TYPE_SHORT,
    SLUG_TO_TYPE,
} from "@/features/fulfillment-operations/types"

export type QueueFilterPatch = Record<string, string | null | undefined>

/** 可被单独移除的已生效条件。 */
export type FulfillmentFilterKey =
    | "q"
    | "type"
    | "warehouseId"
    | "due"
    | "gate"
    | "salesOrderId"
    | "purchaseOrderId"

export type FulfillmentAppliedChip = Readonly<{
    key: FulfillmentFilterKey
    label: string
}>

/** chip 与摘要使用短文案；完整说明见面板下拉选项。 */
const GATE_CHIP_LABELS: Record<GateFilter, string> = {
    satisfied: "货款已到",
    blocked: "先款未到",
}

const DUE_CHIP_LABELS: Record<DueFilter, string> = {
    today: "今日到期",
    overdue: "已超期",
}

const MORE_CHIP_KEYS: readonly FulfillmentFilterKey[] = [
    "warehouseId",
    "due",
    "gate",
    "salesOrderId",
    "purchaseOrderId",
]

/**
 * 履约队列筛选：搜索 + 查询 + 更多筛选（仓库 / 到期 / 货款 / 来源锁定）。
 * 类型是第 0 层视图，不进本工具栏。自动下一项放在 actions。
 */
export function FulfillmentQueueToolbar({
    q,
    warehouseId,
    due,
    gate,
    salesOrderId,
    purchaseOrderId,
    salesOrderNo,
    purchaseNo,
    warehouseLabel,
    autoNext,
    showAutoNext,
    type,
    onPatch,
    onClearAllFilters,
    onAutoNextChange,
    resultCount,
    loading,
    failed,
}: {
    q: string | undefined
    warehouseId: string | undefined
    due: DueFilter | undefined
    gate: GateFilter | undefined
    salesOrderId: string | undefined
    purchaseOrderId: string | undefined
    salesOrderNo: string | undefined
    purchaseNo: string | undefined
    warehouseLabel: string | undefined
    autoNext: boolean
    showAutoNext: boolean
    type?: string | null
    onPatch: (patch: QueueFilterPatch) => void
    onClearAllFilters: () => void
    onAutoNextChange: (next: boolean) => void
    resultCount?: number
    loading?: boolean
    failed?: boolean
}) {
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const panelId = "fulfillment-operations-queue-more-panel"

    const [searchDraft, setSearchDraft] = React.useState(q ?? "")
    const [warehouseIdDraft, setWarehouseIdDraft] = React.useState<
        string | null
    >(warehouseId ?? null)
    const [dueDraft, setDueDraft] = React.useState<DueFilter | "all">(
        due ?? "all",
    )
    const [gateDraft, setGateDraft] = React.useState<GateFilter | "all">(
        gate ?? "all",
    )
    const hasStructuredFilters = Boolean(warehouseId || due || gate)
    const [panelOpen, setPanelOpen] = React.useState(hasStructuredFilters)

    React.useEffect(() => {
        if (document.activeElement !== searchInputRef.current) {
            setSearchDraft(q ?? "")
        }
    }, [q])

    React.useEffect(() => {
        setWarehouseIdDraft(warehouseId ?? null)
        setDueDraft(due ?? "all")
        setGateDraft(gate ?? "all")
    }, [due, gate, warehouseId])

    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (
                event.key !== "/" ||
                event.target instanceof HTMLInputElement ||
                event.target instanceof HTMLTextAreaElement ||
                event.target instanceof HTMLSelectElement ||
                (event.target as HTMLElement | null)?.isContentEditable
            ) {
                return
            }
            if (
                document.querySelector('[role="dialog"], [data-slot="sheet"]')
            ) {
                return
            }
            event.preventDefault()
            searchInputRef.current?.focus()
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [])

    const applyFilters = React.useCallback(() => {
        const next: QueueFilterPatch = {
            q: searchDraft.trim() || null,
            warehouseId: warehouseIdDraft || null,
            due: dueDraft === "all" ? null : dueDraft,
            gate: gateDraft === "all" ? null : gateDraft,
        }
        const unchanged =
            (next.q ?? undefined) === (q ?? undefined) &&
            (next.warehouseId ?? undefined) === (warehouseId ?? undefined) &&
            (next.due ?? undefined) === (due ?? undefined) &&
            (next.gate ?? undefined) === (gate ?? undefined)
        if (unchanged) {
            setPanelOpen(false)
            return
        }
        onPatch({ ...next, currentOperationId: null })
        setPanelOpen(false)
    }, [
        due,
        dueDraft,
        gate,
        gateDraft,
        onPatch,
        q,
        searchDraft,
        warehouseId,
        warehouseIdDraft,
    ])

    const resetMoreFilters = React.useCallback(() => {
        setWarehouseIdDraft(null)
        setDueDraft("all")
        setGateDraft("all")
    }, [])

    const clearAllFilters = React.useCallback(() => {
        setPanelOpen(false)
        onClearAllFilters()
    }, [onClearAllFilters])

    const removeFilter = React.useCallback(
        (key: FulfillmentFilterKey) => {
            onPatch({ [key]: null, currentOperationId: null })
        },
        [onPatch],
    )

    const typeLabel = React.useMemo(() => {
        if (!type || type === "all") return undefined
        return type
            .split(",")
            .map((slug) => {
                const operationType = SLUG_TO_TYPE[slug.trim()]
                return operationType
                    ? OPERATION_TYPE_SHORT[operationType]
                    : null
            })
            .filter((label): label is string => label != null)
            .join("、")
    }, [type])

    const appliedChips = React.useMemo<
        readonly FulfillmentAppliedChip[]
    >(() => {
        const chips: FulfillmentAppliedChip[] = []
        if (q) chips.push({ key: "q", label: `搜索：${q}` })
        if (typeLabel) chips.push({ key: "type", label: `类型：${typeLabel}` })
        if (warehouseId) {
            chips.push({
                key: "warehouseId",
                label: `仓库：${warehouseLabel ?? "已定位"}`,
            })
        }
        if (due)
            chips.push({ key: "due", label: `到期：${DUE_CHIP_LABELS[due]}` })
        if (gate) {
            chips.push({
                key: "gate",
                label: `货款：${GATE_CHIP_LABELS[gate]}`,
            })
        }
        if (salesOrderId) {
            chips.push({
                key: "salesOrderId",
                label: `销售单 ${salesOrderNo ?? "已定位"}`,
            })
        }
        if (purchaseOrderId) {
            chips.push({
                key: "purchaseOrderId",
                label: `采购单 ${purchaseNo ?? "已定位"}`,
            })
        }
        return chips
    }, [
        due,
        gate,
        purchaseNo,
        purchaseOrderId,
        q,
        salesOrderNo,
        salesOrderId,
        typeLabel,
        warehouseId,
        warehouseLabel,
    ])

    const moreCount = appliedChips.filter(({ key }) =>
        MORE_CHIP_KEYS.includes(key),
    ).length
    const hasPendingChanges =
        searchDraft.trim() !== (q ?? "") ||
        (warehouseIdDraft ?? null) !== (warehouseId ?? null) ||
        dueDraft !== (due ?? "all") ||
        gateDraft !== (gate ?? "all")

    return (
        <ListWorkspaceFilterBar
            idPrefix="fulfillment-operations-queue-filter"
            formAriaLabel="履约单据查询"
            onSubmit={applyFilters}
            search={
                <ListSearchField
                    id="fulfillment-operations-queue-search"
                    searchInputRef={searchInputRef}
                    value={searchDraft}
                    onChange={setSearchDraft}
                    placeholder="销售单号、采购单号、客户、供应商"
                    aria-label="搜索履约单据"
                />
            }
            queryButtonId="fulfillment-operations-queue-apply-filters"
            moreCount={moreCount}
            moreOpen={panelOpen}
            onToggleMore={() => setPanelOpen((open) => !open)}
            moreButtonId="fulfillment-operations-queue-filters-trigger"
            morePanelId={panelId}
            morePanelAriaLabel="履约单据更多筛选条件"
            onResetMore={resetMoreFilters}
            resetMoreButtonId="fulfillment-operations-queue-reset-more"
            morePanel={
                <div className="grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-[minmax(0,0.75fr)_minmax(0,1.25fr)]">
                    <ListWorkspaceFilterField
                        htmlFor="fulfillment-operations-queue-due-filter"
                        label="到期"
                    >
                        <OptionCombobox
                            id="fulfillment-operations-queue-due-filter"
                            className="w-full min-w-0"
                            aria-label="到期"
                            value={dueDraft === "all" ? null : dueDraft}
                            options={DUE_FILTER_OPTIONS}
                            placeholder="全部"
                            onValueChange={(value) =>
                                setDueDraft(
                                    value === "today" || value === "overdue"
                                        ? value
                                        : "all",
                                )
                            }
                        />
                    </ListWorkspaceFilterField>
                    <ListWorkspaceFilterField
                        htmlFor="fulfillment-operations-queue-gate-filter"
                        label="货款情况"
                    >
                        <OptionCombobox
                            id="fulfillment-operations-queue-gate-filter"
                            className="w-full min-w-0"
                            aria-label="货款情况"
                            value={gateDraft === "all" ? null : gateDraft}
                            options={GATE_FILTER_OPTIONS}
                            placeholder="全部"
                            onValueChange={(value) =>
                                setGateDraft(
                                    value === "blocked" || value === "satisfied"
                                        ? value
                                        : "all",
                                )
                            }
                        />
                    </ListWorkspaceFilterField>
                    <ListWorkspaceFilterField
                        className="sm:col-span-2"
                        htmlFor="fulfillment-operations-queue-warehouse-filter"
                        label="仓库"
                    >
                        <WarehouseSearchCombobox
                            id="fulfillment-operations-queue-warehouse-filter"
                            className="w-full sm:w-60"
                            value={warehouseIdDraft ?? undefined}
                            onValueChange={(id) =>
                                setWarehouseIdDraft(id ?? null)
                            }
                            placeholder="全部仓库"
                            aria-label="按仓库筛选（只对入库和发货有效）"
                        />
                    </ListWorkspaceFilterField>
                </div>
            }
            resultStatus={listWorkspaceFilterStatusText({
                loading,
                failed,
                resultCount,
                noun: "项作业",
                loadingLabel: "正在加载履约作业…",
            })}
            chips={appliedChips}
            onClearChip={(key) => removeFilter(key as FulfillmentFilterKey)}
            onClearAll={clearAllFilters}
            clearButtonId="fulfillment-operations-queue-clear-all"
            hasPendingChanges={hasPendingChanges}
            pendingHint="条件已修改，待查询"
            actions={
                showAutoNext ? (
                    <div className="flex items-center gap-2">
                        <Label
                            htmlFor="fulfillment-operations-queue-auto-next"
                            className="text-muted-foreground"
                        >
                            自动下一项
                        </Label>
                        <Switch
                            id="fulfillment-operations-queue-auto-next"
                            checked={autoNext}
                            onCheckedChange={onAutoNextChange}
                        />
                    </div>
                ) : undefined
            }
        />
    )
}
