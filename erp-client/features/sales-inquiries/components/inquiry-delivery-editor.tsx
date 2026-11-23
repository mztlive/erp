"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { CopyIcon } from "lucide-react"
import { withForm } from "@/components/form"
import { OptionCombobox } from "@/components/business/option-combobox"
import { QuantityValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import {
    Popover,
    PopoverContent,
    PopoverDescription,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import { StatusBadge } from "@/components/ui/status-badge"
import { toast } from "@/components/ui/toast"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal, sumFixed } from "@/lib/fixed-decimal"
import type { AddressImportChanges } from "../lib/address-import-plan"
import { demandFormDefaults } from "../lib/demand-form"
import {
    allocationRestrictions,
    deliveryProgress,
    newAllocation,
} from "../lib/delivery-progress"
import type { InquiryAllocation, InquiryLine, SalesInquiry } from "../types"
import { InquiryDeliveryAddressDialog } from "./inquiry-delivery-address-form"
import { InquiryAddressImportDialog } from "./inquiry-address-import-dialog"
import { InquiryDeliveryAddressList } from "./inquiry-delivery-address-list"

const DELIVERY_KINDS = [
    { value: "physical", label: "实物配送" },
    { value: "virtual", label: "虚拟交付" },
    { value: "service", label: "服务交付" },
]

export const InquiryDeliveryEditor = withForm({
    defaultValues: demandFormDefaults,
    props: {
        index: 0,
        line: null as InquiryLine | null,
        inquiry: undefined as SalesInquiry | undefined,
        disabled: false,
        unit: "",
    },
    render: function DeliveryEditor({
        form,
        index,
        line,
        inquiry,
        disabled,
        unit,
    }) {
        const [editing, setEditing] = React.useState<{
            initial: InquiryAllocation
            sourceId?: string
            replace: boolean
            title: string
        } | null>(null)
        const [importOpen, setImportOpen] = React.useState(false)
        const [reuseOpen, setReuseOpen] = React.useState(false)
        const [editingTerms, setEditingTerms] = React.useState(false)
        const [selectedIds, setSelectedIds] = React.useState<Set<string>>(
            new Set(),
        )
        const [removed, setRemoved] = React.useState<
            { allocation: InquiryAllocation; index: number }[]
        >([])
        const allLines = useStore(
            form.store,
            (state) => state.values.draft.lines,
        )
        const prefix = `sales-inquiry-line-${toAutomationIdSegment(line?.line_id ?? "")}`
        if (!line) return null
        const reusableSources =
            line.fulfillment_kind === "physical"
                ? allLines.filter(
                      (candidate) =>
                          candidate.line_id !== line.line_id &&
                          candidate.allocations.length > 0,
                  )
                : []
        const progress = deliveryProgress({ ...line, unit_code: unit ?? "" })
        const allocationsPath = `draft.lines[${index}].allocations` as const

        const reuseAddressesFrom = (source: InquiryLine) => {
            if (disabled) return
            const current = form.getFieldValue(allocationsPath)
            const remainingSlots = 100 - current.length
            if (remainingSlots <= 0) {
                toast.add({
                    title: "地址数量已达上限",
                    description: "当前商品已有 100 个配送地址。",
                    type: "warning",
                })
                return
            }
            const sourceList = source.allocations.slice(0, remainingSlots)
            const singleTargetFill =
                current.length === 0 && sourceList.length === 1
            const copied: InquiryAllocation[] = sourceList.map((item) => ({
                ...item,
                allocation_id: crypto.randomUUID(),
                quantity: singleTargetFill ? line.quantity : item.quantity,
            }))
            form.setFieldValue(allocationsPath, [...current, ...copied])
            setReuseOpen(false)
            toast.add({
                title: "已复用配送地址",
                description:
                    source.allocations.length > remainingSlots
                        ? `已从「${source.product_name}」复制前 ${copied.length} 个地址（已达 100 个上限），请核对数量。`
                        : `已从「${source.product_name}」复制 ${copied.length} 个地址${singleTargetFill ? "" : "，请核对各地址配送数量"}。`,
                type: "success",
            })
        }

        const editAddress = (
            allocation: InquiryAllocation,
            replace = false,
        ) => {
            if (disabled) return
            const current = form.getFieldValue(allocationsPath)
            const existing = current.find(
                (item) => item.allocation_id === allocation.allocation_id,
            )
            if (
                !existing ||
                (replace &&
                    allocationRestrictions(inquiry, existing.allocation_id)
                        .occupied)
            )
                return
            const next = replace
                ? { ...existing, allocation_id: crypto.randomUUID() }
                : existing
            setEditing({
                initial: next,
                sourceId: existing.allocation_id,
                replace,
                title: replace ? "替换收件地址" : "编辑配送地址",
            })
        }

        const appendAddresses = (allocations: InquiryAllocation[]) => {
            if (disabled) throw new Error("当前需求不可编辑，请稍后重试。")
            const current = form.getFieldValue(allocationsPath)
            if (current.length + allocations.length > 100)
                throw new Error("加入后超过 100 个地址，请减少新增地址。")
            if (
                allocations.some((entry) =>
                    current.some(
                        (item) => item.allocation_id === entry.allocation_id,
                    ),
                )
            )
                throw new Error("本次地址已添加，请重新解析后核对。")
            form.setFieldValue(allocationsPath, [...current, ...allocations])
        }

        const applyAddress = (value: InquiryAllocation) => {
            if (!editing || disabled)
                throw new Error("当前需求不可编辑，请稍后重试。")
            if (!editing.sourceId) {
                appendAddresses([value])
                return
            }
            const current = form.getFieldValue(allocationsPath)
            const source = current.find(
                (item) => item.allocation_id === editing.sourceId,
            )
            if (!source) throw new Error("原地址已移除，请关闭后重新添加。")
            const access = allocationRestrictions(inquiry, source.allocation_id)
            if (editing.replace && access.occupied)
                throw new Error("该地址已有销售占用，不可替换。")
            const next = editing.replace
                ? value
                : access.frozen
                  ? { ...source, quantity: value.quantity }
                  : { ...value, allocation_id: source.allocation_id }
            form.setFieldValue(
                allocationsPath,
                current.map((item) =>
                    item.allocation_id === source.allocation_id ? next : item,
                ),
            )
            setSelectedIds(new Set())
        }

        const applyImportedAddresses = (changes: AddressImportChanges) => {
            if (disabled) throw new Error("当前需求不可编辑，请稍后重试。")
            const currentLine = form.getFieldValue(`draft.lines[${index}]`)
            const current = currentLine.allocations
            if (JSON.stringify(current) !== changes.base)
                throw new Error("地址已发生变化，请重新核对预览后确认。")
            if (current.length + changes.additions.length > 100)
                throw new Error("处理后超过 100 个地址，请减少新增地址。")
            for (const update of changes.updates) {
                if (
                    !current.some(
                        (item) => item.allocation_id === update.allocation_id,
                    )
                )
                    throw new Error("待更新的地址已移除，请重新核对预览。")
                const occupied = sumFixed(
                    (inquiry?.quantity_ledger ?? [])
                        .filter((entry) => entry.state !== "released")
                        .flatMap((entry) =>
                            entry.selections
                                .filter(
                                    (item) =>
                                        item.line_id === currentLine.line_id &&
                                        item.allocation_id ===
                                            update.allocation_id,
                                )
                                .map((item) => item.quantity),
                        ),
                    {
                        maxScale: currentLine.quantity_scale,
                        outputScale: currentLine.quantity_scale,
                    },
                )
                if (
                    compareDecimal(
                        update.quantity,
                        occupied,
                        currentLine.quantity_scale,
                    ) < 0
                )
                    throw new Error(
                        "结果数量不能少于地址已占用数量，请重新核对。",
                    )
            }
            const quantities = new Map(
                changes.updates.map((item) => [
                    item.allocation_id,
                    item.quantity,
                ]),
            )
            form.setFieldValue(allocationsPath, [
                ...current.map((item) =>
                    quantities.has(item.allocation_id)
                        ? {
                              ...item,
                              quantity: quantities.get(item.allocation_id)!,
                          }
                        : item,
                ),
                ...changes.additions,
            ])
        }

        const removeAddresses = (ids: ReadonlySet<string>) => {
            if (disabled) return
            const current = form.getFieldValue(allocationsPath)
            const entries = current.flatMap((allocation, position) =>
                ids.has(allocation.allocation_id) &&
                !allocationRestrictions(inquiry, allocation.allocation_id)
                    .occupied
                    ? [{ allocation, index: position }]
                    : [],
            )
            if (!entries.length) return
            const removedIds = new Set(
                entries.map(({ allocation }) => allocation.allocation_id),
            )
            form.setFieldValue(
                allocationsPath,
                current.filter((item) => !removedIds.has(item.allocation_id)),
            )
            setRemoved(entries)
            setSelectedIds(new Set())
        }

        return (
            <>
                <fieldset disabled={disabled} className="min-w-0 space-y-4">
                    <div className="flex flex-wrap items-center justify-between gap-2 border-b pb-3">
                        <span className="text-body-compact font-medium">
                            需求{" "}
                            <QuantityValue value={line.quantity} unit={unit} />
                        </span>
                        <StatusBadge
                            tone={
                                progress.complete && !progress.overAllocated
                                    ? "neutral"
                                    : "warning"
                            }
                            label={progress.label}
                        />
                    </div>
                    <form.Field name={allocationsPath} mode="array">
                        {() => (
                            <>
                                <div className="space-y-3 border-b pb-3">
                                    <div className="flex items-start justify-between gap-3">
                                        <p className="min-w-0 text-body-compact leading-6">
                                            <span className="font-medium">
                                                {
                                                    DELIVERY_KINDS.find(
                                                        (kind) =>
                                                            kind.value ===
                                                            line.fulfillment_kind,
                                                    )?.label
                                                }
                                            </span>
                                            <span className="ml-3 text-muted-foreground">
                                                {line.delivery_terms ||
                                                    "未填写交付条件"}
                                            </span>
                                        </p>
                                        <Button
                                            id={`${prefix}-edit-delivery-terms`}
                                            type="button"
                                            size="sm"
                                            variant="ghost"
                                            onClick={() =>
                                                setEditingTerms(
                                                    (value) => !value,
                                                )
                                            }
                                        >
                                            {editingTerms
                                                ? "收起"
                                                : "编辑交付条件"}
                                        </Button>
                                    </div>
                                    {editingTerms && (
                                        <div className="grid gap-3 sm:grid-cols-2">
                                            <form.Field
                                                name={`draft.lines[${index}].fulfillment_kind`}
                                            >
                                                {(field) => (
                                                    <div className="space-y-1.5">
                                                        <Label
                                                            htmlFor={`${prefix}-fulfillment-kind`}
                                                        >
                                                            交付类型
                                                        </Label>
                                                        <OptionCombobox
                                                            id={`${prefix}-fulfillment-kind`}
                                                            aria-label="交付类型"
                                                            allowClear={false}
                                                            options={
                                                                DELIVERY_KINDS
                                                            }
                                                            value={
                                                                field.state
                                                                    .value
                                                            }
                                                            onValueChange={(
                                                                value,
                                                            ) =>
                                                                field.handleChange(
                                                                    value as InquiryLine["fulfillment_kind"],
                                                                )
                                                            }
                                                        />
                                                    </div>
                                                )}
                                            </form.Field>
                                            <form.AppField
                                                name={`draft.lines[${index}].delivery_terms`}
                                            >
                                                {(field) => (
                                                    <field.TextField
                                                        id={`${prefix}-delivery-terms`}
                                                        label="交付条件"
                                                        className="gap-1.5"
                                                    />
                                                )}
                                            </form.AppField>
                                        </div>
                                    )}
                                </div>
                                {removed.length > 0 && (
                                    <div
                                        className="flex flex-wrap items-center justify-between gap-2 rounded-md bg-muted/40 px-3 py-2 text-body-compact"
                                        role="status"
                                    >
                                        <span>
                                            已移除 {removed.length} 个地址
                                        </span>
                                        <Button
                                            id={`${prefix}-undo-remove-addresses`}
                                            type="button"
                                            variant="ghost"
                                            size="sm"
                                            disabled={
                                                disabled ||
                                                line.allocations.length +
                                                    removed.length >
                                                    100
                                            }
                                            title={
                                                line.allocations.length +
                                                    removed.length >
                                                100
                                                    ? "恢复后超过 100 个地址，请先移除其他地址"
                                                    : undefined
                                            }
                                            onClick={() => {
                                                const current = [
                                                    ...form.getFieldValue(
                                                        allocationsPath,
                                                    ),
                                                ]
                                                const pending = removed.filter(
                                                    ({ allocation }) =>
                                                        !current.some(
                                                            (item) =>
                                                                item.allocation_id ===
                                                                allocation.allocation_id,
                                                        ),
                                                )
                                                if (
                                                    current.length +
                                                        pending.length >
                                                    100
                                                )
                                                    return
                                                for (const entry of pending)
                                                    current.splice(
                                                        Math.min(
                                                            entry.index,
                                                            current.length,
                                                        ),
                                                        0,
                                                        entry.allocation,
                                                    )
                                                form.setFieldValue(
                                                    allocationsPath,
                                                    current,
                                                )
                                                setRemoved([])
                                            }}
                                        >
                                            撤销
                                        </Button>
                                    </div>
                                )}
                                <InquiryDeliveryAddressList
                                    prefix={prefix}
                                    allocations={line.allocations}
                                    unit={unit}
                                    inquiry={inquiry}
                                    selectedIds={selectedIds}
                                    disabled={disabled}
                                    onSelectionChange={setSelectedIds}
                                    onEdit={editAddress}
                                    onRemove={removeAddresses}
                                    onImport={() => setImportOpen(true)}
                                    extraActions={
                                        reusableSources.length > 0 ? (
                                            <Popover
                                                open={reuseOpen}
                                                onOpenChange={setReuseOpen}
                                            >
                                                <PopoverTrigger
                                                    id={`${prefix}-reuse-addresses`}
                                                    render={
                                                        <Button
                                                            type="button"
                                                            size="sm"
                                                            variant="outline"
                                                        />
                                                    }
                                                    disabled={
                                                        disabled ||
                                                        line.allocations
                                                            .length >= 100
                                                    }
                                                >
                                                    <CopyIcon
                                                        aria-hidden="true"
                                                        data-icon="inline-start"
                                                        className="size-3.5"
                                                    />
                                                    复用已有地址
                                                </PopoverTrigger>
                                                <PopoverContent
                                                    align="end"
                                                    className="w-80 max-w-[calc(100vw-2rem)]"
                                                >
                                                    <PopoverTitle>
                                                        复用其他商品的配送地址
                                                    </PopoverTitle>
                                                    <PopoverDescription>
                                                        复制收件人与地址到当前商品，复制后可按需调整数量。
                                                    </PopoverDescription>
                                                    <div className="max-h-56 space-y-1 overflow-y-auto pt-1">
                                                        {reusableSources.map(
                                                            (source) => (
                                                                <button
                                                                    key={
                                                                        source.line_id
                                                                    }
                                                                    id={`${prefix}-reuse-from-${toAutomationIdSegment(source.line_id)}`}
                                                                    type="button"
                                                                    disabled={
                                                                        disabled
                                                                    }
                                                                    onClick={() =>
                                                                        reuseAddressesFrom(
                                                                            source,
                                                                        )
                                                                    }
                                                                    className="flex w-full items-center justify-between gap-2 rounded-md px-2.5 py-2 text-left text-body-compact transition-colors hover:bg-muted focus-visible:outline-2 focus-visible:outline-ring"
                                                                >
                                                                    <span className="min-w-0 truncate font-medium text-foreground">
                                                                        {
                                                                            source.product_name
                                                                        }
                                                                    </span>
                                                                    <span className="num shrink-0 text-xs text-muted-foreground">
                                                                        {
                                                                            source
                                                                                .allocations
                                                                                .length
                                                                        }{" "}
                                                                        个地址
                                                                    </span>
                                                                </button>
                                                            ),
                                                        )}
                                                    </div>
                                                </PopoverContent>
                                            </Popover>
                                        ) : undefined
                                    }
                                    onAdd={() => {
                                        const currentLine = form.getFieldValue(
                                            `draft.lines[${index}]`,
                                        )
                                        if (
                                            disabled ||
                                            currentLine.allocations.length >=
                                                100
                                        )
                                            return
                                        const allocation =
                                            newAllocation(currentLine)
                                        setEditing({
                                            initial: allocation,
                                            replace: false,
                                            title: "添加配送地址",
                                        })
                                    }}
                                />
                                {line.fulfillment_kind === "physical" && (
                                    <p className="text-xs text-muted-foreground">
                                        提交采购前，收件资料须完整，配送数量合计不得少于需求数量；超量允许保存和提交。
                                    </p>
                                )}
                            </>
                        )}
                    </form.Field>
                </fieldset>
                {editing && (
                    <InquiryDeliveryAddressDialog
                        key={editing.initial.allocation_id}
                        prefix={prefix}
                        initial={editing.initial}
                        title={editing.title}
                        quantityScale={line.quantity_scale}
                        frozen={
                            !editing.replace &&
                            allocationRestrictions(
                                inquiry,
                                editing.sourceId ?? "",
                            ).frozen
                        }
                        occupied={
                            allocationRestrictions(
                                inquiry,
                                editing.sourceId ?? "",
                            ).occupied
                        }
                        disabled={disabled}
                        returnFocusId={
                            editing.sourceId
                                ? `${prefix}-address-${toAutomationIdSegment(editing.initial.allocation_id)}-edit`
                                : `${prefix}-add-address`
                        }
                        onClose={() => setEditing(null)}
                        onApply={applyAddress}
                    />
                )}
                <InquiryAddressImportDialog
                    prefix={prefix}
                    open={importOpen}
                    onOpenChange={setImportOpen}
                    line={line}
                    inquiry={inquiry}
                    unit={unit}
                    disabled={disabled}
                    onApply={applyImportedAddresses}
                />
            </>
        )
    },
})
