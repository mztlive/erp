"use client"

import * as React from "react"
import { ChevronDownIcon, FilterIcon, SearchIcon } from "lucide-react"

import {
    FilterChip,
    FixedOptionRadioFilter,
    type FixedOptionRadioFilterOption,
    ListToolbar,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { WarehouseSearchCombobox } from "@/features/entity-selectors"
import { toAutomationIdSegment } from "@/lib/automation-id"
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

export type QueueFilterPatch = Record<string, string | null>

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

const DUE_RADIO_FILTER_OPTIONS: ReadonlyArray<
    FixedOptionRadioFilterOption<DueFilter | "all">
> = [{ value: "all", label: "全部" }, ...DUE_FILTER_OPTIONS]

const GATE_RADIO_FILTER_OPTIONS: ReadonlyArray<
    FixedOptionRadioFilterOption<GateFilter | "all">
> = [{ value: "all", label: "全部" }, ...GATE_FILTER_OPTIONS]

/** chip 与摘要使用短文案；完整说明见面板单选选项。 */
const GATE_CHIP_LABELS: Record<GateFilter, string> = {
    satisfied: "货款已到",
    blocked: "先款未到",
}

const DUE_CHIP_LABELS: Record<DueFilter, string> = {
    today: "今日到期",
    overdue: "已超期",
}

/**
 * W09 队列筛选工具栏（第 1 / 2 层）。
 *
 * 第 0 层（类型）由页面 sticky 处理面渲染；本组件只负责：
 * - 第 1 层：搜索 +「更多筛选」开关
 * - 第 2 层：已生效 FilterChip 行 + 可折叠「更多筛选」面板
 * - actions：队列「自动下一项」
 * 整个筛选区域共用一个 <form>：收起态按 Enter 或点搜索框尾部箭头，
 * 展开态点面板底部「应用全部筛选」，都走同一个 applyFilters。
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
    /** 内部 ID 只进 URL：chip 与摘要展示仓库名称 */
    warehouseLabel,
    autoNext,
    showAutoNext,
    type,
    onPatch,
    onClearAllFilters,
    onAutoNextChange,
}: {
    q: string | undefined
    warehouseId: string | undefined
    due: DueFilter | undefined
    gate: GateFilter | undefined
    salesOrderId: string | undefined
    purchaseOrderId: string | undefined
    /** 内部 ID 只进 URL：chip 与摘要展示业务单号 */
    salesOrderNo: string | undefined
    purchaseNo: string | undefined
    warehouseLabel: string | undefined
    autoNext: boolean
    /** 只读角色不会连续处理，不显示自动下一项 */
    showAutoNext: boolean
    /** 单据类型筛选（slug，多值逗号分隔）；"all" 视为未激活 */
    type?: string | null
    onPatch: (patch: QueueFilterPatch) => void
    onClearAllFilters: () => void
    onAutoNextChange: (next: boolean) => void
}) {
    const panelId = React.useId()
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    // 三层状态：Applied 来自 URL（props），Draft 在本地，面板展开是 UI 态。
    // 关键词草稿在输入聚焦时不被 URL 回填覆盖（与列表页 useSearchDraft 同款保护）。
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
    // 初始深链带结构化条件时展开面板；后续 URL 回填不得再改展开态
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

    // `/` 聚焦搜索；输入框 / 文本域 / 弹层打开时不抢焦点
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

    /** 唯一提交路径：收起态 Enter / 搜索框尾部箭头 / 面板「应用全部筛选」。 */
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

    /** 只清除「更多筛选」；保留关键词和第 0 层类型，保持面板展开。 */
    const resetMoreFilters = React.useCallback(() => {
        setWarehouseIdDraft(null)
        setDueDraft("all")
        setGateDraft("all")
        onPatch({
            warehouseId: null,
            due: null,
            gate: null,
            currentOperationId: null,
        })
    }, [onPatch])

    /** 清空全部：URL、草稿、面板一次清干净；草稿随 URL 回填同步。 */
    const clearAllFilters = React.useCallback(() => {
        setPanelOpen(false)
        onClearAllFilters()
    }, [onClearAllFilters])

    /** 移除单个已生效条件；只动自己的参数。 */
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

    const hasActiveFilters = Boolean(
        q ||
        (type && type !== "all") ||
        warehouseId ||
        due ||
        gate ||
        salesOrderId ||
        purchaseOrderId,
    )

    /** 全部已生效条件都以 chip 显性展示，来源锁定参数也不例外。 */
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

    const hasChips = hasActiveFilters && appliedChips.length > 0

    return (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                applyFilters()
            }}
        >
            <ListToolbar
                aria-label="履约单据筛选"
                search={
                    <InputGroup>
                        <InputGroupAddon>
                            <SearchIcon aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            id="fulfillment-operations-queue-search"
                            ref={searchInputRef}
                            value={searchDraft}
                            onChange={(event) =>
                                setSearchDraft(event.target.value)
                            }
                            placeholder="销售单号、采购单号、客户、供应商"
                            aria-label="搜索履约单据"
                        />
                        {/* 面板展开时隐藏尾部提交箭头，只留面板底部唯一主按钮 */}
                    </InputGroup>
                }
                filters={
                    <Button
                        id="fulfillment-operations-queue-filters-trigger"
                        type="button"
                        variant="outline"
                        aria-expanded={panelOpen}
                        aria-controls={panelId}
                        onClick={() => setPanelOpen((open) => !open)}
                    >
                        <FilterIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        更多筛选
                        {hasStructuredFilters ? (
                            <Badge variant="info">已启用</Badge>
                        ) : null}
                        <ChevronDownIcon
                            data-icon="inline-end"
                            aria-hidden="true"
                            className={
                                panelOpen
                                    ? "rotate-180 transition-transform"
                                    : "transition-transform"
                            }
                        />
                    </Button>
                }
                secondary={
                    hasChips || panelOpen ? (
                        <div className="w-full space-y-3">
                            {hasChips ? (
                                <div className="flex flex-wrap items-center gap-2 border-t pt-3">
                                    <span className="text-xs text-muted-foreground">
                                        已筛选
                                    </span>
                                    {appliedChips.map((chip) => (
                                        <FilterChip
                                            key={chip.key}
                                            id={`fulfillment-operations-queue-filter-chip-${toAutomationIdSegment(chip.key)}`}
                                            label={chip.label}
                                            clearLabel={`移除${chip.label}`}
                                            onClear={() =>
                                                removeFilter(chip.key)
                                            }
                                        />
                                    ))}
                                    <Button
                                        id="fulfillment-operations-queue-clear-all"
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        onClick={clearAllFilters}
                                    >
                                        清空全部
                                    </Button>
                                </div>
                            ) : null}
                            {panelOpen ? (
                                <div
                                    id={panelId}
                                    className="flex w-full flex-col gap-3 border-t pt-3"
                                    aria-label="履约单据更多筛选条件"
                                >
                                    <FixedOptionRadioFilter
                                        id="fulfillment-operations-queue-due-filter"
                                        label="到期"
                                        value={dueDraft}
                                        onValueChange={setDueDraft}
                                        options={DUE_RADIO_FILTER_OPTIONS}
                                    />
                                    <FixedOptionRadioFilter
                                        id="fulfillment-operations-queue-gate-filter"
                                        label="货款情况"
                                        value={gateDraft}
                                        onValueChange={setGateDraft}
                                        options={GATE_RADIO_FILTER_OPTIONS}
                                    />
                                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                仓库
                                            </span>
                                            <WarehouseSearchCombobox
                                                id="fulfillment-operations-queue-warehouse-filter"
                                                className="w-full"
                                                value={
                                                    warehouseIdDraft ??
                                                    undefined
                                                }
                                                onValueChange={(id) =>
                                                    setWarehouseIdDraft(
                                                        id ?? null,
                                                    )
                                                }
                                                placeholder="全部仓库"
                                                aria-label="按仓库筛选（只对入库和发货有效）"
                                            />
                                        </div>
                                    </div>
                                    <div className="flex flex-col gap-3 border-t pt-3 sm:flex-row sm:items-center sm:justify-between">
                                        <p className="text-xs text-muted-foreground">
                                            将同时应用上方关键词和以下筛选条件；结果也用于导出。
                                        </p>
                                        <div className="flex flex-wrap items-center gap-2 sm:justify-end">
                                            <Button
                                                id="fulfillment-operations-queue-reset-more"
                                                type="button"
                                                variant="ghost"
                                                onClick={resetMoreFilters}
                                            >
                                                重置更多条件
                                            </Button>
                                            <Button
                                                id="fulfillment-operations-queue-apply-filters"
                                                type="submit"
                                            >
                                                <SearchIcon
                                                    data-icon="inline-start"
                                                    aria-hidden="true"
                                                />
                                                应用全部筛选
                                            </Button>
                                        </div>
                                    </div>
                                </div>
                            ) : null}
                        </div>
                    ) : undefined
                }
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
        </form>
    )
}
