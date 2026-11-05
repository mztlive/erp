"use client"

import {
    useEffect,
    useImperativeHandle,
    useRef,
    useState,
    type Ref,
} from "react"
import { ChevronDownIcon, MapPinIcon, PlusIcon, PencilIcon } from "lucide-react"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Checkbox } from "@/components/ui/checkbox"
import { Badge } from "@/components/ui/badge"
import { Label } from "@/components/ui/label"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { CARRIER_OPTIONS } from "@/lib/business-options"
import { compactFixed, compareDecimal } from "@/lib/fixed-decimal"
import { FulfillmentPackageSplit } from "./fulfillment-package-split"
import {
    allocationBalance,
    automaticPackage,
    changeDispatch,
    changePackageItem,
    dispatchItems,
    packingQuantity,
    normalizePackingQuantity,
} from "../../lib/packing-allocation"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    packingIssues,
    unpackedItems,
    type DeliveryAddressSnapshot,
    type DeliveryPacking,
} from "../../lib/delivery-packing"
import type { FulfillmentDraft, FulfillmentOperation } from "../../types"
import type { FulfillmentDeliveryFormHandle } from "./fulfillment-delivery-form"

type ShippingDraft = Extract<
    FulfillmentDraft,
    { type: "SUPPLIER_DIRECT" | "WAREHOUSE_SHIP" }
>

/** 地址、包裹和数量统一由 TanStack Form 保存；筛选和折叠不丢失录入。 */
export function FulfillmentPackingForm({
    operation,
    draft,
    packing,
    source,
    onChange,
    disabled,
    ref,
}: {
    operation: FulfillmentOperation
    draft: ShippingDraft
    packing: DeliveryPacking
    source: DeliveryAddressSnapshot
    onChange: (draft: FulfillmentDraft) => void
    disabled?: boolean
    ref?: Ref<FulfillmentDeliveryFormHandle>
}) {
    const [search, setSearch] = useState("")
    const [onlyMissing, setOnlyMissing] = useState(false)
    const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set())
    const [editing, setEditing] = useState<Set<string>>(() => new Set())
    const [splitting, setSplitting] = useState<string | null>(null)
    const following = useRef(new Set<string>())
    const form = useAppForm({
        defaultValues: packing,
        listeners: {
            onChange: ({ formApi }) =>
                onChange({ ...draft, packing: formApi.state.values }),
        },
    })
    useEffect(() => {
        if (JSON.stringify(form.state.values) !== JSON.stringify(packing)) {
            following.current.clear()
            form.reset(packing)
        }
    }, [form, packing])
    useImperativeHandle(ref, () => ({
        locate(targetId) {
            if (!targetId.startsWith("fulfillment-packing")) return false
            setSearch("")
            setOnlyMissing(false)
            setCollapsed(new Set())
            requestAnimationFrame(() => {
                const node = document.getElementById(targetId)
                node?.scrollIntoView({ block: "center" })
                node?.focus()
            })
            return true
        },
    }))
    const apply = (next: DeliveryPacking) => {
        form.reset(next)
        onChange({ ...draft, packing: next })
    }
    const balanceNote = (
        values: DeliveryPacking,
        address: string,
        line: string,
    ) => {
        try {
            const { remaining } = allocationBalance(values, address, line)
            const comparison = compareDecimal(remaining, "0", 6)
            return comparison === 0
                ? ""
                : comparison > 0
                  ? `尚未分配 ${packingQuantity(remaining)}`
                  : `超出本次发货 ${packingQuantity(remaining).replace("-", "")}`
        } catch {
            return "请填写有效数量，最多6位小数"
        }
    }
    const itemName = (id: string) =>
        operation.lines.find((line) => line.lineId === id)?.itemName ||
        "商品名称未提供"
    const unit = (id: string) =>
        operation.lines.find((line) => line.lineId === id)?.unitCode || ""
    const toggle = (key: string, setter: typeof setCollapsed) =>
        setter((current) => {
            const next = new Set(current)
            if (next.has(key)) next.delete(key)
            else next.add(key)
            return next
        })
    return (
        <section
            id="fulfillment-packing-form"
            tabIndex={-1}
            className="min-w-0 space-y-3 outline-none"
            aria-label="按地址登记发货包裹"
        >
            <form.Subscribe selector={(state) => state.values}>
                {(values) => {
                    const ready = (key: string) =>
                        packingIssues(
                            {
                                ...values,
                                dispatch: values.dispatch.filter(
                                    (row) => row.destination_key === key,
                                ),
                                packages: values.packages.filter(
                                    (row) => row.destination_key === key,
                                ),
                            },
                            source,
                        ).length === 0
                    const visible = source.destinations.filter(
                        (row) =>
                            (!onlyMissing || !ready(row.key)) &&
                            `${row.recipient} ${row.address} ${row.items.map((item) => itemName(item.delivery_line_id)).join(" ")}`
                                .toLocaleLowerCase()
                                .includes(search.trim().toLocaleLowerCase()),
                    )
                    return (
                        <>
                            <div className="flex flex-wrap items-start justify-between gap-4">
                                <div className="space-y-1.5">
                                    <div className="flex flex-wrap items-baseline gap-4">
                                        <h3 className="text-lg font-semibold">
                                            发货明细与物流
                                        </h3>
                                        <span className="text-sm text-muted-foreground">
                                            共 {source.destinations.length}{" "}
                                            个收货地址 ·{" "}
                                            {
                                                new Set(
                                                    operation.lines.map(
                                                        (line) =>
                                                            line.salesOrderLineId,
                                                    ),
                                                ).size
                                            }{" "}
                                            种商品 · {values.packages.length}{" "}
                                            个包裹
                                        </span>
                                    </div>
                                    <p className="text-xs text-muted-foreground">
                                        同一地址内的商品可合并包裹；每个包裹分别登记物流及商品数量。未发数量保留为后续任务。
                                    </p>
                                </div>
                                <div className="flex flex-wrap items-center gap-3">
                                    <Input
                                        id="fulfillment-packing-search"
                                        placeholder="搜索收货人、地址或商品"
                                        value={search}
                                        onChange={(event) =>
                                            setSearch(event.target.value)
                                        }
                                        className="w-64"
                                    />
                                    <Label htmlFor="fulfillment-packing-only-missing">
                                        <Checkbox
                                            id="fulfillment-packing-only-missing"
                                            checked={onlyMissing}
                                            onCheckedChange={setOnlyMissing}
                                        />
                                        只看未登记
                                    </Label>
                                </div>
                            </div>
                            {draft.legacyTrackingNeedsAllocation && (
                                <div className="flex flex-wrap items-baseline gap-x-4 gap-y-1 rounded-lg bg-muted/30 px-3 py-2 text-xs">
                                    <p>
                                        原物流记录尚未关联收货地址，请核对后录入对应包裹。保存后以地址包裹为准。
                                    </p>
                                    <p className="text-muted-foreground">
                                        {(draft.trackingEntries ?? [])
                                            .map(
                                                (entry) =>
                                                    `${entry.carrier ?? ""} ${entry.trackingNo}`,
                                            )
                                            .join(" · ")}
                                    </p>
                                </div>
                            )}
                            {!visible.length && (
                                <p className="py-8 text-center text-sm text-muted-foreground">
                                    没有符合条件的收货地址
                                </p>
                            )}
                            {visible.map((destination) => {
                                const segment = toAutomationIdSegment(
                                    destination.key,
                                )
                                const dispatchIndex = values.dispatch.findIndex(
                                    (row) =>
                                        row.destination_key === destination.key,
                                )
                                const parcels = values.packages
                                    .map((parcel, index) => ({ parcel, index }))
                                    .filter(
                                        ({ parcel }) =>
                                            parcel.destination_key ===
                                            destination.key,
                                    )
                                const isCollapsed = collapsed.has(
                                    destination.key,
                                )
                                return (
                                    <section
                                        key={destination.key}
                                        className="overflow-hidden rounded-xl border border-border/70"
                                        aria-label={`${destination.recipient}的发货明细`}
                                    >
                                        <div className="flex flex-wrap items-center gap-x-4 gap-y-2 bg-muted/35 px-3 py-2">
                                            <MapPinIcon
                                                className="size-4 text-muted-foreground"
                                                aria-hidden="true"
                                            />
                                            <span className="font-semibold">
                                                {destination.recipient}
                                            </span>
                                            <span className="text-sm">
                                                {destination.phone}
                                            </span>
                                            <span className="min-w-0 flex-1 break-words text-sm">
                                                {destination.address}
                                            </span>
                                            <Badge
                                                variant={
                                                    ready(destination.key)
                                                        ? "teal"
                                                        : "outline"
                                                }
                                            >
                                                {ready(destination.key)
                                                    ? "物流已登记"
                                                    : "待登记物流"}
                                            </Badge>
                                            <Button
                                                id={`fulfillment-packing-address-${segment}-toggle`}
                                                size="icon-sm"
                                                variant="ghost"
                                                aria-label={
                                                    isCollapsed
                                                        ? "展开收货地址"
                                                        : "收起收货地址"
                                                }
                                                aria-expanded={!isCollapsed}
                                                onClick={() =>
                                                    toggle(
                                                        destination.key,
                                                        setCollapsed,
                                                    )
                                                }
                                            >
                                                <ChevronDownIcon
                                                    className={
                                                        isCollapsed
                                                            ? ""
                                                            : "rotate-180"
                                                    }
                                                />
                                            </Button>
                                        </div>
                                        <div
                                            hidden={isCollapsed}
                                            className="space-y-2 px-3 pb-3"
                                        >
                                            <Table className="min-w-[34rem] table-fixed">
                                                <colgroup>
                                                    <col />
                                                    <col className="w-24 sm:w-28" />
                                                    <col className="w-28 sm:w-32" />
                                                    <col className="w-28 sm:w-40" />
                                                </colgroup>
                                                <TableHeader>
                                                    <TableRow>
                                                        <TableHead>
                                                            商品
                                                        </TableHead>
                                                        <TableHead>
                                                            应发数量
                                                        </TableHead>
                                                        <TableHead>
                                                            本次发货
                                                        </TableHead>
                                                        <TableHead>
                                                            已分配包裹
                                                        </TableHead>
                                                    </TableRow>
                                                </TableHeader>
                                                <TableBody>
                                                    {destination.items.map(
                                                        (item) => {
                                                            const itemIndex =
                                                                values.dispatch[
                                                                    dispatchIndex
                                                                ]?.items.findIndex(
                                                                    (value) =>
                                                                        value.delivery_line_id ===
                                                                        item.delivery_line_id,
                                                                ) ?? -1
                                                            const itemSegment =
                                                                toAutomationIdSegment(
                                                                    item.delivery_line_id,
                                                                )
                                                            const assigned =
                                                                parcels.filter(
                                                                    ({
                                                                        parcel,
                                                                    }) =>
                                                                        parcel.items.some(
                                                                            (
                                                                                value,
                                                                            ) =>
                                                                                value.delivery_line_id ===
                                                                                item.delivery_line_id,
                                                                        ),
                                                                )
                                                            return (
                                                                <TableRow
                                                                    key={
                                                                        item.delivery_line_id
                                                                    }
                                                                >
                                                                    <TableCell className="whitespace-normal break-words font-medium">
                                                                        {itemName(
                                                                            item.delivery_line_id,
                                                                        )}
                                                                    </TableCell>
                                                                    <TableCell>
                                                                        {compactFixed(
                                                                            item.quantity,
                                                                        )}{" "}
                                                                        {unit(
                                                                            item.delivery_line_id,
                                                                        )}
                                                                    </TableCell>
                                                                    <TableCell>
                                                                        {dispatchIndex >=
                                                                            0 &&
                                                                            itemIndex >=
                                                                                0 && (
                                                                                <form.Field
                                                                                    name={`dispatch[${dispatchIndex}].items[${itemIndex}].quantity`}
                                                                                >
                                                                                    {(
                                                                                        field,
                                                                                    ) => (
                                                                                        <Input
                                                                                            id={`fulfillment-packing-${segment}-${itemSegment}-quantity`}
                                                                                            aria-label={`${destination.recipient} ${itemName(item.delivery_line_id)}本次发货数量`}
                                                                                            inputMode="decimal"
                                                                                            value={
                                                                                                field
                                                                                                    .state
                                                                                                    .value
                                                                                            }
                                                                                            onFocus={() => {
                                                                                                if (
                                                                                                    automaticPackage(
                                                                                                        values,
                                                                                                        destination.key,
                                                                                                    )
                                                                                                )
                                                                                                    following.current.add(
                                                                                                        destination.key,
                                                                                                    )
                                                                                            }}
                                                                                            onChange={(
                                                                                                event,
                                                                                            ) =>
                                                                                                apply(
                                                                                                    changeDispatch(
                                                                                                        values,
                                                                                                        destination.key,
                                                                                                        item.delivery_line_id,
                                                                                                        event
                                                                                                            .target
                                                                                                            .value,
                                                                                                        following.current.has(
                                                                                                            destination.key,
                                                                                                        ) ||
                                                                                                            automaticPackage(
                                                                                                                values,
                                                                                                                destination.key,
                                                                                                            ),
                                                                                                    ),
                                                                                                )
                                                                                            }
                                                                                            disabled={
                                                                                                disabled
                                                                                            }
                                                                                            className="h-8 w-24"
                                                                                        />
                                                                                    )}
                                                                                </form.Field>
                                                                            )}
                                                                    </TableCell>
                                                                    <TableCell className="whitespace-normal text-sm">
                                                                        {assigned.length ? (
                                                                            assigned
                                                                                .map(
                                                                                    ({
                                                                                        index,
                                                                                    }) =>
                                                                                        `包裹 ${index + 1}`,
                                                                                )
                                                                                .join(
                                                                                    "、",
                                                                                )
                                                                        ) : (
                                                                            <span className="text-muted-foreground">
                                                                                未分配
                                                                            </span>
                                                                        )}
                                                                        {balanceNote(
                                                                            values,
                                                                            destination.key,
                                                                            item.delivery_line_id,
                                                                        ) && (
                                                                            <p className="mt-1 text-xs text-warning-soft-foreground">
                                                                                {balanceNote(
                                                                                    values,
                                                                                    destination.key,
                                                                                    item.delivery_line_id,
                                                                                )}{" "}
                                                                                {unit(
                                                                                    item.delivery_line_id,
                                                                                )}
                                                                                ，请调整包裹
                                                                            </p>
                                                                        )}
                                                                    </TableCell>
                                                                </TableRow>
                                                            )
                                                        },
                                                    )}
                                                </TableBody>
                                            </Table>
                                            {parcels.map(
                                                ({ parcel, index }) => {
                                                    const prefix = `fulfillment-packing-package-${toAutomationIdSegment(parcel.id)}`
                                                    const automatic =
                                                        automaticPackage(
                                                            values,
                                                            destination.key,
                                                        )
                                                    const editingItems =
                                                        editing.has(parcel.id)
                                                    return (
                                                        <div
                                                            key={parcel.id}
                                                            className="space-y-2 rounded-lg border border-border/60 bg-muted/15 p-3"
                                                        >
                                                            <div className="flex flex-wrap items-center gap-3">
                                                                <span className="text-sm font-medium">
                                                                    包裹{" "}
                                                                    {index + 1}
                                                                </span>
                                                                <form.AppField
                                                                    name={`packages[${index}].carrier`}
                                                                >
                                                                    {(
                                                                        field,
                                                                    ) => (
                                                                        <field.SelectField
                                                                            id={`${prefix}-carrier`}
                                                                            label="物流公司"
                                                                            hideLabel
                                                                            options={
                                                                                CARRIER_OPTIONS
                                                                            }
                                                                            placeholder="选择物流公司"
                                                                            disabled={
                                                                                disabled
                                                                            }
                                                                            className="w-40"
                                                                        />
                                                                    )}
                                                                </form.AppField>
                                                                <form.Field
                                                                    name={`packages[${index}].tracking_no`}
                                                                >
                                                                    {(
                                                                        field,
                                                                    ) => (
                                                                        <Input
                                                                            id={`${prefix}-tracking`}
                                                                            aria-label={`包裹${index + 1}物流单号`}
                                                                            placeholder="输入物流单号"
                                                                            value={
                                                                                field
                                                                                    .state
                                                                                    .value
                                                                            }
                                                                            onChange={(
                                                                                event,
                                                                            ) =>
                                                                                field.handleChange(
                                                                                    event
                                                                                        .target
                                                                                        .value,
                                                                                )
                                                                            }
                                                                            disabled={
                                                                                disabled
                                                                            }
                                                                            className="w-64"
                                                                        />
                                                                    )}
                                                                </form.Field>
                                                                <div className="ml-auto flex flex-wrap items-center gap-1">
                                                                    <Button
                                                                        id={`${prefix}-split`}
                                                                        size="sm"
                                                                        variant="ghost"
                                                                        disabled={
                                                                            disabled ||
                                                                            !parcel
                                                                                .items
                                                                                .length ||
                                                                            values
                                                                                .packages
                                                                                .length >=
                                                                                100 ||
                                                                            destination.items.some(
                                                                                (
                                                                                    item,
                                                                                ) =>
                                                                                    !!balanceNote(
                                                                                        values,
                                                                                        destination.key,
                                                                                        item.delivery_line_id,
                                                                                    ),
                                                                            )
                                                                        }
                                                                        onClick={() =>
                                                                            setSplitting(
                                                                                parcel.id,
                                                                            )
                                                                        }
                                                                    >
                                                                        拆分包裹
                                                                    </Button>
                                                                    {(!automatic ||
                                                                        editingItems) && (
                                                                        <Button
                                                                            id={`${prefix}-edit-items`}
                                                                            size="sm"
                                                                            variant="ghost"
                                                                            disabled={
                                                                                disabled
                                                                            }
                                                                            onClick={() => {
                                                                                if (
                                                                                    !editingItems
                                                                                )
                                                                                    form.setFieldValue(
                                                                                        `packages[${index}].items`,
                                                                                        parcel.items.map(
                                                                                            (
                                                                                                item,
                                                                                            ) => ({
                                                                                                ...item,
                                                                                                quantity:
                                                                                                    normalizePackingQuantity(
                                                                                                        item.quantity,
                                                                                                    ),
                                                                                            }),
                                                                                        ),
                                                                                    )
                                                                                toggle(
                                                                                    parcel.id,
                                                                                    setEditing,
                                                                                )
                                                                            }}
                                                                        >
                                                                            <PencilIcon />
                                                                            {editingItems
                                                                                ? "完成编辑"
                                                                                : "调整商品"}
                                                                        </Button>
                                                                    )}
                                                                    <Button
                                                                        id={`${prefix}-remove`}
                                                                        size="sm"
                                                                        variant="ghost"
                                                                        disabled={
                                                                            disabled
                                                                        }
                                                                        onClick={() => {
                                                                            following.current.delete(
                                                                                destination.key,
                                                                            )
                                                                            apply(
                                                                                {
                                                                                    ...values,
                                                                                    packages:
                                                                                        values.packages.filter(
                                                                                            (
                                                                                                row,
                                                                                            ) =>
                                                                                                row.id !==
                                                                                                parcel.id,
                                                                                        ),
                                                                                },
                                                                            )
                                                                        }}
                                                                    >
                                                                        移除
                                                                    </Button>
                                                                </div>
                                                            </div>
                                                            {editingItems ? (
                                                                <div className="space-y-2 border-t pt-2">
                                                                    <p className="text-xs text-muted-foreground">
                                                                        填写分到本包裹的数量，填0表示不放入。其他包裹的数量保持不变。
                                                                    </p>
                                                                    {destination.items.map(
                                                                        (
                                                                            item,
                                                                        ) => {
                                                                            const id = `${prefix}-item-${toAutomationIdSegment(item.delivery_line_id)}`
                                                                            let other =
                                                                                "待核对"
                                                                            try {
                                                                                other =
                                                                                    packingQuantity(
                                                                                        allocationBalance(
                                                                                            values,
                                                                                            destination.key,
                                                                                            item.delivery_line_id,
                                                                                            parcel.id,
                                                                                        )
                                                                                            .assigned,
                                                                                    )
                                                                            } catch {}
                                                                            return (
                                                                                <div
                                                                                    key={
                                                                                        item.delivery_line_id
                                                                                    }
                                                                                    className="flex flex-wrap items-center gap-x-4 gap-y-2"
                                                                                >
                                                                                    <Label
                                                                                        htmlFor={`${id}-quantity`}
                                                                                        className="min-w-40"
                                                                                    >
                                                                                        {itemName(
                                                                                            item.delivery_line_id,
                                                                                        )}
                                                                                    </Label>
                                                                                    <form.Field
                                                                                        name={`packages[${index}].items`}
                                                                                    >
                                                                                        {(
                                                                                            field,
                                                                                        ) => (
                                                                                            <Input
                                                                                                id={`${id}-quantity`}
                                                                                                aria-label={`包裹${index + 1}中${itemName(item.delivery_line_id)}数量`}
                                                                                                inputMode="decimal"
                                                                                                className="h-8 w-24"
                                                                                                disabled={
                                                                                                    disabled
                                                                                                }
                                                                                                value={
                                                                                                    field.state.value.find(
                                                                                                        (
                                                                                                            row,
                                                                                                        ) =>
                                                                                                            row.delivery_line_id ===
                                                                                                            item.delivery_line_id,
                                                                                                    )
                                                                                                        ?.quantity ??
                                                                                                    "0"
                                                                                                }
                                                                                                onFocus={() =>
                                                                                                    following.current.delete(
                                                                                                        destination.key,
                                                                                                    )
                                                                                                }
                                                                                                onChange={(
                                                                                                    event,
                                                                                                ) =>
                                                                                                    field.handleChange(
                                                                                                        changePackageItem(
                                                                                                            field
                                                                                                                .state
                                                                                                                .value,
                                                                                                            item.delivery_line_id,
                                                                                                            event
                                                                                                                .target
                                                                                                                .value,
                                                                                                        ),
                                                                                                    )
                                                                                                }
                                                                                                onBlur={() => {
                                                                                                    const current =
                                                                                                        field.state.value.find(
                                                                                                            (
                                                                                                                row,
                                                                                                            ) =>
                                                                                                                row.delivery_line_id ===
                                                                                                                item.delivery_line_id,
                                                                                                        )
                                                                                                    if (
                                                                                                        current
                                                                                                    )
                                                                                                        field.handleChange(
                                                                                                            changePackageItem(
                                                                                                                field
                                                                                                                    .state
                                                                                                                    .value,
                                                                                                                item.delivery_line_id,
                                                                                                                normalizePackingQuantity(
                                                                                                                    current.quantity,
                                                                                                                ),
                                                                                                            ),
                                                                                                        )
                                                                                                }}
                                                                                            />
                                                                                        )}
                                                                                    </form.Field>
                                                                                    <span className="text-xs text-muted-foreground">
                                                                                        {unit(
                                                                                            item.delivery_line_id,
                                                                                        )}{" "}
                                                                                        ·
                                                                                        其他包裹已分配{" "}
                                                                                        {
                                                                                            other
                                                                                        }{" "}
                                                                                        ·{" "}
                                                                                        {balanceNote(
                                                                                            values,
                                                                                            destination.key,
                                                                                            item.delivery_line_id,
                                                                                        ) ||
                                                                                            "已全部分配"}
                                                                                    </span>
                                                                                </div>
                                                                            )
                                                                        },
                                                                    )}
                                                                </div>
                                                            ) : (
                                                                <div className="flex flex-wrap items-baseline gap-x-4 gap-y-1 text-xs text-muted-foreground">
                                                                    <span>
                                                                        包含：
                                                                        {parcel
                                                                            .items
                                                                            .length
                                                                            ? parcel.items
                                                                                  .map(
                                                                                      (
                                                                                          item,
                                                                                      ) =>
                                                                                          `${itemName(item.delivery_line_id)} × ${packingQuantity(item.quantity)} ${unit(item.delivery_line_id)}`,
                                                                                  )
                                                                                  .join(
                                                                                      " · ",
                                                                                  )
                                                                            : "尚未分配商品，请调整商品或移除此包裹"}
                                                                    </span>
                                                                    {automatic && (
                                                                        <span>
                                                                            数量随本次发货自动更新
                                                                        </span>
                                                                    )}
                                                                </div>
                                                            )}
                                                            {!automatic &&
                                                                parcels.length ===
                                                                    1 && (
                                                                    <Button
                                                                        id={`${prefix}-assign-remaining`}
                                                                        size="sm"
                                                                        variant="outline"
                                                                        disabled={
                                                                            disabled
                                                                        }
                                                                        onClick={() => {
                                                                            try {
                                                                                const items =
                                                                                    dispatchItems(
                                                                                        values,
                                                                                        destination.key,
                                                                                    )
                                                                                apply(
                                                                                    {
                                                                                        ...values,
                                                                                        packages:
                                                                                            values.packages.map(
                                                                                                (
                                                                                                    row,
                                                                                                ) =>
                                                                                                    row.id ===
                                                                                                    parcel.id
                                                                                                        ? {
                                                                                                              ...row,
                                                                                                              items,
                                                                                                          }
                                                                                                        : row,
                                                                                            ),
                                                                                    },
                                                                                )
                                                                            } catch {
                                                                                setEditing(
                                                                                    (
                                                                                        current,
                                                                                    ) =>
                                                                                        new Set(
                                                                                            [
                                                                                                ...current,
                                                                                                parcel.id,
                                                                                            ],
                                                                                        ),
                                                                                )
                                                                            }
                                                                        }}
                                                                    >
                                                                        将本次商品全部分配到此包裹
                                                                    </Button>
                                                                )}
                                                            {splitting ===
                                                                parcel.id && (
                                                                <FulfillmentPackageSplit
                                                                    key={
                                                                        parcel.id
                                                                    }
                                                                    packing={
                                                                        values
                                                                    }
                                                                    parcel={
                                                                        parcel
                                                                    }
                                                                    label={`包裹 ${index + 1}`}
                                                                    itemName={
                                                                        itemName
                                                                    }
                                                                    unit={unit}
                                                                    disabled={
                                                                        disabled
                                                                    }
                                                                    onApply={(
                                                                        next,
                                                                    ) => {
                                                                        following.current.delete(
                                                                            destination.key,
                                                                        )
                                                                        apply(
                                                                            next,
                                                                        )
                                                                    }}
                                                                    onClose={() =>
                                                                        setSplitting(
                                                                            null,
                                                                        )
                                                                    }
                                                                />
                                                            )}
                                                        </div>
                                                    )
                                                },
                                            )}
                                            {(() => {
                                                let remaining: ReturnType<
                                                    typeof unpackedItems
                                                > = []
                                                try {
                                                    remaining = unpackedItems(
                                                        values,
                                                        destination.key,
                                                    )
                                                } catch {}
                                                if (
                                                    parcels.length &&
                                                    !remaining.length
                                                )
                                                    return null
                                                return (
                                                    <div className="flex flex-wrap items-center gap-3 border-t border-border/50 pt-2">
                                                        <Button
                                                            id={`fulfillment-packing-address-${segment}-add-package`}
                                                            variant="outline"
                                                            size="sm"
                                                            disabled={
                                                                disabled ||
                                                                !remaining.length ||
                                                                values.packages
                                                                    .length >=
                                                                    100
                                                            }
                                                            onClick={() => {
                                                                following.current.delete(
                                                                    destination.key,
                                                                )
                                                                apply({
                                                                    ...values,
                                                                    packages: [
                                                                        ...values.packages,
                                                                        {
                                                                            id: crypto.randomUUID(),
                                                                            destination_key:
                                                                                destination.key,
                                                                            carrier:
                                                                                "",
                                                                            tracking_no:
                                                                                "",
                                                                            items: remaining.map(
                                                                                (
                                                                                    item,
                                                                                ) => ({
                                                                                    ...item,
                                                                                    quantity:
                                                                                        packingQuantity(
                                                                                            item.quantity,
                                                                                        ),
                                                                                }),
                                                                            ),
                                                                        },
                                                                    ],
                                                                })
                                                            }}
                                                        >
                                                            <PlusIcon />
                                                            {parcels.length
                                                                ? "将未分配商品装入新包裹"
                                                                : "登记首个包裹物流"}
                                                        </Button>
                                                        <span className="text-xs text-muted-foreground">
                                                            {parcels.length
                                                                ? "仅装入尚未分配的数量，原包裹保持不变"
                                                                : remaining.length
                                                                  ? "自动装入本次发货商品，填写物流即可"
                                                                  : "请先填写本次发货数量"}
                                                        </span>
                                                    </div>
                                                )
                                            })()}
                                        </div>
                                    </section>
                                )
                            })}
                        </>
                    )
                }}
            </form.Subscribe>
        </section>
    )
}
