"use client"

import * as React from "react"
import {
    CheckIcon,
    ChevronLeftIcon,
    ChevronRightIcon,
    ClipboardPasteIcon,
    CopyIcon,
    MapPinIcon,
    PencilIcon,
    PlusIcon,
    SearchIcon,
    Trash2Icon,
} from "lucide-react"
import { z } from "zod"

import {
    DetailWorkspaceSubTabs,
    QuantityValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { OptionCombobox } from "@/components/business/option-combobox"
import { toFieldErrors, useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    Popover,
    PopoverContent,
    PopoverDescription,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import {
    Sheet,
    SheetContent,
    SheetDescription,
    SheetFooter,
    SheetHeader,
    SheetTitle,
} from "@/components/ui/sheet"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { Textarea } from "@/components/ui/textarea"
import { toast } from "@/components/ui/toast"
import { RegionName, regionFullName, useRegionIndex } from "@/features/regions"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { downloadBlob } from "@/lib/download-blob"
import { compactFixed, compareDecimal, sumFixed } from "@/lib/fixed-decimal"
import type { DeliveryCoverageHint as CoverageCode } from "../api/delivery-coverage"
import type { InquiryDeliveryOption } from "../api/inquiry-basis"
import { useDeliveryCoverage } from "../hooks/use-delivery-coverage"
import { allocationSummary } from "../lib/sales-delivery-allocation-summary"
import {
    addressKey,
    buildDeliveryImportPreview,
    type DeliveryImportPreview,
} from "../lib/sales-delivery-import"
import {
    allocatedPercentForLine,
    applyDeliveryAddressToUnsetLines,
    remainingQuantityForLine,
    removeAddressAcrossOrder,
    replaceAddressAcrossOrder,
    summarizeOrderDelivery,
    type DeliveryAddressGroup,
} from "../lib/sales-delivery-pool"
import {
    createSalesDeliveryIssueWorkbook,
    readPastedDeliveryRows,
} from "../lib/sales-delivery-workbook"
import type { SalesOrderCreateFormApi } from "../lib/sales-order-create-form-types"
import type { SalesLineFilterKey } from "../lib/sales-order-create-line-health"
import {
    applyDeliveryAddressToLines,
    type UnifiedDeliveryAddress,
} from "../lib/sales-order-create-model"
import { decimalInput } from "../lib/sales-order-create-validation"
import type {
    SalesDeliveryAllocation,
    SalesOrderDraftLineInput,
} from "../types"
import { DeliveryCoverageHint } from "./delivery-coverage-hint"
import { DeliveryRegionField } from "./delivery-region-field"
import { InquiryDeliveryPane } from "./inquiry-delivery-pane"
import { SalesDeliveryImportPreview } from "./sales-delivery-import-preview"
import { SalesDeliverySmartPaste } from "./sales-delivery-smart-paste"

export type DeliveryWorkspaceTab = "product" | "overview"

const addressFormSchema = z.object({
    recipient: z
        .string()
        .trim()
        .min(1, "请填写收货人")
        .max(64, "收货人最多 64 个字"),
    phone: z
        .string()
        .trim()
        .min(1, "请填写联系电话")
        .max(32, "联系电话最多 32 个字符"),
    region_code: z.string().min(1, "请选择收货地区（到区县）"),
    address: z
        .string()
        .trim()
        .min(1, "请填写详细地址")
        .max(512, "详细地址最多 512 个字"),
    quantity: decimalInput("分配数量", 6, { positive: true }),
})

const overviewEditSchema = addressFormSchema.omit({ quantity: true })

export type SalesDeliveryWorkspaceSheetProps = {
    open: boolean
    unitNames: ReadonlyMap<string, string>
    onOpenChange: (open: boolean) => void
    tab: DeliveryWorkspaceTab
    onTabChange: (tab: DeliveryWorkspaceTab) => void
    activeRowKey: string | null
    onActiveRowKeyChange: (rowKey: string | null) => void
    form: SalesOrderCreateFormApi
    lines: SalesOrderDraftLineInput[]
    inquiryDelivery?: ReadonlyMap<string, readonly InquiryDeliveryOption[]>
    onJumpToTableFilter?: (filter: SalesLineFilterKey) => void
}

/**
 * 右侧半屏收货安排工作台：
 * - 「按商品分配」：支持上/下一项快速切换、单品多地址表、行内调数量、智能粘贴识别与本单地址库一键复用；
 * - 「按地址总览」：跨 100 项商品自动去重，支持一处改地址同步更新全部关联商品、一键补齐未设地址商品。
 */
export function SalesDeliveryWorkspaceSheet({
    open,
    unitNames,
    onOpenChange,
    tab,
    onTabChange,
    activeRowKey,
    onActiveRowKeyChange,
    form,
    lines,
    inquiryDelivery,
    onJumpToTableFilter,
}: SalesDeliveryWorkspaceSheetProps) {
    const overview = React.useMemo(() => summarizeOrderDelivery(lines), [lines])
    const excludedQuantities = React.useRef(new Map<string, string>())
    const coverageHint = useDeliveryCoverage(open ? lines : [])

    // 确保按商品视图始终指向一个有效商品行。
    const activeLine =
        lines.find((line) => line.rowKey === activeRowKey) ?? lines[0]
    const effectiveRowKey = activeLine?.rowKey ?? null

    return (
        <Sheet open={open} onOpenChange={onOpenChange}>
            <SheetContent
                size="detail"
                closeButtonId="sales-delivery-workspace-close"
            >
                <SheetHeader className="border-b border-border/60 pb-3">
                    <div className="space-y-1 pr-8">
                        <SheetTitle>
                            {inquiryDelivery ? "询价配送分配" : "收货安排"}
                        </SheetTitle>
                        <SheetDescription className="num">
                            共 {overview.totalLines} 项商品 ·{" "}
                            {inquiryDelivery
                                ? `本单转入 ${overview.uniqueAddressCount} 个地址`
                                : `${overview.uniqueAddressCount} 个收货地址 · 已完整 ${overview.completeLineCount}/${overview.totalLines} 项`}
                            {!inquiryDelivery &&
                            overview.incompleteLineCount > 0
                                ? ` · ${overview.incompleteLineCount} 项待核对`
                                : ""}
                            {!inquiryDelivery && overview.unsetLineCount > 0
                                ? ` · ${overview.unsetLineCount} 项未设置`
                                : ""}
                        </SheetDescription>
                    </div>
                    {!inquiryDelivery ? (
                        <DetailWorkspaceSubTabs
                            value={tab}
                            onValueChange={(next) =>
                                onTabChange(next as DeliveryWorkspaceTab)
                            }
                            ariaLabel="收货安排视图"
                            idPrefix="sales-delivery-workspace-tab"
                            className="pt-0"
                            items={[
                                {
                                    id: "product",
                                    label: "按商品分配",
                                    badge: (
                                        <span className="num rounded-sm bg-muted px-1.5 text-tiny text-muted-foreground">
                                            {lines.length}
                                        </span>
                                    ),
                                },
                                {
                                    id: "overview",
                                    label: "按地址总览",
                                    badge: (
                                        <span className="num rounded-sm bg-muted px-1.5 text-tiny text-muted-foreground">
                                            {overview.uniqueAddressCount}
                                        </span>
                                    ),
                                },
                            ]}
                        />
                    ) : null}
                </SheetHeader>

                {inquiryDelivery && activeLine ? (
                    <InquiryDeliveryPane
                        form={form}
                        lines={lines}
                        line={activeLine}
                        displayUnit={unitNames.get(activeLine.unit) ?? ""}
                        options={inquiryDelivery.get(activeLine.rowKey) ?? []}
                        excludedQuantities={excludedQuantities.current}
                        coverageHint={coverageHint}
                        onSelectLine={onActiveRowKeyChange}
                    />
                ) : (
                    <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
                        {tab === "product" ? (
                            effectiveRowKey ? (
                                <ProductDeliveryPane
                                    unitNames={unitNames}
                                    form={form}
                                    lines={lines}
                                    activeRowKey={effectiveRowKey}
                                    onSelectRowKey={onActiveRowKeyChange}
                                    addressPool={overview.groups}
                                    coverageHint={coverageHint}
                                />
                            ) : (
                                <p className="py-12 text-center text-xs text-muted-foreground">
                                    请先在销售明细中添加商品。
                                </p>
                            )
                        ) : (
                            <AddressOverviewPane
                                form={form}
                                lines={lines}
                                overview={overview}
                                onInspectLine={(rowKey) => {
                                    onActiveRowKeyChange(rowKey)
                                    onTabChange("product")
                                }}
                                onFilterUnset={() => {
                                    onOpenChange(false)
                                    onJumpToTableFilter?.("no_address")
                                }}
                            />
                        )}
                    </div>
                )}

                <SheetFooter className="border-t border-border/60">
                    <p className="mr-auto text-xs text-muted-foreground">
                        {inquiryDelivery
                            ? "勾选和数量会同步到销售明细。"
                            : "修改会实时同步到当前销售单表单，关闭后可继续保存或提交。"}
                    </p>
                    <Button
                        id="sales-delivery-workspace-done"
                        type="button"
                        size="sm"
                        onClick={() => onOpenChange(false)}
                    >
                        完成
                    </Button>
                </SheetFooter>
            </SheetContent>
        </Sheet>
    )
}

function ProductDeliveryPane({
    form,
    unitNames,
    lines,
    activeRowKey,
    onSelectRowKey,
    addressPool,
    coverageHint,
}: {
    form: SalesOrderCreateFormApi
    lines: SalesOrderDraftLineInput[]
    activeRowKey: string
    unitNames: ReadonlyMap<string, string>
    onSelectRowKey: (rowKey: string) => void
    addressPool: readonly DeliveryAddressGroup[]
    coverageHint: (sku: string, regionCode: string) => CoverageCode
}) {
    const regions = useRegionIndex()
    const [onlyPendingNav, setOnlyPendingNav] = React.useState(false)
    const [addressSearch, setAddressSearch] = React.useState("")
    const [editingState, setEditingState] = React.useState<
        | { mode: "add" }
        | { mode: "edit"; index: number; initial: SalesDeliveryAllocation }
        | null
    >(null)
    const [pastePanelOpen, setPastePanelOpen] = React.useState(false)
    const [reusePopoverOpen, setReusePopoverOpen] = React.useState(false)

    // 切换商品时收起当前正在编辑的子表单与搜索条件。
    React.useEffect(() => {
        setEditingState(null)
        setPastePanelOpen(false)
        setAddressSearch("")
    }, [activeRowKey])

    const lineIndex = lines.findIndex((line) => line.rowKey === activeRowKey)
    const line = lines[lineIndex]

    const navLines = React.useMemo(() => {
        if (!onlyPendingNav) return lines
        const filtered = lines.filter(
            (item) =>
                item.rowKey === activeRowKey ||
                (item.deliveryAllocations?.length ?? 0) === 0 ||
                !allocationSummary(item).complete,
        )
        return filtered.length > 0 ? filtered : lines
    }, [activeRowKey, lines, onlyPendingNav])

    const navPos = navLines.findIndex((item) => item.rowKey === activeRowKey)
    const prevLine = navPos > 0 ? navLines[navPos - 1] : undefined
    const nextLine =
        navPos >= 0 && navPos < navLines.length - 1
            ? navLines[navPos + 1]
            : undefined

    const productOptions = React.useMemo(
        () =>
            lines.map((item, idx) => {
                const count = item.deliveryAllocations?.length ?? 0
                const done = count > 0 && allocationSummary(item).complete
                const tag =
                    count === 0 ? "未设地址" : done ? `${count} 地址` : "待核对"
                return {
                    value: item.rowKey,
                    label: `#${idx + 1} ${item.name || "未命名商品"}`,
                    description: `${item.quantity} ${unitNames.get(item.unit.trim()) ?? ""} · ${tag}`,
                }
            }),
        [lines, unitNames],
    )

    if (!line || lineIndex < 0) return null

    const unitName = unitNames.get(line.unit.trim()) ?? ""
    const rows = line.deliveryAllocations ?? []
    const summary = allocationSummary({ ...line, unit: unitName })
    const remaining = remainingQuantityForLine(line.quantity, rows)
    const pct = allocatedPercentForLine(line.quantity, rows)
    const prefix = `sales-delivery-${toAutomationIdSegment(line.rowKey)}`

    const updateAllocations = (nextRows: SalesDeliveryAllocation[]) => {
        const currentIndex = form
            .getFieldValue("lineItems")
            .findIndex((item) => item.rowKey === line.rowKey)
        if (currentIndex < 0) return
        form.setFieldValue(
            `lineItems[${currentIndex}].deliveryAllocations`,
            nextRows,
        )
    }

    const existingKeys = new Set(rows.map((row) => addressKey(row)))
    const reusablePool = addressPool.filter(
        (group) => !existingKeys.has(group.key),
    )

    const visibleRows = rows
        .map((row, index) => ({ row, index }))
        .filter(({ row }) => {
            const needle = addressSearch.trim().toLowerCase()
            if (!needle) return true
            const regionText =
                row.region_name ||
                (regions.data && row.region_code
                    ? regionFullName(regions.data, row.region_code)
                    : "")
            return [row.recipient, row.phone, regionText, row.address].some(
                (field) => field.toLowerCase().includes(needle),
            )
        })

    const pendingCount = lines.filter(
        (item) =>
            (item.deliveryAllocations?.length ?? 0) === 0 ||
            !allocationSummary(item).complete,
    ).length

    const handleReuseFromPool = (group: DeliveryAddressGroup) => {
        if (rows.length >= 100) {
            toast.add({
                title: "已达地址上限",
                description: "每项商品最多分配 100 个收货地址。",
                type: "warning",
            })
            return
        }
        const qty = rows.length === 0 ? line.quantity : remaining || "1"
        updateAllocations([
            ...rows,
            {
                recipient: group.recipient,
                phone: group.phone,
                region_code: group.region_code,
                region_name: group.region_name,
                address: group.address,
                quantity: qty,
            },
        ])
        setReusePopoverOpen(false)
        toast.add({
            title: "已复用收货地址",
            description: `已为「${line.name || `商品 ${lineIndex + 1}`}」添加 ${group.recipient} 的收货地址。`,
            type: "success",
            timeout: 2500,
        })
    }

    const handleFillRemainingOnRow = (targetIndex: number) => {
        if (!remaining) return
        const row = rows[targetIndex]
        if (!row) return
        try {
            const nextQty = compactFixed(
                sumFixed([row.quantity || "0", remaining], {
                    maxScale: 6,
                    outputScale: 6,
                }),
            )
            updateAllocations(
                rows.map((item, idx) =>
                    idx === targetIndex ? { ...item, quantity: nextQty } : item,
                ),
            )
        } catch {
            updateAllocations(
                rows.map((item, idx) =>
                    idx === targetIndex
                        ? { ...item, quantity: remaining }
                        : item,
                ),
            )
        }
    }

    return (
        <div className="space-y-4">
            {/* 商品快速切换条：支持 100 项商品连续配置而不关闭抽屉 */}
            <div className="space-y-2.5 rounded-lg border border-border/75 bg-surface-sunken/70 p-3">
                <div className="flex flex-wrap items-center gap-2">
                    <div className="min-w-0 flex-1">
                        <OptionCombobox
                            id="sales-delivery-workspace-product-jump"
                            aria-label="切换商品"
                            allowClear={false}
                            options={productOptions}
                            value={line.rowKey}
                            onValueChange={(next) => {
                                if (next) onSelectRowKey(next)
                            }}
                            placeholder="搜索并跳转商品…"
                        />
                    </div>
                    <div className="flex shrink-0 items-center gap-1">
                        <Button
                            id="sales-delivery-workspace-prev-product"
                            type="button"
                            variant="outline"
                            size="icon-sm"
                            aria-label="上一项商品"
                            disabled={!prevLine}
                            onClick={() => {
                                if (prevLine) onSelectRowKey(prevLine.rowKey)
                            }}
                        >
                            <ChevronLeftIcon aria-hidden="true" />
                        </Button>
                        <span className="num px-1.5 text-xs text-muted-foreground">
                            {lineIndex + 1} / {lines.length}
                        </span>
                        <Button
                            id="sales-delivery-workspace-next-product"
                            type="button"
                            variant="outline"
                            size="icon-sm"
                            aria-label="下一项商品"
                            disabled={!nextLine}
                            onClick={() => {
                                if (nextLine) onSelectRowKey(nextLine.rowKey)
                            }}
                        >
                            <ChevronRightIcon aria-hidden="true" />
                        </Button>
                    </div>
                </div>

                <div className="flex flex-wrap items-center justify-between gap-2 text-xs">
                    <div className="flex min-w-0 flex-wrap items-center gap-x-2.5 gap-y-0.5 text-muted-foreground">
                        {line.specification ? (
                            <span className="truncate">
                                {line.specification}
                            </span>
                        ) : null}
                        <span>
                            销售数量{" "}
                            <QuantityValue
                                value={line.quantity}
                                unit={unitName}
                                className="font-medium text-foreground"
                            />
                        </span>
                    </div>
                    {pendingCount > 0 && lines.length > 1 ? (
                        <Button
                            id="sales-delivery-workspace-only-pending"
                            type="button"
                            variant="ghost"
                            size="xs"
                            aria-pressed={onlyPendingNav}
                            className={
                                onlyPendingNav
                                    ? "bg-sidebar-accent text-sidebar-accent-foreground hover:bg-sidebar-accent"
                                    : "text-muted-foreground hover:text-foreground"
                            }
                            onClick={() => setOnlyPendingNav((v) => !v)}
                        >
                            {onlyPendingNav ? (
                                <CheckIcon
                                    aria-hidden="true"
                                    className="size-3"
                                />
                            ) : null}
                            仅切换待完善/未设项（{pendingCount}）
                        </Button>
                    ) : null}
                </div>
            </div>

            {/* 分配进度条 */}
            <div className="space-y-2 border-b border-border/60 pb-3">
                <div className="flex flex-wrap items-center justify-between gap-2">
                    <div className="flex flex-wrap items-center gap-2">
                        <span className="num text-body-compact font-medium text-foreground">
                            {rows.length > 0
                                ? summary.summary
                                : "暂未设置收货地址"}
                        </span>
                        {rows.length > 0 ? (
                            <StatusBadge
                                tone={summary.complete ? "success" : "warning"}
                                appearance="quiet"
                                label={
                                    summary.complete
                                        ? "已分配完整"
                                        : summary.status || "待完善"
                                }
                            />
                        ) : (
                            <StatusBadge
                                tone="neutral"
                                appearance="quiet"
                                label="选填"
                            />
                        )}
                    </div>
                    <span className="num text-xs text-muted-foreground">
                        {rows.length} / 100 个地址
                    </span>
                </div>
                {rows.length > 0 ? (
                    <div
                        aria-hidden="true"
                        className="h-1.5 w-full overflow-hidden rounded-full bg-muted"
                    >
                        <div
                            className={
                                summary.complete
                                    ? "h-full rounded-full bg-success transition-all"
                                    : "h-full rounded-full bg-warning transition-all"
                            }
                            style={{ width: `${pct}%` }}
                        />
                    </div>
                ) : null}
            </div>

            <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex flex-wrap items-center gap-1.5">
                    <Button
                        id={`${prefix}-add`}
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={rows.length >= 100}
                        onClick={() => {
                            setPastePanelOpen(false)
                            setEditingState({ mode: "add" })
                        }}
                    >
                        <PlusIcon
                            data-icon="inline-start"
                            className="size-3.5"
                            aria-hidden="true"
                        />
                        添加收货地址
                    </Button>

                    {reusablePool.length > 0 ? (
                        <Popover
                            open={reusePopoverOpen}
                            onOpenChange={setReusePopoverOpen}
                        >
                            <PopoverTrigger
                                id={`${prefix}-reuse-pool`}
                                render={
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="sm"
                                        className={
                                            detailWorkspaceStyles.subtleButton
                                        }
                                    />
                                }
                                disabled={rows.length >= 100}
                            >
                                <CopyIcon
                                    data-icon="inline-start"
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                                从本单地址库选用（{reusablePool.length}）
                            </PopoverTrigger>
                            <PopoverContent
                                align="start"
                                className="w-96 max-w-[calc(100vw-2rem)]"
                            >
                                <PopoverTitle>本单已有收货地址</PopoverTitle>
                                <PopoverDescription>
                                    点击直接添加到当前商品，默认填入剩余待分数量。
                                </PopoverDescription>
                                <div className="max-h-64 space-y-1 overflow-y-auto pt-1">
                                    {reusablePool.map((group) => (
                                        <button
                                            key={group.key}
                                            id={`${prefix}-reuse-${toAutomationIdSegment(group.key).slice(0, 32)}`}
                                            type="button"
                                            onClick={() =>
                                                handleReuseFromPool(group)
                                            }
                                            className="flex w-full cursor-pointer items-start justify-between gap-2 rounded-md px-2.5 py-2 text-left transition-colors hover:bg-muted focus-visible:outline-2 focus-visible:outline-ring"
                                        >
                                            <div className="min-w-0 space-y-0.5">
                                                <p className="truncate text-body-compact font-medium text-foreground">
                                                    {group.recipient}
                                                    <span className="num ml-2 font-normal text-muted-foreground">
                                                        {group.phone}
                                                    </span>
                                                </p>
                                                <p className="truncate text-xs text-muted-foreground">
                                                    <RegionName
                                                        code={group.region_code}
                                                    />
                                                    {group.address}
                                                </p>
                                            </div>
                                            <span className="num shrink-0 rounded-sm bg-muted/80 px-1.5 py-0.5 text-tiny text-muted-foreground">
                                                {group.lineCount} 项
                                            </span>
                                        </button>
                                    ))}
                                </div>
                            </PopoverContent>
                        </Popover>
                    ) : null}

                    <Button
                        id={`${prefix}-paste-toggle`}
                        type="button"
                        variant="ghost"
                        size="sm"
                        aria-expanded={pastePanelOpen}
                        className={detailWorkspaceStyles.subtleButton}
                        onClick={() => {
                            setEditingState(null)
                            setPastePanelOpen((v) => !v)
                        }}
                    >
                        <ClipboardPasteIcon
                            data-icon="inline-start"
                            className="size-3.5"
                            aria-hidden="true"
                        />
                        批量粘贴多行
                    </Button>
                </div>

                {rows.length > 0 ? (
                    <Button
                        id={`${prefix}-clear-all`}
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.quietDangerButton}
                        onClick={() => {
                            updateAllocations([])
                            setEditingState(null)
                        }}
                    >
                        清空本商品地址
                    </Button>
                ) : null}
            </div>

            {/* 内联新增/编辑单个地址面板 */}
            {editingState ? (
                <SingleAddressInlineForm
                    unitName={unitName}
                    key={
                        editingState.mode === "edit"
                            ? `edit-${editingState.index}`
                            : "add"
                    }
                    prefix={prefix}
                    line={line}
                    mode={editingState.mode}
                    initial={
                        editingState.mode === "edit"
                            ? editingState.initial
                            : {
                                  recipient: "",
                                  phone: "",
                                  region_code: "",
                                  address: "",
                                  quantity:
                                      rows.length === 0
                                          ? line.quantity
                                          : remaining,
                              }
                    }
                    remaining={remaining}
                    onCancel={() => setEditingState(null)}
                    onSave={(saved) => {
                        if (editingState.mode === "edit") {
                            updateAllocations(
                                rows.map((row, idx) =>
                                    idx === editingState.index ? saved : row,
                                ),
                            )
                        } else {
                            updateAllocations([...rows, saved])
                        }
                        setEditingState(null)
                    }}
                />
            ) : null}

            {/* 内联批量粘贴多行 Excel 地址（针对当前单个商品） */}
            {pastePanelOpen ? (
                <SingleProductBulkPastePanel
                    unitNames={unitNames}
                    prefix={prefix}
                    line={line}
                    form={form}
                    onClose={() => setPastePanelOpen(false)}
                    onApplied={(allocations) => {
                        updateAllocations(allocations)
                        setPastePanelOpen(false)
                        toast.add({
                            title: "已更新当前商品收货地址",
                            description: `当前共 ${allocations.length} 个收货地址。`,
                            type: "success",
                            timeout: 3000,
                        })
                    }}
                />
            ) : null}

            {/* 地址较多时提供搜索过滤（支持单品最多 100 个地址） */}
            {rows.length > 6 ? (
                <div className="relative">
                    <SearchIcon
                        className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                        aria-hidden="true"
                    />
                    <Input
                        id={`${prefix}-address-search`}
                        value={addressSearch}
                        onChange={(event) =>
                            setAddressSearch(event.target.value)
                        }
                        placeholder="按收货人、电话或地址筛选…"
                        className="h-control-sm pl-8"
                    />
                </div>
            ) : null}

            {/* 地址列表 */}
            {rows.length === 0 ? (
                <div className="flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed border-border/80 bg-surface-sunken/50 px-6 py-10 text-center">
                    <MapPinIcon
                        className="size-5 text-muted-foreground"
                        aria-hidden="true"
                    />
                    <p className="font-medium text-foreground">
                        该商品暂未设置收货地址
                    </p>
                    <p className="max-w-sm text-xs text-muted-foreground">
                        可在建单时直接添加或复用本单地址，也可留空在履约前补充。
                    </p>
                </div>
            ) : (
                <div
                    data-slot="detail-inline-table"
                    className="overflow-hidden rounded-lg border border-border/75"
                >
                    <Table>
                        <TableHeader>
                            <TableRow>
                                <TableHead>收货信息</TableHead>
                                <TableHead data-align="end" className="w-36">
                                    分配数量
                                </TableHead>
                                <TableHead className="w-24 text-right">
                                    操作
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {visibleRows.length === 0 ? (
                                <TableRow>
                                    <TableCell
                                        colSpan={3}
                                        className="py-8 text-center text-xs text-muted-foreground"
                                    >
                                        未找到匹配的收货地址。
                                    </TableCell>
                                </TableRow>
                            ) : (
                                visibleRows.map(({ row, index }) => {
                                    const rowPrefix = `${prefix}-addr-${index + 1}`
                                    const hint = coverageHint(
                                        line.sku,
                                        row.region_code,
                                    )
                                    const qtyInvalid = (() => {
                                        try {
                                            return (
                                                compareDecimal(
                                                    row.quantity || "0",
                                                    "0",
                                                    6,
                                                ) <= 0
                                            )
                                        } catch {
                                            return true
                                        }
                                    })()
                                    return (
                                        <TableRow
                                            key={`${addressKey(row)}-${index}`}
                                        >
                                            <TableCell className="whitespace-normal py-2.5 align-top">
                                                <div className="space-y-0.5">
                                                    <p className="font-medium text-foreground">
                                                        {row.recipient ||
                                                            "待填收货人"}
                                                        <span className="num ml-2 font-normal text-muted-foreground">
                                                            {row.phone ||
                                                                "待填电话"}
                                                        </span>
                                                    </p>
                                                    <p className="break-words text-xs text-muted-foreground">
                                                        {row.region_name ? (
                                                            row.region_name
                                                        ) : row.region_code ? (
                                                            <RegionName
                                                                code={
                                                                    row.region_code
                                                                }
                                                            />
                                                        ) : (
                                                            "未选地区"
                                                        )}
                                                        {row.address}
                                                    </p>
                                                    {hint !== "NONE" ? (
                                                        <DeliveryCoverageHint
                                                            id={`${rowPrefix}-coverage`}
                                                            hint={hint}
                                                        />
                                                    ) : null}
                                                </div>
                                            </TableCell>
                                            <TableCell
                                                data-align="end"
                                                className="py-2.5 align-top"
                                            >
                                                <div className="flex flex-col items-end gap-1">
                                                    <div className="flex items-center justify-end gap-1">
                                                        <Input
                                                            id={`${rowPrefix}-quantity`}
                                                            aria-label={`${row.recipient || `地址 ${index + 1}`}分配数量`}
                                                            aria-invalid={
                                                                qtyInvalid ||
                                                                undefined
                                                            }
                                                            type="text"
                                                            inputMode="decimal"
                                                            value={row.quantity}
                                                            className="h-control-sm num w-20 text-right"
                                                            onChange={(
                                                                event,
                                                            ) => {
                                                                const nextVal =
                                                                    event.target
                                                                        .value
                                                                updateAllocations(
                                                                    rows.map(
                                                                        (
                                                                            item,
                                                                            idx,
                                                                        ) =>
                                                                            idx ===
                                                                            index
                                                                                ? {
                                                                                      ...item,
                                                                                      quantity:
                                                                                          nextVal,
                                                                                  }
                                                                                : item,
                                                                    ),
                                                                )
                                                            }}
                                                        />
                                                        {unitName ? (
                                                            <span className="text-xs text-muted-foreground">
                                                                {unitName}
                                                            </span>
                                                        ) : null}
                                                    </div>
                                                    {remaining ? (
                                                        <button
                                                            id={`${rowPrefix}-fill-remaining`}
                                                            type="button"
                                                            className="cursor-pointer text-xs text-primary hover:underline"
                                                            onClick={() =>
                                                                handleFillRemainingOnRow(
                                                                    index,
                                                                )
                                                            }
                                                        >
                                                            +{remaining} 补齐
                                                        </button>
                                                    ) : null}
                                                </div>
                                            </TableCell>
                                            <TableCell className="py-2.5 text-right align-top">
                                                <div className="flex items-center justify-end gap-0.5">
                                                    <Button
                                                        id={`${rowPrefix}-edit`}
                                                        type="button"
                                                        variant="ghost"
                                                        size="icon-sm"
                                                        aria-label={`编辑 ${row.recipient || `地址 ${index + 1}`}`}
                                                        className={
                                                            detailWorkspaceStyles.quietButton
                                                        }
                                                        onClick={() => {
                                                            setPastePanelOpen(
                                                                false,
                                                            )
                                                            setEditingState({
                                                                mode: "edit",
                                                                index,
                                                                initial: row,
                                                            })
                                                        }}
                                                    >
                                                        <PencilIcon
                                                            className="size-3.5"
                                                            aria-hidden="true"
                                                        />
                                                    </Button>
                                                    <Button
                                                        id={`${rowPrefix}-remove`}
                                                        type="button"
                                                        variant="ghost"
                                                        size="icon-sm"
                                                        aria-label={`移除 ${row.recipient || `地址 ${index + 1}`}`}
                                                        className={
                                                            detailWorkspaceStyles.quietDangerButton
                                                        }
                                                        onClick={() =>
                                                            updateAllocations(
                                                                rows.filter(
                                                                    (_, idx) =>
                                                                        idx !==
                                                                        index,
                                                                ),
                                                            )
                                                        }
                                                    >
                                                        <Trash2Icon
                                                            className="size-3.5"
                                                            aria-hidden="true"
                                                        />
                                                    </Button>
                                                </div>
                                            </TableCell>
                                        </TableRow>
                                    )
                                })
                            )}
                        </TableBody>
                    </Table>
                </div>
            )}
        </div>
    )
}

function SingleAddressInlineForm({
    unitName,
    prefix,
    line,
    mode,
    initial,
    remaining,
    onCancel,
    onSave,
}: {
    unitName: string
    prefix: string
    line: SalesOrderDraftLineInput
    mode: "add" | "edit"
    initial: SalesDeliveryAllocation
    remaining: string
    onCancel: () => void
    onSave: (value: SalesDeliveryAllocation) => void
}) {
    const regions = useRegionIndex()
    const form = useAppForm({
        defaultValues: {
            recipient: initial.recipient,
            phone: initial.phone,
            region_code: initial.region_code,
            address: initial.address,
            quantity: initial.quantity,
        },
        validators: { onChange: addressFormSchema },
        onSubmit: ({ value }) => {
            onSave({
                recipient: value.recipient.trim(),
                phone: value.phone.trim(),
                region_code: value.region_code,
                region_name:
                    value.region_code && regions.data
                        ? regionFullName(regions.data, value.region_code)
                        : undefined,
                address: value.address.trim(),
                quantity: value.quantity.trim(),
            })
        },
    })

    const formPrefix = `${prefix}-inline`

    return (
        <div className="space-y-3 rounded-lg border border-primary/30 bg-card p-4 shadow-2xs">
            <div className="flex items-center justify-between gap-2">
                <h3 className="font-semibold text-foreground">
                    {mode === "edit" ? "编辑收货地址" : "新增收货地址"}
                </h3>
                <Button
                    id={`${formPrefix}-cancel-top`}
                    type="button"
                    variant="ghost"
                    size="xs"
                    className={detailWorkspaceStyles.quietButton}
                    onClick={onCancel}
                >
                    收起
                </Button>
            </div>

            <SalesDeliverySmartPaste
                idPrefix={formPrefix}
                onParsed={(parsed) => {
                    if (parsed.recipient)
                        form.setFieldValue("recipient", parsed.recipient)
                    if (parsed.phone) form.setFieldValue("phone", parsed.phone)
                    if (parsed.region_code)
                        form.setFieldValue("region_code", parsed.region_code)
                    if (parsed.address)
                        form.setFieldValue("address", parsed.address)
                }}
            />

            <form
                data-slot="detail-form-grid"
                className="space-y-3"
                noValidate
                onSubmit={(event) => {
                    event.preventDefault()
                    event.stopPropagation()
                    void form.handleSubmit()
                }}
            >
                <div className="grid gap-3 sm:grid-cols-2">
                    <form.AppField name="recipient">
                        {(field) => (
                            <field.TextField
                                id={`${formPrefix}-recipient`}
                                label="收货人"
                                required
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="phone">
                        {(field) => (
                            <field.TextField
                                id={`${formPrefix}-phone`}
                                label="联系电话"
                                type="tel"
                                required
                            />
                        )}
                    </form.AppField>
                </div>

                <form.AppField name="region_code">
                    {(field) => (
                        <DeliveryRegionField
                            id={`${formPrefix}-region`}
                            required
                            value={field.state.value}
                            onValueChange={(code) => field.handleChange(code)}
                            onBlur={field.handleBlur}
                            invalid={
                                field.state.meta.isTouched &&
                                !field.state.meta.isValid
                            }
                            errors={toFieldErrors(field.state.meta.errors)}
                            hint={
                                field.state.value ? (
                                    <InlineCoverageHint
                                        sku={line.sku}
                                        regionCode={field.state.value}
                                        id={`${formPrefix}-coverage`}
                                    />
                                ) : null
                            }
                        />
                    )}
                </form.AppField>

                <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_10rem]">
                    <form.AppField name="address">
                        {(field) => (
                            <field.TextField
                                id={`${formPrefix}-address`}
                                label="详细地址"
                                placeholder="街道、门牌号"
                                required
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="quantity">
                        {(field) => (
                            <div className="space-y-1">
                                <field.TextField
                                    id={`${formPrefix}-quantity`}
                                    label={`分配数量（${unitName || "单位"}）`}
                                    type="text"
                                    inputMode="decimal"
                                    inputClassName="num text-right"
                                    required
                                />
                                {mode === "add" && remaining ? (
                                    <button
                                        id={`${formPrefix}-fill-remaining`}
                                        type="button"
                                        className="cursor-pointer text-xs text-primary hover:underline"
                                        onClick={() =>
                                            field.handleChange(remaining)
                                        }
                                    >
                                        填入剩余 {remaining} {unitName}
                                    </button>
                                ) : null}
                            </div>
                        )}
                    </form.AppField>
                </div>

                <div className="flex justify-end gap-2 pt-1">
                    <Button
                        id={`${formPrefix}-cancel`}
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={onCancel}
                    >
                        取消
                    </Button>
                    <form.AppForm>
                        <form.SubmitButton
                            id={`${formPrefix}-submit`}
                            size="sm"
                            label={mode === "edit" ? "保存修改" : "确认添加"}
                        />
                    </form.AppForm>
                </div>
            </form>
        </div>
    )
}

function InlineCoverageHint({
    sku,
    regionCode,
    id,
}: {
    sku: string
    regionCode: string
    id: string
}) {
    const coverageHint = useDeliveryCoverage([
        { sku, deliveryAllocations: [{ region_code: regionCode }] },
    ])
    return <DeliveryCoverageHint id={id} hint={coverageHint(sku, regionCode)} />
}

function SingleProductBulkPastePanel({
    unitNames,
    prefix,
    line,
    form,
    onClose,
    onApplied,
}: {
    unitNames: ReadonlyMap<string, string>
    prefix: string
    line: SalesOrderDraftLineInput
    form: SalesOrderCreateFormApi
    onClose: () => void
    onApplied: (allocations: SalesDeliveryAllocation[]) => void
}) {
    const regions = useRegionIndex()
    const [text, setText] = React.useState("")
    const [busy, setBusy] = React.useState(false)
    const [error, setError] = React.useState("")
    const [preview, setPreview] = React.useState<DeliveryImportPreview | null>(
        null,
    )

    const parsePaste = async (fillOne = false) => {
        if (!regions.data || busy) return
        setBusy(true)
        setError("")
        setPreview(null)
        try {
            const context = {
                scope: "single-product-paste",
                unitNames,
                customerId: form.getFieldValue("customerId"),
                lines: form.getFieldValue("lineItems"),
            }
            const source = readPastedDeliveryRows(text, context, line.rowKey)
            if (!source.rows.length) {
                throw new Error("请粘贴至少一行有效地址")
            }
            const result = await buildDeliveryImportPreview(
                source,
                context.lines,
                regions.data,
                fillOne,
                context.unitNames,
            )
            setPreview(result)
        } catch (reason) {
            setError(getErrorMessage(reason))
        } finally {
            setBusy(false)
        }
    }

    const group = preview?.groups.find(
        (entry) => entry.line.rowKey === line.rowKey,
    )
    const canApply = Boolean(
        preview &&
        !preview.issues.length &&
        group &&
        group.changes.length > 0 &&
        group.summary.complete,
    )

    return (
        <div className="space-y-3 rounded-lg border border-border/80 bg-surface-sunken/70 p-4">
            <div className="flex items-center justify-between gap-2">
                <div>
                    <h3 className="font-semibold text-foreground">
                        批量粘贴地址行（当前商品）
                    </h3>
                    <p className="text-xs text-muted-foreground">
                        从 Excel 复制 6 或 7
                        列直接粘贴：收货人、联系电话、省、市、区县、详细地址、配送数量（选填）。
                    </p>
                </div>
                <Button
                    id={`${prefix}-paste-close`}
                    type="button"
                    variant="ghost"
                    size="xs"
                    className={detailWorkspaceStyles.quietButton}
                    onClick={onClose}
                >
                    收起
                </Button>
            </div>
            <Textarea
                id={`${prefix}-paste-textarea`}
                rows={4}
                value={text}
                disabled={busy}
                placeholder={
                    "张三\t13800138001\t北京市\t市辖区\t朝阳区\t测试路 1 号\t10\n李四\t13900139002\t广东省\t深圳市\t南山区\t科技园 8 号\t20"
                }
                className="bg-card text-body-compact"
                onChange={(event) => {
                    setText(event.target.value)
                    setPreview(null)
                    setError("")
                }}
            />
            {error ? (
                <p role="alert" className="text-xs text-destructive">
                    {error}
                </p>
            ) : null}
            <div className="flex flex-wrap items-center gap-2">
                <Button
                    id={`${prefix}-paste-preview`}
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={!text.trim() || !regions.data || busy}
                    onClick={() => void parsePaste(false)}
                >
                    {busy ? "解析中…" : "解析预览"}
                </Button>
                <Button
                    id={`${prefix}-paste-preview-fill-one`}
                    type="button"
                    variant="ghost"
                    size="sm"
                    className={detailWorkspaceStyles.subtleButton}
                    disabled={!text.trim() || !regions.data || busy}
                    onClick={() => void parsePaste(true)}
                >
                    空白数量按 1 解析
                </Button>
                {canApply && group ? (
                    <Button
                        id={`${prefix}-paste-confirm`}
                        type="button"
                        size="sm"
                        className="ml-auto"
                        onClick={() => onApplied(group.allocations)}
                    >
                        应用 {group.allocations.length} 个地址
                    </Button>
                ) : null}
            </div>
            {preview && regions.data ? (
                <SalesDeliveryImportPreview
                    result={preview}
                    regions={regions.data}
                    filename="粘贴的地址"
                    prefix={`${prefix}-paste`}
                    onDownloadIssues={() => {
                        if (!preview) return
                        void createSalesDeliveryIssueWorkbook(preview.issues)
                            .then((blob) =>
                                downloadBlob(blob, "收货安排问题行.xlsx", {
                                    linkId: `${prefix}-paste-issues-file`,
                                }),
                            )
                            .catch((reason) =>
                                setError(getErrorMessage(reason)),
                            )
                    }}
                />
            ) : null}
        </div>
    )
}

function AddressOverviewPane({
    form,
    lines,
    overview,
    onInspectLine,
    onFilterUnset,
}: {
    form: SalesOrderCreateFormApi
    lines: SalesOrderDraftLineInput[]
    overview: ReturnType<typeof summarizeOrderDelivery>
    onInspectLine: (rowKey: string) => void
    onFilterUnset: () => void
}) {
    const regions = useRegionIndex()
    const [search, setSearch] = React.useState("")
    const [editingKey, setEditingKey] = React.useState<string | null>(null)
    const [expandedKey, setExpandedKey] = React.useState<string | null>(null)

    const filteredGroups = overview.groups.filter((group) => {
        const needle = search.trim().toLowerCase()
        if (!needle) return true
        const regionText =
            group.region_name ||
            (regions.data && group.region_code
                ? regionFullName(regions.data, group.region_code)
                : "")
        return [
            group.recipient,
            group.phone,
            regionText,
            group.address,
            ...group.lineNames,
        ].some((field) => field.toLowerCase().includes(needle))
    })

    const firstUnsetLine = lines.find(
        (line) => (line.deliveryAllocations?.length ?? 0) === 0,
    )

    const handleBulkUpdateAddress = (
        oldKey: string,
        next: UnifiedDeliveryAddress,
    ) => {
        const current = form.getFieldValue("lineItems")
        const { nextLines, affectedLineCount } = replaceAddressAcrossOrder(
            current,
            oldKey,
            {
                ...next,
                region_name:
                    next.region_code && regions.data
                        ? regionFullName(regions.data, next.region_code)
                        : undefined,
            },
        )
        form.setFieldValue("lineItems", nextLines)
        setEditingKey(null)
        toast.add({
            title: "已批量更新收货地址",
            description: `已同步更新 ${affectedLineCount} 项商品的收货信息，各商品分配数量保持不变。`,
            type: "success",
            timeout: 3000,
        })
    }

    const handleFillUnset = (group: DeliveryAddressGroup) => {
        const current = form.getFieldValue("lineItems")
        const { nextLines, updatedCount } = applyDeliveryAddressToUnsetLines(
            current,
            {
                recipient: group.recipient,
                phone: group.phone,
                region_code: group.region_code,
                region_name: group.region_name,
                address: group.address,
            },
        )
        if (updatedCount === 0) return
        form.setFieldValue("lineItems", nextLines)
        toast.add({
            title: "已填补未设地址商品",
            description: `已将 ${updatedCount} 项未设地址的商品设为「${group.recipient}」的地址。`,
            type: "success",
            timeout: 3000,
        })
    }

    const handleApplyToAll = (group: DeliveryAddressGroup) => {
        const current = form.getFieldValue("lineItems")
        form.setFieldValue(
            "lineItems",
            applyDeliveryAddressToLines(current, {
                recipient: group.recipient,
                phone: group.phone,
                region_code: group.region_code,
                region_name: group.region_name,
                address: group.address,
            }),
        )
        toast.add({
            title: "已设为全单统一地址",
            description: `全部 ${current.length} 项商品将按各自数量送到该地址。`,
            type: "success",
            timeout: 3000,
        })
    }

    const handleRemoveGroup = (group: DeliveryAddressGroup) => {
        const current = form.getFieldValue("lineItems")
        const { nextLines, affectedLineCount } = removeAddressAcrossOrder(
            current,
            group.key,
        )
        form.setFieldValue("lineItems", nextLines)
        toast.add({
            title: "已移除该收货地址",
            description: `已从 ${affectedLineCount} 项商品中移除「${group.recipient}」的地址。`,
            type: "success",
            timeout: 3000,
        })
    }

    return (
        <div className="space-y-4">
            {overview.unsetLineCount > 0 ? (
                <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-warning-border/70 bg-warning-soft/55 px-3.5 py-2.5 text-body-compact">
                    <span className="text-warning-soft-foreground">
                        尚有{" "}
                        <span className="num font-semibold">
                            {overview.unsetLineCount}
                        </span>{" "}
                        项商品未设置收货地址
                    </span>
                    <div className="flex items-center gap-1.5">
                        {firstUnsetLine ? (
                            <Button
                                id="sales-delivery-overview-configure-unset"
                                type="button"
                                variant="ghost"
                                size="xs"
                                className={
                                    detailWorkspaceStyles.inlineLinkButton
                                }
                                onClick={() =>
                                    onInspectLine(firstUnsetLine.rowKey)
                                }
                            >
                                逐项配置
                            </Button>
                        ) : null}
                        <Button
                            id="sales-delivery-overview-filter-unset"
                            type="button"
                            variant="ghost"
                            size="xs"
                            className={detailWorkspaceStyles.quietButton}
                            onClick={onFilterUnset}
                        >
                            在明细表筛选
                        </Button>
                    </div>
                </div>
            ) : null}

            {overview.groups.length > 5 ? (
                <div className="relative">
                    <SearchIcon
                        className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                        aria-hidden="true"
                    />
                    <Input
                        id="sales-delivery-overview-search"
                        value={search}
                        onChange={(event) => setSearch(event.target.value)}
                        placeholder="按收货人、电话、地址或商品名搜索…"
                        className="h-control-sm pl-8"
                    />
                </div>
            ) : null}

            {overview.groups.length === 0 ? (
                <div className="flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed border-border/80 bg-surface-sunken/50 px-6 py-12 text-center">
                    <MapPinIcon
                        className="size-5 text-muted-foreground"
                        aria-hidden="true"
                    />
                    <p className="font-medium text-foreground">
                        当前订单尚未设置收货地址
                    </p>
                    <p className="max-w-sm text-xs text-muted-foreground">
                        点击页面工具栏的“设置统一地址”，或在明细表勾选多件商品分批设置，或通过
                        Excel 批量导入。
                    </p>
                    {firstUnsetLine ? (
                        <Button
                            id="sales-delivery-overview-empty-start"
                            type="button"
                            variant="outline"
                            size="sm"
                            className="mt-1"
                            onClick={() => onInspectLine(firstUnsetLine.rowKey)}
                        >
                            按商品逐项设置
                        </Button>
                    ) : null}
                </div>
            ) : (
                <div className="divide-y divide-border/70 rounded-lg border border-border/75">
                    {filteredGroups.map((group) => {
                        const seg = toAutomationIdSegment(group.key).slice(
                            0,
                            32,
                        )
                        const isEditing = editingKey === group.key
                        const isExpanded = expandedKey === group.key
                        return (
                            <div key={group.key} className="space-y-3 p-4">
                                <div className="flex flex-wrap items-start justify-between gap-3">
                                    <div className="min-w-0 flex-1 space-y-1">
                                        <div className="flex flex-wrap items-center gap-2">
                                            <span className="font-medium text-foreground">
                                                {group.recipient ||
                                                    "待填收货人"}
                                            </span>
                                            <span className="num text-xs text-muted-foreground">
                                                {group.phone || "待填电话"}
                                            </span>
                                            <span className="num rounded-sm bg-info-soft px-1.5 py-0.5 text-tiny font-medium text-info-soft-foreground">
                                                覆盖 {group.lineCount} 项商品
                                            </span>
                                        </div>
                                        <p className="break-words text-xs text-muted-foreground">
                                            {group.region_code ? (
                                                <RegionName
                                                    code={group.region_code}
                                                />
                                            ) : (
                                                "未选地区"
                                            )}
                                            {group.address}
                                        </p>
                                    </div>

                                    <div className="flex shrink-0 flex-wrap items-center gap-1">
                                        <Button
                                            id={`sales-delivery-overview-${seg}-edit`}
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            className={
                                                detailWorkspaceStyles.subtleButton
                                            }
                                            onClick={() =>
                                                setEditingKey(
                                                    isEditing
                                                        ? null
                                                        : group.key,
                                                )
                                            }
                                        >
                                            <PencilIcon
                                                className="size-3"
                                                aria-hidden="true"
                                            />
                                            {isEditing
                                                ? "取消编辑"
                                                : "统一修改"}
                                        </Button>
                                        {overview.unsetLineCount > 0 ? (
                                            <Button
                                                id={`sales-delivery-overview-${seg}-fill-unset`}
                                                type="button"
                                                variant="ghost"
                                                size="xs"
                                                className={
                                                    detailWorkspaceStyles.inlineLinkButton
                                                }
                                                onClick={() =>
                                                    handleFillUnset(group)
                                                }
                                            >
                                                填补其余{" "}
                                                {overview.unsetLineCount} 项
                                            </Button>
                                        ) : null}
                                        {group.lineCount < lines.length ? (
                                            <Button
                                                id={`sales-delivery-overview-${seg}-apply-all`}
                                                type="button"
                                                variant="ghost"
                                                size="xs"
                                                className={
                                                    detailWorkspaceStyles.quietButton
                                                }
                                                onClick={() =>
                                                    handleApplyToAll(group)
                                                }
                                            >
                                                设为全单统一
                                            </Button>
                                        ) : null}
                                        <Button
                                            id={`sales-delivery-overview-${seg}-remove`}
                                            type="button"
                                            variant="ghost"
                                            size="icon-sm"
                                            aria-label={`从 ${group.lineCount} 项商品中移除该地址`}
                                            className={
                                                detailWorkspaceStyles.quietDangerButton
                                            }
                                            onClick={() =>
                                                handleRemoveGroup(group)
                                            }
                                        >
                                            <Trash2Icon
                                                className="size-3.5"
                                                aria-hidden="true"
                                            />
                                        </Button>
                                    </div>
                                </div>

                                {isEditing ? (
                                    <OverviewAddressEditForm
                                        idPrefix={`sales-delivery-overview-${seg}`}
                                        initial={group}
                                        affectedCount={group.lineCount}
                                        onCancel={() => setEditingKey(null)}
                                        onSave={(next) =>
                                            handleBulkUpdateAddress(
                                                group.key,
                                                next,
                                            )
                                        }
                                    />
                                ) : null}

                                <div className="flex flex-wrap items-center gap-1.5 text-xs">
                                    <button
                                        id={`sales-delivery-overview-${seg}-lines-toggle`}
                                        type="button"
                                        className="cursor-pointer text-muted-foreground hover:text-primary"
                                        onClick={() =>
                                            setExpandedKey(
                                                isExpanded ? null : group.key,
                                            )
                                        }
                                    >
                                        {isExpanded
                                            ? "收起关联商品"
                                            : `查看关联商品（${group.lineNames.slice(0, 2).join("、")}${group.lineCount > 2 ? ` 等 ${group.lineCount} 项` : ""}）`}
                                    </button>
                                </div>

                                {isExpanded ? (
                                    <div className="flex flex-wrap gap-1.5 pt-1">
                                        {group.lineKeys.map(
                                            (lineKey, linePos) => (
                                                <button
                                                    key={lineKey}
                                                    id={`sales-delivery-overview-${seg}-line-${toAutomationIdSegment(lineKey)}`}
                                                    type="button"
                                                    className="cursor-pointer rounded-md border border-border/70 bg-surface-sunken/70 px-2 py-1 text-xs text-foreground transition-colors hover:border-primary/40 hover:text-primary"
                                                    onClick={() =>
                                                        onInspectLine(lineKey)
                                                    }
                                                >
                                                    {group.lineNames[linePos] ||
                                                        "商品"}
                                                </button>
                                            ),
                                        )}
                                    </div>
                                ) : null}
                            </div>
                        )
                    })}
                </div>
            )}
        </div>
    )
}

function OverviewAddressEditForm({
    idPrefix,
    initial,
    affectedCount,
    onCancel,
    onSave,
}: {
    idPrefix: string
    initial: DeliveryAddressGroup
    affectedCount: number
    onCancel: () => void
    onSave: (value: UnifiedDeliveryAddress) => void
}) {
    const form = useAppForm({
        defaultValues: {
            recipient: initial.recipient,
            phone: initial.phone,
            region_code: initial.region_code,
            address: initial.address,
        },
        validators: { onChange: overviewEditSchema },
        onSubmit: ({ value }) => {
            onSave({
                recipient: value.recipient.trim(),
                phone: value.phone.trim(),
                region_code: value.region_code,
                address: value.address.trim(),
            })
        },
    })

    return (
        <form
            data-slot="detail-form-grid"
            className="space-y-3 rounded-lg border border-primary/25 bg-surface-sunken/60 p-3.5"
            noValidate
            onSubmit={(event) => {
                event.preventDefault()
                event.stopPropagation()
                void form.handleSubmit()
            }}
        >
            <p className="text-xs text-muted-foreground">
                保存后将同步更新所有使用此地址的{" "}
                <span className="num font-medium text-foreground">
                    {affectedCount}
                </span>{" "}
                项商品，各商品分配数量保持不变。
            </p>
            <div className="grid gap-3 sm:grid-cols-2">
                <form.AppField name="recipient">
                    {(field) => (
                        <field.TextField
                            id={`${idPrefix}-edit-recipient`}
                            label="收货人"
                            required
                        />
                    )}
                </form.AppField>
                <form.AppField name="phone">
                    {(field) => (
                        <field.TextField
                            id={`${idPrefix}-edit-phone`}
                            label="联系电话"
                            type="tel"
                            required
                        />
                    )}
                </form.AppField>
            </div>
            <form.AppField name="region_code">
                {(field) => (
                    <DeliveryRegionField
                        id={`${idPrefix}-edit-region`}
                        required
                        value={field.state.value}
                        onValueChange={(code) => field.handleChange(code)}
                        onBlur={field.handleBlur}
                        invalid={
                            field.state.meta.isTouched &&
                            !field.state.meta.isValid
                        }
                        errors={toFieldErrors(field.state.meta.errors)}
                    />
                )}
            </form.AppField>
            <form.AppField name="address">
                {(field) => (
                    <field.TextField
                        id={`${idPrefix}-edit-address`}
                        label="详细地址"
                        required
                    />
                )}
            </form.AppField>
            <div className="flex justify-end gap-2">
                <Button
                    id={`${idPrefix}-edit-cancel`}
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={onCancel}
                >
                    取消
                </Button>
                <form.AppForm>
                    <form.SubmitButton
                        id={`${idPrefix}-edit-save`}
                        size="sm"
                        label={`同步更新 ${affectedCount} 项`}
                    />
                </form.AppForm>
            </div>
        </form>
    )
}
