"use client"

import { useState, type RefObject } from "react"
import {
    CopyIcon,
    CrosshairIcon,
    MapPinIcon,
    SearchIcon,
    XIcon,
} from "lucide-react"

import { QuantityValue, detailWorkspaceStyles } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { toast } from "@/components/ui/toast"
import { RegionName } from "@/features/regions"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { fullDeliveryAddress } from "@/lib/delivery-address"
import {
    deliveryAddressIdentityKey,
    filterDeliveryAddressGroups,
    filterDeliveryItemGroups,
    type DeliveryAddressGroup,
    type DeliveryAllocationSummary,
} from "@/lib/delivery-allocation-summary"
import { compactFixed } from "@/lib/fixed-decimal"

export function WorkspaceSalesDeliveryDialog({
    idPrefix,
    summary,
    open,
    onOpenChange,
    onCloseComplete,
    returnFocus,
    focusedLineId,
    onClearFocus,
    onLocateLine,
}: {
    idPrefix: string
    summary: DeliveryAllocationSummary
    open: boolean
    onOpenChange: (open: boolean) => void
    onCloseComplete: () => void
    returnFocus: RefObject<HTMLElement | null>
    focusedLineId: string | null
    onClearFocus: () => void
    onLocateLine: (id: string) => void
}) {
    const [sessionKey, setSessionKey] = useState(0)

    const handleOpenChange = (nextOpen: boolean) => {
        if (!nextOpen) {
            setSessionKey((current) => current + 1)
        }
        onOpenChange(nextOpen)
    }

    return (
        <Dialog
            open={open}
            onOpenChange={handleOpenChange}
            onOpenChangeComplete={(nextOpen) => {
                if (!nextOpen) onCloseComplete()
            }}
        >
            <DialogContent
                size="lg"
                scroll="body"
                closeButtonId={`${idPrefix}-close`}
                finalFocus={returnFocus}
            >
                <DialogHeader>
                    <DialogTitle>配送地址明细</DialogTitle>
                    <DialogDescription className="num">
                        共 {summary.uniqueAddressCount} 处收货地址 ·{" "}
                        {summary.totalRows} 条分配
                    </DialogDescription>
                </DialogHeader>
                <DialogBody className="space-y-3">
                    <SalesDeliveryViews
                        key={sessionKey}
                        idPrefix={idPrefix}
                        summary={summary}
                        focusedLineId={focusedLineId}
                        onClearFocus={onClearFocus}
                        onLocateLine={onLocateLine}
                    />
                </DialogBody>
                <DialogFooter
                    showCloseButton
                    closeButtonId={`${idPrefix}-footer-close`}
                />
            </DialogContent>
        </Dialog>
    )
}

export function SalesDeliveryViews({
    idPrefix,
    summary,
    focusedLineId,
    onClearFocus,
    onLocateLine,
}: {
    idPrefix: string
    summary: DeliveryAllocationSummary
    focusedLineId: string | null
    onClearFocus: () => void
    onLocateLine: (id: string) => void
}) {
    const [view, setView] = useState<"address" | "item">("address")
    const [search, setSearch] = useState("")
    const focusedLine = summary.itemGroups.find(
        ({ lineId }) => lineId === focusedLineId,
    )

    const singleAddress =
        summary.isUniformSingleAddress && !focusedLine
            ? summary.addressGroups[0]
            : undefined
    if (singleAddress) {
        return (
            <div className="space-y-1.5 rounded-lg bg-surface-sunken p-4">
                <AddressHeading
                    group={singleAddress}
                    copyId={`${idPrefix}-copy-single`}
                />
                <p className="num text-xs text-muted-foreground">
                    整单统一收货 · 覆盖 {summary.assignedLineCount} 项商品
                </p>
            </div>
        )
    }

    const filteredAddresses = filterDeliveryAddressGroups(
        summary.addressGroups,
        search,
        focusedLine?.lineId,
    )
    const filteredItems = filterDeliveryItemGroups(
        summary.itemGroups,
        search,
        focusedLine?.lineId,
    )

    return (
        <>
            {focusedLine ? (
                <div className="flex flex-wrap items-center justify-between gap-2 rounded-md bg-info-soft/55 px-3 py-2 text-xs text-info-soft-foreground">
                    <span className="num min-w-0 wrap-anywhere">
                        第 {focusedLine.lineNo} 行「{focusedLine.itemName}」 ·{" "}
                        {focusedLine.uniqueAddressCount} 处
                    </span>
                    <Button
                        id={`${idPrefix}-clear-focus`}
                        type="button"
                        variant="ghost"
                        size="xs"
                        className="h-control-xs px-2 text-xs text-info-soft-foreground hover:bg-info-soft hover:text-info-soft-foreground"
                        onClick={() => {
                            setSearch("")
                            onClearFocus()
                        }}
                    >
                        <XIcon data-icon="inline-start" aria-hidden="true" />
                        查看整单全部地址
                    </Button>
                </div>
            ) : null}

            <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex flex-wrap items-center gap-2">
                    <div
                        role="group"
                        aria-label="配送地址查看维度"
                        className={detailWorkspaceStyles.subTabs}
                    >
                        <button
                            id={`${idPrefix}-view-address`}
                            type="button"
                            aria-pressed={view === "address"}
                            data-active={
                                view === "address" ? "true" : undefined
                            }
                            className={detailWorkspaceStyles.subTab}
                            onClick={() => setView("address")}
                        >
                            按收货地址
                        </button>
                        <button
                            id={`${idPrefix}-view-item`}
                            type="button"
                            aria-pressed={view === "item"}
                            data-active={view === "item" ? "true" : undefined}
                            className={detailWorkspaceStyles.subTab}
                            onClick={() => setView("item")}
                        >
                            按商品
                        </button>
                    </div>
                    {search.trim() ? (
                        <span
                            role="status"
                            aria-live="polite"
                            className="num text-xs text-muted-foreground"
                        >
                            {view === "address"
                                ? `匹配 ${filteredAddresses.length} / ${summary.uniqueAddressCount} 处地址`
                                : `匹配 ${filteredItems.length} / ${summary.itemGroups.length} 项商品`}
                        </span>
                    ) : null}
                </div>
                {summary.uniqueAddressCount > 3 || search ? (
                    <div className="relative w-60 max-w-full">
                        <SearchIcon
                            aria-hidden="true"
                            className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                        />
                        <Input
                            id={`${idPrefix}-search`}
                            size="sm"
                            aria-label="搜索地址、收货人或商品"
                            placeholder="搜索地址、收货人或商品"
                            value={search}
                            onChange={(event) =>
                                setSearch(event.currentTarget.value)
                            }
                            className="pr-8 pl-8"
                        />
                        {search ? (
                            <Button
                                id={`${idPrefix}-search-clear`}
                                type="button"
                                variant="ghost"
                                size="icon-xs"
                                aria-label="清除配送地址搜索"
                                className="absolute top-1/2 right-1 -translate-y-1/2"
                                onClick={() => setSearch("")}
                            >
                                <XIcon aria-hidden="true" />
                            </Button>
                        ) : null}
                    </div>
                ) : null}
            </div>
            {!focusedLine && summary.uniqueAddressCount > 3 ? (
                <ul
                    aria-label="收货地区分布"
                    className="num flex flex-wrap items-center gap-1.5 text-tiny text-muted-foreground"
                >
                    {summary.regionDistribution
                        .slice(0, 5)
                        .map(({ label, count }) => (
                            <li
                                key={label}
                                className="rounded-sm bg-surface-sunken px-2 py-1"
                            >
                                {label} · {count} 处
                            </li>
                        ))}
                    {summary.regionDistribution.length > 5 ? (
                        <li>等 {summary.regionDistribution.length} 个地区</li>
                    ) : null}
                </ul>
            ) : null}
            {!focusedLine && summary.unassignedLineCount > 0 ? (
                <p className="num text-xs text-warning-soft-foreground">
                    {summary.unassignedLineCount} 项商品未记录收货地址
                </p>
            ) : null}
            {view === "address" ? (
                <DeliveryGroups
                    groups={filteredAddresses}
                    groupKey={(group) => group.key}
                    renderGroup={(group) => {
                        const addressPrefix = `${idPrefix}-address-${addressIdSegment(group.key)}`
                        return (
                            <>
                                <AddressHeading
                                    group={group}
                                    copyId={`${addressPrefix}-copy`}
                                />
                                <ul className="mt-1.5 space-y-1 text-xs">
                                    {group.items.map((item) => (
                                        <li
                                            key={item.lineId}
                                            className="flex flex-wrap items-center gap-x-2 gap-y-0.5"
                                        >
                                            <span className="num min-w-0 wrap-anywhere">
                                                #{item.lineNo} {item.itemName}
                                                {item.specification
                                                    ? ` · ${item.specification}`
                                                    : ""}
                                            </span>
                                            <QuantityValue
                                                value={item.quantity}
                                                unit={item.unit}
                                            />
                                            <LocateLineButton
                                                id={`${addressPrefix}-locate-${toAutomationIdSegment(item.lineId)}`}
                                                lineNo={item.lineNo}
                                                onClick={() =>
                                                    onLocateLine(item.lineId)
                                                }
                                            />
                                        </li>
                                    ))}
                                </ul>
                            </>
                        )
                    }}
                />
            ) : (
                <DeliveryGroups
                    groups={filteredItems}
                    groupKey={(group) => group.lineId}
                    renderGroup={(group) => {
                        const lineSegment = toAutomationIdSegment(group.lineId)
                        return (
                            <>
                                <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
                                    <h3 className="num text-body-compact font-medium wrap-anywhere">
                                        #{group.lineNo} {group.itemName}
                                        {group.specification
                                            ? ` · ${group.specification}`
                                            : ""}
                                    </h3>
                                    <span className="text-xs text-muted-foreground">
                                        已分配{" "}
                                        <QuantityValue
                                            value={group.allocatedTotal}
                                            unit={group.unit}
                                        />
                                    </span>
                                    <LocateLineButton
                                        id={`${idPrefix}-item-locate-${lineSegment}`}
                                        lineNo={group.lineNo}
                                        onClick={() =>
                                            onLocateLine(group.lineId)
                                        }
                                    />
                                </div>
                                {group.allocations.length > 0 ? (
                                    <ul className="mt-1.5 space-y-1 text-xs">
                                        {group.allocations.map(
                                            (allocation, index) => {
                                                const allocKey =
                                                    deliveryAddressIdentityKey(
                                                        allocation,
                                                    )
                                                const allocSeg = `${addressIdSegment(allocKey)}-${index}`
                                                return (
                                                    <li
                                                        key={JSON.stringify([
                                                            allocKey,
                                                            allocation.quantity,
                                                            index,
                                                        ])}
                                                        className="flex flex-wrap items-center justify-between gap-x-2 gap-y-0.5"
                                                    >
                                                        <span className="num min-w-0 flex-1 wrap-anywhere">
                                                            {[
                                                                allocation.recipient,
                                                                allocation.phone,
                                                            ]
                                                                .filter(Boolean)
                                                                .join(" · ")}
                                                            {allocation.recipient ||
                                                            allocation.phone
                                                                ? " · "
                                                                : null}
                                                            <AddressText
                                                                regionCode={
                                                                    allocation.region_code
                                                                }
                                                                regionName={
                                                                    allocation.region_name
                                                                }
                                                                address={
                                                                    allocation.address
                                                                }
                                                            />
                                                        </span>
                                                        <span className="inline-flex shrink-0 items-center gap-1.5">
                                                            <QuantityValue
                                                                value={compactFixed(
                                                                    allocation.quantity,
                                                                )}
                                                                unit={
                                                                    group.unit
                                                                }
                                                            />
                                                            <Button
                                                                id={`${idPrefix}-item-${lineSegment}-copy-${allocSeg}`}
                                                                type="button"
                                                                variant="ghost"
                                                                size="xs"
                                                                aria-label={`复制 ${allocation.recipient} 的收货地址`}
                                                                className="h-control-xs px-1.5 text-xs text-muted-foreground hover:text-foreground"
                                                                onClick={() =>
                                                                    void copyAddressText(
                                                                        allocation.recipient,
                                                                        allocation.phone,
                                                                        fullDeliveryAddress(
                                                                            allocation,
                                                                        ),
                                                                    )
                                                                }
                                                            >
                                                                <CopyIcon aria-hidden="true" />
                                                                <span className="sr-only">
                                                                    复制地址
                                                                </span>
                                                            </Button>
                                                        </span>
                                                    </li>
                                                )
                                            },
                                        )}
                                    </ul>
                                ) : (
                                    <p className="mt-1 text-xs text-muted-foreground">
                                        未记录收货地址
                                    </p>
                                )}
                            </>
                        )
                    }}
                />
            )}
        </>
    )
}

function DeliveryGroups<Group>({
    groups,
    groupKey,
    renderGroup,
}: {
    groups: readonly Group[]
    groupKey: (group: Group) => string
    renderGroup: (group: Group) => React.ReactNode
}) {
    if (!groups.length) {
        return (
            <p className="py-6 text-center text-xs text-muted-foreground">
                没有匹配的配送记录。
            </p>
        )
    }

    return (
        <ol className="divide-y divide-border/60">
            {groups.map((group) => (
                <li key={groupKey(group)} className="py-2.5">
                    {renderGroup(group)}
                </li>
            ))}
        </ol>
    )
}

function addressIdSegment(addressKey: string): string {
    let hash = 2166136261
    for (const character of addressKey) {
        hash = Math.imul(hash ^ character.codePointAt(0)!, 16777619)
    }
    return toAutomationIdSegment((hash >>> 0).toString(36))
}

function AddressText({
    regionCode,
    regionName,
    address,
}: {
    regionCode: string
    regionName?: string | null
    address: string
}) {
    const cleanRegion = regionName?.trim()
    return (
        <>
            {!cleanRegion && regionCode ? (
                <RegionName code={regionCode} />
            ) : null}
            {fullDeliveryAddress({ region_name: cleanRegion, address })}
        </>
    )
}

async function copyAddressText(
    recipient: string,
    phone: string,
    fullAddress: string,
) {
    if (typeof navigator === "undefined" || !navigator.clipboard) {
        toast.add({ title: "当前环境不支持一键复制", type: "warning" })
        return
    }
    try {
        await navigator.clipboard.writeText(
            [recipient, phone, fullAddress].filter(Boolean).join(" "),
        )
        toast.add({ title: "收货地址已复制", type: "success" })
    } catch {
        toast.add({ title: "复制失败，请选中地址手动复制", type: "error" })
    }
}

function AddressHeading({
    group,
    copyId,
}: {
    group: DeliveryAddressGroup
    copyId: string
}) {
    return (
        <div className="flex flex-wrap items-start justify-between gap-x-3 gap-y-1">
            <div className="flex min-w-0 flex-1 items-start gap-2">
                <MapPinIcon
                    aria-hidden="true"
                    className="mt-0.5 size-4 shrink-0 text-primary"
                />
                <div className="num min-w-0 wrap-anywhere">
                    <div className="text-body-compact font-medium">
                        {group.recipient}{" "}
                        <span className="font-normal text-muted-foreground">
                            {group.phone}
                        </span>
                    </div>
                    <p className="text-xs">
                        <AddressText
                            regionCode={group.regionCode}
                            regionName={group.regionName}
                            address={group.address}
                        />
                    </p>
                </div>
            </div>
            <Button
                id={copyId}
                type="button"
                variant="ghost"
                size="xs"
                onClick={() =>
                    void copyAddressText(
                        group.recipient,
                        group.phone,
                        group.fullAddress,
                    )
                }
            >
                <CopyIcon aria-hidden="true" data-icon="inline-start" />
                复制地址
            </Button>
        </div>
    )
}

function LocateLineButton({
    id,
    lineNo,
    onClick,
}: {
    id: string
    lineNo: number
    onClick: () => void
}) {
    return (
        <Button
            id={id}
            type="button"
            variant="ghost"
            size="xs"
            aria-label={`定位第 ${lineNo} 行商品`}
            onClick={onClick}
        >
            <CrosshairIcon aria-hidden="true" data-icon="inline-start" />
            定位商品行
        </Button>
    )
}
