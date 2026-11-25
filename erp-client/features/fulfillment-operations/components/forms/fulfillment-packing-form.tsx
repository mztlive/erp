"use client"

import {
    useEffect,
    useImperativeHandle,
    useMemo,
    useRef,
    useState,
    type Ref,
} from "react"
import {
    ChevronRightIcon,
    PlusIcon,
    PencilIcon,
    MapPinIcon,
    SearchIcon,
    Trash2Icon,
} from "lucide-react"
import { useQueryClient } from "@tanstack/react-query"
import {
    FulfillmentAddressDialog,
    type AddressAction,
} from "./fulfillment-address-dialog"
import { getDeliveryPackingDetail } from "../../api/packing"
import { fulfillmentKeys } from "../../hooks/queries"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { FulfillmentAddressList } from "./fulfillment-address-list"
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
import { FulfillmentPackageImport } from "./fulfillment-package-import"
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
    const queryClient = useQueryClient()
    const [search, setSearch] = useState("")
    const [onlyMissing, setOnlyMissing] = useState(false)
    const [productSearch, setProductSearch] = useState({
        address: "",
        query: "",
    })
    const [addressAction, setAddressAction] = useState<AddressAction | null>(
        null,
    )
    const [activeAddress, setActiveAddress] = useState<string | null>(null)
    const [editing, setEditing] = useState<Set<string>>(() => new Set())
    const [splitting, setSplitting] = useState<string | null>(null)
    const following = useRef(new Set<string>())
    const itemName = (id: string) =>
        operation.lines.find((line) => line.lineId === id)?.itemName ||
        "商品名称未提供"
    const unit = (id: string) =>
        operation.lines.find((line) => line.lineId === id)?.unitName || ""
    const addressStates = useMemo(
        () =>
            new Map(
                source.destinations.map((destination) => {
                    const dispatch = packing.dispatch.filter(
                        (row) => row.destination_key === destination.key,
                    )
                    const packages = packing.packages.filter(
                        (row) => row.destination_key === destination.key,
                    )
                    let skipped = false
                    try {
                        skipped =
                            !packages.length &&
                            !!dispatch[0]?.items.length &&
                            dispatch[0].items.every(
                                (item) =>
                                    compareDecimal(item.quantity, "0", 6) === 0,
                            )
                    } catch {
                        /* 数量录入未完成时继续显示待填写。 */
                    }
                    const ready =
                        !destination.pending_address &&
                        !packages.some(
                            (parcel) =>
                                parcel.carrier.trim() &&
                                parcel.tracking_no.trim() &&
                                packing.packages.some(
                                    (other) =>
                                        other.id !== parcel.id &&
                                        other.carrier.trim() ===
                                            parcel.carrier.trim() &&
                                        other.tracking_no.trim() ===
                                            parcel.tracking_no.trim(),
                                ),
                        ) &&
                        packingIssues(
                            { ...packing, dispatch, packages },
                            source,
                        ).length === 0
                    return [
                        destination.key,
                        {
                            ready,
                            skipped,
                            label: destination.pending_address
                                ? "待补地址"
                                : skipped
                                  ? "本次不发货"
                                  : ready
                                    ? "已填写"
                                    : packages.length
                                      ? "待完善"
                                      : "待填写",
                        },
                    ]
                }),
            ),
        [packing, source],
    )
    const visible = source.destinations.filter((row) => {
        const status = addressStates.get(row.key)!
        return (
            (!onlyMissing ||
                row.pending_address ||
                (!status.ready && !status.skipped)) &&
            `${row.recipient} ${row.phone} ${row.address} ${row.items.map((item) => itemName(item.delivery_line_id)).join(" ")}`
                .toLocaleLowerCase()
                .includes(search.trim().toLocaleLowerCase())
        )
    })
    const selectedDestination =
        source.destinations.find((row) => row.key === activeAddress) ??
        visible[0]
    const selectedOrder = source.destinations.findIndex(
        (row) => row.key === selectedDestination?.key,
    )
    const nextAddress = [
        ...visible.filter(
            (row) => source.destinations.indexOf(row) > selectedOrder,
        ),
        ...visible.filter(
            (row) => source.destinations.indexOf(row) < selectedOrder,
        ),
    ].find((row) => {
        const state = addressStates.get(row.key)
        return state && !state.ready && !state.skipped
    })
    const multipleAddresses = source.destinations.length > 1
    // 只为当前地址展示待填写包裹；切换地址不写草稿，已录入包裹始终保留。
    const formPacking = useMemo(() => {
        const destination = selectedDestination
        if (
            !destination ||
            destination.pending_address ||
            packing.packages.some(
                (row) => row.destination_key === destination.key,
            ) ||
            packing.packages.length >= 100 ||
            packing.source_version !== source.source_version ||
            draft.legacyTrackingNeedsAllocation
        )
            return packing
        try {
            const items = dispatchItems(packing, destination.key)
            if (!items.length) return packing
            return {
                ...packing,
                packages: [
                    ...packing.packages,
                    {
                        id: `initial-${destination.key}`,
                        destination_key: destination.key,
                        carrier: "",
                        tracking_no: "",
                        items,
                    },
                ],
            }
        } catch {
            return packing
        }
    }, [
        packing,
        source,
        selectedDestination,
        draft.legacyTrackingNeedsAllocation,
    ])
    const form = useAppForm({
        defaultValues: formPacking,
        listeners: {
            onChange: ({ formApi }) =>
                onChange({ ...draft, packing: formApi.state.values }),
        },
    })
    useEffect(() => {
        if (JSON.stringify(form.state.values) !== JSON.stringify(formPacking)) {
            following.current.clear()
            setEditing(new Set())
            setSplitting(null)
            form.reset(formPacking)
        }
    }, [form, formPacking])
    useImperativeHandle(ref, () => ({
        locate(targetId) {
            if (!targetId.startsWith("fulfillment-packing")) return false
            setSearch("")
            setOnlyMissing(false)
            setProductSearch({ address: "", query: "" })
            const targetPackage = form.state.values.packages.find((row) =>
                targetId.startsWith(
                    `fulfillment-packing-package-${toAutomationIdSegment(row.id)}-`,
                ),
            )
            const targetAddress =
                targetPackage?.destination_key ??
                source.destinations.find(
                    (row) =>
                        targetId.startsWith(
                            `fulfillment-packing-address-${toAutomationIdSegment(row.key)}-`,
                        ) ||
                        row.items.some(
                            (item) =>
                                targetId ===
                                `fulfillment-packing-${toAutomationIdSegment(row.key)}-${toAutomationIdSegment(item.delivery_line_id)}-quantity`,
                        ),
                )?.key
            if (targetAddress) setActiveAddress(targetAddress)
            let focusId = targetId
            const parcel = form.state.values.packages.find(
                (row) =>
                    targetId ===
                    `fulfillment-packing-package-${toAutomationIdSegment(row.id)}-edit-items`,
            )
            if (parcel) {
                setEditing((current) => new Set([...current, parcel.id]))
                const item = source.destinations.find(
                    (row) => row.key === parcel.destination_key,
                )?.items[0]
                if (item)
                    focusId = `fulfillment-packing-package-${toAutomationIdSegment(parcel.id)}-item-${toAutomationIdSegment(item.delivery_line_id)}-quantity`
            }
            requestAnimationFrame(() => {
                if (parcel)
                    setEditing((current) => new Set([...current, parcel.id]))
                requestAnimationFrame(() => {
                    const node = document.getElementById(focusId)
                    node?.scrollIntoView({ block: "center" })
                    const input = node?.matches(
                        "input, button, textarea, select",
                    )
                        ? node
                        : node?.querySelector<HTMLElement>(
                              "input:not(:disabled), button:not(:disabled), textarea:not(:disabled), select:not(:disabled)",
                          )
                    const focusTarget = input ?? node
                    focusTarget?.focus({ preventScroll: true })
                })
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
    const toggle = (key: string, setter: typeof setEditing) =>
        setter((current) => {
            const next = new Set(current)
            if (next.has(key)) next.delete(key)
            else next.add(key)
            return next
        })
    const reloadAddresses = async () => {
        const fresh = await queryClient.fetchQuery({
            queryKey: fulfillmentKeys.packing(operation.operationId),
            queryFn: () => getDeliveryPackingDetail(operation.operationId),
            staleTime: 0,
        })
        onChange(fresh.operation.draft)
        setActiveAddress(null)
    }
    const arrangementAction = selectedDestination ? (
        <Button
            id="fulfillment-packing-edit-address"
            type="button"
            variant="outline"
            size="sm"
            disabled={disabled}
            onClick={() =>
                setAddressAction(
                    selectedDestination.pending_address
                        ? { mode: "add" }
                        : { mode: "edit", destination: selectedDestination },
                )
            }
        >
            <MapPinIcon />
            {selectedDestination.pending_address ? "补充地址" : "调整当前地址"}
        </Button>
    ) : null
    return (
        <section
            id="fulfillment-packing-form"
            tabIndex={-1}
            className="flex min-h-0 min-w-0 flex-1 flex-col gap-3 outline-none"
            aria-label="按地址登记发货包裹"
        >
            <div className="flex shrink-0 flex-wrap justify-end gap-2">
                <FulfillmentPackageImport
                    operationId={operation.operationId}
                    source={source}
                    packing={packing}
                    itemName={itemName}
                    unit={unit}
                    disabled={disabled}
                    onApply={(next) => {
                        following.current.clear()
                        setEditing(new Set())
                        setSplitting(null)
                        apply(next)
                    }}
                />
            </div>
            {addressAction && (
                <FulfillmentAddressDialog
                    key={`${addressAction.mode}-${addressAction.mode === "add" ? "new" : addressAction.destination.key}`}
                    operationId={operation.operationId}
                    action={addressAction}
                    hasUnsavedChanges={
                        JSON.stringify(draft) !==
                        JSON.stringify(operation.draft)
                    }
                    onSaved={reloadAddresses}
                    onClose={() => setAddressAction(null)}
                />
            )}
            <form.Subscribe selector={(state) => state.values}>
                {(values) => {
                    return (
                        <>
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
                            <div className="grid min-h-0 min-w-0 flex-1 gap-5 lg:grid-cols-[15rem_minmax(0,1fr)] lg:overflow-hidden">
                                <FulfillmentAddressList
                                    destinations={visible}
                                    total={source.destinations.length}
                                    completed={
                                        [...addressStates.values()].filter(
                                            (state) => state.ready,
                                        ).length
                                    }
                                    states={addressStates}
                                    selectedKey={selectedDestination?.key}
                                    search={search}
                                    onlyMissing={onlyMissing}
                                    onSearch={(value) => {
                                        setSearch(value)
                                        setActiveAddress(null)
                                    }}
                                    onOnlyMissing={(value) => {
                                        setOnlyMissing(value)
                                        setActiveAddress(null)
                                    }}
                                    actions={
                                        <div className="flex items-center gap-1">
                                            <Button
                                                id="fulfillment-packing-add-address"
                                                type="button"
                                                variant="ghost"
                                                size="icon-sm"
                                                title="新增地址"
                                                aria-label="新增收货地址"
                                                disabled={disabled}
                                                onClick={() =>
                                                    setAddressAction({
                                                        mode: "add",
                                                    })
                                                }
                                            >
                                                <PlusIcon />
                                            </Button>
                                            <Button
                                                id="fulfillment-packing-delete-address"
                                                type="button"
                                                variant="ghost"
                                                size="icon-sm"
                                                title="删除选中地址"
                                                aria-label="删除选中收货地址"
                                                disabled={
                                                    disabled ||
                                                    !selectedDestination ||
                                                    selectedDestination.pending_address
                                                }
                                                onClick={() =>
                                                    selectedDestination &&
                                                    setAddressAction({
                                                        mode: "delete",
                                                        destination:
                                                            selectedDestination,
                                                    })
                                                }
                                            >
                                                <Trash2Icon />
                                            </Button>
                                        </div>
                                    }
                                    onSelect={setActiveAddress}
                                />
                                <div
                                    id="fulfillment-packing-address-detail"
                                    className="min-h-0 min-w-0 lg:overflow-y-auto lg:overscroll-contain lg:border-l lg:border-border/60 lg:px-5"
                                >
                                    {!selectedDestination && (
                                        <p className="py-12 text-center text-xs text-muted-foreground">
                                            没有符合条件的收货地址
                                        </p>
                                    )}
                                    {(selectedDestination
                                        ? [selectedDestination]
                                        : []
                                    ).map((destination) => {
                                        if (destination.pending_address)
                                            return (
                                                <section
                                                    key={destination.key}
                                                    className="space-y-3 rounded-xl border border-dashed bg-muted/20 p-4"
                                                >
                                                    <div>
                                                        <div className="flex flex-wrap items-center justify-between gap-2">
                                                            <h3 className="font-medium">
                                                                待补收货地址
                                                            </h3>
                                                            {arrangementAction}
                                                        </div>
                                                        <p className="text-xs text-muted-foreground">
                                                            以下商品本次暂不发货。补齐地址后可登记包裹，其他商品可以先发。
                                                        </p>
                                                    </div>
                                                    {destination.items.map(
                                                        (item) => (
                                                            <div
                                                                key={
                                                                    item.delivery_line_id
                                                                }
                                                                className="flex justify-between gap-4 text-body-compact"
                                                            >
                                                                <span>
                                                                    {itemName(
                                                                        item.delivery_line_id,
                                                                    )}
                                                                </span>
                                                                <span>
                                                                    {packingQuantity(
                                                                        item.quantity,
                                                                    )}{" "}
                                                                    {unit(
                                                                        item.delivery_line_id,
                                                                    )}
                                                                </span>
                                                            </div>
                                                        ),
                                                    )}
                                                </section>
                                            )
                                        const segment = toAutomationIdSegment(
                                            destination.key,
                                        )
                                        const dispatchIndex =
                                            values.dispatch.findIndex(
                                                (row) =>
                                                    row.destination_key ===
                                                    destination.key,
                                            )
                                        const parcels = values.packages
                                            .map((parcel, index) => ({
                                                parcel,
                                                index,
                                            }))
                                            .filter(
                                                ({ parcel }) =>
                                                    parcel.destination_key ===
                                                    destination.key,
                                            )
                                        const query =
                                            destination.items.length > 5 &&
                                            productSearch.address ===
                                                destination.key
                                                ? productSearch.query
                                                : ""
                                        const matchesProduct = (id: string) =>
                                            itemName(id)
                                                .toLocaleLowerCase()
                                                .includes(
                                                    query
                                                        .trim()
                                                        .toLocaleLowerCase(),
                                                )
                                        const matchedItems =
                                            destination.items.filter((item) =>
                                                matchesProduct(
                                                    item.delivery_line_id,
                                                ),
                                            )
                                        const showAllocation =
                                            parcels.length > 1 ||
                                            (parcels.length === 1 &&
                                                !automaticPackage(
                                                    values,
                                                    destination.key,
                                                )) ||
                                            !!draft.legacyTrackingNeedsAllocation
                                        return (
                                            <section
                                                key={destination.key}
                                                className="min-w-0"
                                                aria-label={`${destination.recipient}的发货明细`}
                                                onFocusCapture={() =>
                                                    setActiveAddress(
                                                        destination.key,
                                                    )
                                                }
                                            >
                                                <div className="flex flex-wrap items-start justify-between gap-3 border-b border-border pb-4">
                                                    <div className="min-w-0 flex-1">
                                                        <div className="flex flex-wrap items-baseline gap-x-4 gap-y-1">
                                                            <h3 className="text-body-compact font-semibold">
                                                                {
                                                                    destination.recipient
                                                                }
                                                            </h3>
                                                            <span className="text-body-compact">
                                                                {
                                                                    destination.phone
                                                                }
                                                            </span>
                                                        </div>
                                                        <p className="mt-1.5 break-words text-xs text-muted-foreground">
                                                            {
                                                                destination.address
                                                            }
                                                        </p>
                                                    </div>
                                                    <div className="flex flex-wrap items-center gap-2">
                                                        {arrangementAction}
                                                        {multipleAddresses && (
                                                            <>
                                                                <span className="text-xs text-muted-foreground">
                                                                    地址{" "}
                                                                    {selectedOrder +
                                                                        1}
                                                                    /
                                                                    {
                                                                        source
                                                                            .destinations
                                                                            .length
                                                                    }
                                                                </span>
                                                                <Button
                                                                    id="fulfillment-packing-next-address"
                                                                    type="button"
                                                                    variant="outline"
                                                                    size="sm"
                                                                    disabled={
                                                                        !nextAddress
                                                                    }
                                                                    onClick={() =>
                                                                        nextAddress &&
                                                                        setActiveAddress(
                                                                            nextAddress.key,
                                                                        )
                                                                    }
                                                                >
                                                                    下一未完成{" "}
                                                                    <ChevronRightIcon />
                                                                </Button>
                                                            </>
                                                        )}
                                                    </div>
                                                </div>
                                                <div className="min-w-0">
                                                    {parcels.map(
                                                        ({ parcel, index }) => {
                                                            const prefix = `fulfillment-packing-package-${toAutomationIdSegment(parcel.id)}`
                                                            const automatic =
                                                                automaticPackage(
                                                                    values,
                                                                    destination.key,
                                                                )
                                                            const editingItems =
                                                                editing.has(
                                                                    parcel.id,
                                                                )
                                                            return (
                                                                <div
                                                                    key={
                                                                        parcel.id
                                                                    }
                                                                    id={
                                                                        parcel.id ===
                                                                        parcels[0]
                                                                            .parcel
                                                                            .id
                                                                            ? `fulfillment-packing-address-${segment}-logistics`
                                                                            : undefined
                                                                    }
                                                                    tabIndex={
                                                                        -1
                                                                    }
                                                                    className="space-y-3 py-4 outline-none"
                                                                >
                                                                    <div className="flex flex-wrap items-center gap-3">
                                                                        <span className="text-body-compact font-medium">
                                                                            {parcels.length ===
                                                                            1
                                                                                ? "物流"
                                                                                : `包裹 ${index + 1}`}
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
                                                                                    className="w-full sm:w-44"
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
                                                                                    className="w-full sm:min-w-48 sm:flex-1"
                                                                                />
                                                                            )}
                                                                        </form.Field>
                                                                        <div className="ml-auto flex flex-wrap items-center gap-1">
                                                                            <Button
                                                                                id={`${prefix}-split`}
                                                                                size="sm"
                                                                                variant="secondary"
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
                                                                            {packing.packages.some(
                                                                                (
                                                                                    row,
                                                                                ) =>
                                                                                    row.id ===
                                                                                    parcel.id,
                                                                            ) && (
                                                                                <Button
                                                                                    id={`${prefix}-remove`}
                                                                                    aria-label={`移除包裹 ${index + 1}`}
                                                                                    title={`移除包裹 ${index + 1}`}
                                                                                    size="icon-sm"
                                                                                    variant="ghost"
                                                                                    disabled={
                                                                                        disabled
                                                                                    }
                                                                                    onClick={() => {
                                                                                        setSplitting(
                                                                                            null,
                                                                                        )
                                                                                        setEditing(
                                                                                            (
                                                                                                current,
                                                                                            ) => {
                                                                                                const next =
                                                                                                    new Set(
                                                                                                        current,
                                                                                                    )
                                                                                                next.delete(
                                                                                                    parcel.id,
                                                                                                )
                                                                                                return next
                                                                                            },
                                                                                        )
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
                                                                                    <Trash2Icon />
                                                                                </Button>
                                                                            )}
                                                                        </div>
                                                                    </div>
                                                                    {editingItems ? (
                                                                        <div className="max-h-80 space-y-3 overflow-y-auto overscroll-contain border-t pt-2">
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
                                                                                                        className="w-24"
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
                                                                    ) : showAllocation ? (
                                                                        <details className="text-xs text-muted-foreground">
                                                                            <summary
                                                                                id={`${prefix}-contents`}
                                                                                className="cursor-pointer"
                                                                            >
                                                                                {
                                                                                    parcel
                                                                                        .items
                                                                                        .length
                                                                                }{" "}
                                                                                种商品
                                                                            </summary>
                                                                            <div className="mt-2 max-h-40 space-y-1 overflow-y-auto overscroll-contain">
                                                                                {parcel
                                                                                    .items
                                                                                    .length ? (
                                                                                    parcel.items.map(
                                                                                        (
                                                                                            item,
                                                                                        ) => (
                                                                                            <p
                                                                                                key={
                                                                                                    item.delivery_line_id
                                                                                                }
                                                                                            >
                                                                                                {itemName(
                                                                                                    item.delivery_line_id,
                                                                                                )}{" "}
                                                                                                ×{" "}
                                                                                                {packingQuantity(
                                                                                                    item.quantity,
                                                                                                )}{" "}
                                                                                                {unit(
                                                                                                    item.delivery_line_id,
                                                                                                )}
                                                                                            </p>
                                                                                        ),
                                                                                    )
                                                                                ) : (
                                                                                    <p>
                                                                                        尚未分配商品，请调整商品或移除此包裹
                                                                                    </p>
                                                                                )}
                                                                            </div>
                                                                        </details>
                                                                    ) : null}
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
                                                                            unit={
                                                                                unit
                                                                            }
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
                                                            remaining =
                                                                unpackedItems(
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
                                                            <div
                                                                id={
                                                                    !parcels.length
                                                                        ? `fulfillment-packing-address-${segment}-logistics`
                                                                        : undefined
                                                                }
                                                                tabIndex={-1}
                                                                className="flex flex-wrap items-center gap-3 border-t border-border/50 pt-2 outline-none"
                                                            >
                                                                <Button
                                                                    id={`fulfillment-packing-address-${segment}-add-package`}
                                                                    variant="outline"
                                                                    size="sm"
                                                                    disabled={
                                                                        disabled ||
                                                                        !remaining.length ||
                                                                        values
                                                                            .packages
                                                                            .length >=
                                                                            100
                                                                    }
                                                                    onClick={() => {
                                                                        following.current.delete(
                                                                            destination.key,
                                                                        )
                                                                        apply({
                                                                            ...values,
                                                                            packages:
                                                                                [
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
                                                    <div className="min-w-0 border-t border-border">
                                                        <div className="flex flex-wrap items-center justify-between gap-3 py-3">
                                                            <h4 className="text-body-compact font-semibold">
                                                                商品 ·{" "}
                                                                {
                                                                    destination
                                                                        .items
                                                                        .length
                                                                }
                                                            </h4>
                                                            {destination.items
                                                                .length > 5 && (
                                                                <div className="relative w-full sm:w-52">
                                                                    <SearchIcon
                                                                        aria-hidden="true"
                                                                        className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
                                                                    />
                                                                    <Input
                                                                        id={`fulfillment-packing-${segment}-product-search`}
                                                                        aria-label="搜索当前地址的商品"
                                                                        placeholder="搜索商品"
                                                                        className="pl-8"
                                                                        value={
                                                                            query
                                                                        }
                                                                        onChange={(
                                                                            event,
                                                                        ) =>
                                                                            setProductSearch(
                                                                                {
                                                                                    address:
                                                                                        destination.key,
                                                                                    query: event
                                                                                        .target
                                                                                        .value,
                                                                                },
                                                                            )
                                                                        }
                                                                    />
                                                                </div>
                                                            )}
                                                        </div>
                                                        <div className="[&_[data-slot=table-container]]:overflow-x-auto">
                                                            <Table
                                                                className={
                                                                    showAllocation
                                                                        ? "min-w-[48rem] table-fixed"
                                                                        : "min-w-[34rem] table-fixed"
                                                                }
                                                            >
                                                                <colgroup>
                                                                    <col />
                                                                    <col className="w-32 sm:w-40" />
                                                                    <col className="w-36 sm:w-44" />
                                                                    {showAllocation && (
                                                                        <col className="w-44 sm:w-56" />
                                                                    )}
                                                                </colgroup>
                                                                <TableHeader className="[&_th]:sticky [&_th]:top-0 [&_th]:z-10">
                                                                    <TableRow>
                                                                        <TableHead>
                                                                            商品
                                                                        </TableHead>
                                                                        <TableHead>
                                                                            待发
                                                                        </TableHead>
                                                                        <TableHead>
                                                                            本次发货
                                                                        </TableHead>
                                                                        {showAllocation && (
                                                                            <TableHead>
                                                                                包裹分配
                                                                            </TableHead>
                                                                        )}
                                                                    </TableRow>
                                                                </TableHeader>
                                                                <TableBody>
                                                                    {destination.items.map(
                                                                        (
                                                                            item,
                                                                        ) => {
                                                                            const itemIndex =
                                                                                values.dispatch[
                                                                                    dispatchIndex
                                                                                ]?.items.findIndex(
                                                                                    (
                                                                                        value,
                                                                                    ) =>
                                                                                        value.delivery_line_id ===
                                                                                        item.delivery_line_id,
                                                                                ) ??
                                                                                -1
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
                                                                                    hidden={
                                                                                        !matchesProduct(
                                                                                            item.delivery_line_id,
                                                                                        )
                                                                                    }
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
                                                                                                            className="w-24"
                                                                                                        />
                                                                                                    )}
                                                                                                </form.Field>
                                                                                            )}
                                                                                    </TableCell>
                                                                                    {showAllocation && (
                                                                                        <TableCell className="whitespace-normal text-body-compact">
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
                                                                                    )}
                                                                                </TableRow>
                                                                            )
                                                                        },
                                                                    )}
                                                                </TableBody>
                                                            </Table>
                                                        </div>
                                                        {!matchedItems.length && (
                                                            <p className="px-4 py-6 text-center text-xs text-muted-foreground">
                                                                没有匹配的商品，请更换搜索词。
                                                            </p>
                                                        )}
                                                        <div className="border-t border-border/60 py-2 text-xs text-muted-foreground">
                                                            {query.trim()
                                                                ? `${matchedItems.length} / ${destination.items.length} 种商品`
                                                                : `共 ${destination.items.length} 种商品`}
                                                        </div>
                                                    </div>
                                                </div>
                                            </section>
                                        )
                                    })}
                                </div>
                            </div>
                        </>
                    )
                }}
            </form.Subscribe>
        </section>
    )
}
