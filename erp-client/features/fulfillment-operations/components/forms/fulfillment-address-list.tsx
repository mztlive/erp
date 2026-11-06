"use client"

import { useEffect, useRef } from "react"
import { CheckIcon, MapPinIcon, SearchIcon } from "lucide-react"
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
        <aside
            aria-label="收货地址导航"
            className="min-w-0 rounded-lg border border-border/70 bg-muted/15"
        >
            <div className="space-y-3 border-b border-border/60 p-3">
                <div className="flex items-center justify-between gap-2 text-sm">
                    <span className="font-medium">
                        收货地址{" "}
                        <span className="text-muted-foreground">{total}</span>
                    </span>
                    <span className="text-xs text-muted-foreground">
                        已填写 {completed} / {total}
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
                        placeholder="收货人、电话、地址或商品"
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
                    只看未完成
                </Label>
            </div>
            <div
                ref={listRef}
                className="relative max-h-52 overflow-y-auto overscroll-contain p-1.5 lg:max-h-[30rem]"
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
                            aria-controls="fulfillment-packing-address-detail"
                            onClick={() => onSelect(destination.key)}
                            className={cn(
                                "mb-1 block w-full rounded-md border border-transparent px-3 py-2.5 text-left transition-colors outline-none last:mb-0 hover:bg-muted/60 focus-visible:ring-2 focus-visible:ring-ring",
                                selected &&
                                    "border-primary/20 bg-primary/5 hover:bg-primary/5",
                            )}
                        >
                            <div className="flex items-center justify-between gap-2">
                                <span className="truncate text-sm font-medium">
                                    {destination.recipient || "待补收货地址"}
                                </span>
                                <span
                                    className={cn(
                                        "flex shrink-0 items-center gap-1 text-xs",
                                        status.ready
                                            ? "text-primary"
                                            : "text-muted-foreground",
                                    )}
                                >
                                    {status.ready && (
                                        <CheckIcon
                                            aria-hidden="true"
                                            className="size-3"
                                        />
                                    )}
                                    {status.label}
                                </span>
                            </div>
                            {destination.phone && (
                                <p className="mt-1 text-xs text-muted-foreground">
                                    {destination.phone}
                                </p>
                            )}
                            <p className="mt-1 line-clamp-2 break-words text-xs leading-5 text-muted-foreground">
                                {destination.address || "补齐地址后可登记发货"}
                            </p>
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
