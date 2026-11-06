"use client"

import { useEffect, useRef } from "react"
import {
    CheckIcon,
    CircleIcon,
    CircleAlertIcon,
    MinusIcon,
    MapPinIcon,
    SearchIcon,
} from "lucide-react"
import { Input } from "@/components/ui/input"
import { Checkbox } from "@/components/ui/checkbox"
import { Label } from "@/components/ui/label"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import type { DeliveryDestination } from "../../lib/delivery-packing"

export type AddressEntryState = {
    ready: boolean
    skipped: boolean
    label: string
}

/** 地址导航独立滚动；选择只切换右侧编辑区，不提交或清空草稿。 */
export function FulfillmentAddressList({
    destinations,
    total,
    completed,
    states,
    selectedKey,
    search,
    onlyMissing,
    onSearch,
    onOnlyMissing,
    onSelect,
}: {
    destinations: readonly DeliveryDestination[]
    total: number
    completed: number
    states: ReadonlyMap<string, AddressEntryState>
    selectedKey?: string
    search: string
    onlyMissing: boolean
    onSearch: (value: string) => void
    onOnlyMissing: (value: boolean) => void
    onSelect: (key: string) => void
}) {
    const listRef = useRef<HTMLDivElement>(null)
    const selectedVisible = destinations.some((row) => row.key === selectedKey)
    useEffect(() => {
        const list = listRef.current
        const selected = list?.querySelector<HTMLElement>(
            '[aria-current="true"]',
        )
        if (!list || !selected) return
        const bounds = list.getBoundingClientRect()
        const item = selected.getBoundingClientRect()
        if (item.top < bounds.top) list.scrollTop -= bounds.top - item.top
        else if (item.bottom > bounds.bottom)
            list.scrollTop += item.bottom - bounds.bottom
    }, [selectedKey])
    return (
        <aside aria-label="收货地址导航" className="min-w-0 self-start">
            <div className="space-y-3 pb-3">
                <div className="flex items-center justify-between gap-2 text-sm">
                    <span className="font-medium">
                        收货地址 ·{" "}
                        <span className="text-muted-foreground">{total}</span>
                    </span>
                    <span className="text-xs text-muted-foreground">
                        已填 {completed}/{total}
                    </span>
                </div>
                <div className="relative">
                    <SearchIcon
                        aria-hidden="true"
                        className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
                    />
                    <Input
                        id="fulfillment-packing-search"
                        aria-label="搜索收货人、电话、地址或商品"
                        placeholder="搜索地址"
                        value={search}
                        onChange={(event) => onSearch(event.target.value)}
                        className="pl-8"
                    />
                </div>
                <Label
                    htmlFor="fulfillment-packing-only-missing"
                    className="text-xs font-normal"
                >
                    <Checkbox
                        id="fulfillment-packing-only-missing"
                        checked={onlyMissing}
                        onCheckedChange={onOnlyMissing}
                    />
                    未完成
                </Label>
            </div>
            <div
                ref={listRef}
                className="relative max-h-52 overflow-y-auto overscroll-contain lg:max-h-[34rem]"
            >
                {destinations.map((destination) => {
                    const status = states.get(destination.key)!
                    const selected = selectedKey === destination.key
                    return (
                        <button
                            key={destination.key}
                            id={`fulfillment-packing-select-address-${toAutomationIdSegment(destination.key)}`}
                            type="button"
                            onKeyDown={(event) => {
                                if (
                                    ![
                                        "ArrowUp",
                                        "ArrowDown",
                                        "Home",
                                        "End",
                                    ].includes(event.key) ||
                                    !destinations.length
                                )
                                    return
                                const index = destinations.findIndex(
                                    (row) => row.key === selectedKey,
                                )
                                const next =
                                    event.key === "Home"
                                        ? 0
                                        : event.key === "End"
                                          ? destinations.length - 1
                                          : event.key === "ArrowDown"
                                            ? Math.min(
                                                  index + 1,
                                                  destinations.length - 1,
                                              )
                                            : Math.max(index - 1, 0)
                                const destination = destinations[next]
                                event.preventDefault()
                                onSelect(destination.key)
                                document
                                    .getElementById(
                                        `fulfillment-packing-select-address-${toAutomationIdSegment(destination.key)}`,
                                    )
                                    ?.focus({ preventScroll: true })
                            }}
                            tabIndex={
                                selected ||
                                (!selectedVisible &&
                                    destination === destinations[0])
                                    ? 0
                                    : -1
                            }
                            aria-current={selected ? "true" : undefined}
                            aria-label={`${destination.recipient || "待补收货地址"}，${destination.address || "地址待补充"}，${status.label}`}
                            title={`${destination.recipient} ${destination.phone} ${destination.address} · ${status.label}`}
                            aria-controls="fulfillment-packing-address-detail"
                            onClick={() => onSelect(destination.key)}
                            className={cn(
                                "flex h-9 w-full items-center gap-2 rounded-md px-2 text-left transition-colors outline-none hover:bg-muted/60 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset",
                                selected &&
                                    "bg-muted font-medium hover:bg-muted",
                            )}
                        >
                            <span className="min-w-0 flex-1 truncate text-sm">
                                <span>
                                    {destination.recipient || "待补收货地址"}
                                </span>
                                {destination.address && (
                                    <span className="font-normal text-muted-foreground">
                                        {" · "}
                                        {destination.address}
                                    </span>
                                )}
                            </span>
                            <span className="shrink-0" aria-hidden="true">
                                {status.ready ? (
                                    <CheckIcon className="size-4 text-success" />
                                ) : destination.pending_address ? (
                                    <CircleAlertIcon className="size-4 text-warning-soft-foreground" />
                                ) : status.skipped ? (
                                    <MinusIcon className="size-4 text-muted-foreground" />
                                ) : (
                                    <CircleIcon className="size-3.5 text-muted-foreground" />
                                )}
                            </span>
                        </button>
                    )
                })}
                {!destinations.length && (
                    <div className="space-y-2 px-3 py-8 text-center text-xs text-muted-foreground">
                        <MapPinIcon
                            aria-hidden="true"
                            className="mx-auto size-5"
                        />
                        <p>{search ? "没有匹配的地址" : "没有未完成的地址"}</p>
                    </div>
                )}
            </div>
        </aside>
    )
}
