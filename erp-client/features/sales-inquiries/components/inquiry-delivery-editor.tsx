"use client"

import * as React from "react"
import { withForm } from "@/components/form"
import { OptionCombobox } from "@/components/business/option-combobox"
import { QuantityValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
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
        const [editingTerms, setEditingTerms] = React.useState(false)
        const [selectedIds, setSelectedIds] = React.useState<Set<string>>(
            new Set(),
        )
        const [removed, setRemoved] = React.useState<
            { allocation: InquiryAllocation; index: number }[]
        >([])
        const prefix = `sales-inquiry-line-${toAutomationIdSegment(line?.line_id ?? "")}`
        if (!line) return null
        const progress = deliveryProgress({ ...line, unit_code: unit ?? "" })
        const allocationsPath = `draft.lines[${index}].allocations` as const

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
                        <span className="text-sm font-medium">
                            需求{" "}
                            <QuantityValue value={line.quantity} unit={unit} />
                        </span>
                        <Badge
                            variant={
                                progress.complete && !progress.overAllocated
                                    ? "secondary"
                                    : "warning"
                            }
                        >
                            {progress.label}
                        </Badge>
                    </div>
                    <form.Field name={allocationsPath} mode="array">
                        {() => (
                            <>
                                <div className="space-y-3 border-b pb-3">
                                    <div className="flex items-start justify-between gap-3">
                                        <p className="min-w-0 text-sm leading-6">
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
                                        className="flex flex-wrap items-center justify-between gap-2 rounded-md bg-muted/40 px-3 py-2 text-sm"
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
                                    <p className="text-xs leading-5 text-muted-foreground">
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
