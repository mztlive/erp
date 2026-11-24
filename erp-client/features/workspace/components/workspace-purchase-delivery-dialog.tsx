"use client"

import { useMemo, useState } from "react"
import { CopyIcon, MapPinIcon, SearchIcon, XIcon } from "lucide-react"

import { QuantityValue, detailWorkspaceStyles } from "@/components/business"
import { Badge } from "@/components/ui/badge"
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
import type {
    ApprovalPurchaseFulfillment,
    ApprovalPurchaseLine,
} from "@/features/approval-workflow/api/materials"
import { formatPurchaseFulfillmentSummary } from "@/features/approval-workflow/purchase-fulfillment"
import { RegionName } from "@/features/regions"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { fullDeliveryAddress } from "@/lib/delivery-address"
import {
    buildDeliveryAllocationSummary,
    deliveryAddressIdentityKey,
    filterDeliveryAddressGroups,
    filterDeliveryItemGroups,
    type DeliveryAddressGroup,
    type DeliveryAllocationLineInput,
    type DeliveryItemGroup,
} from "@/lib/delivery-allocation-summary"
import { displayName } from "@/lib/display-name"
import { compactFixed } from "@/lib/fixed-decimal"
import { purchaseSpecification } from "./workspace-purchase-comparison-model"

export type WorkspacePurchaseDeliveryDialogProps = Readonly<{
    idPrefix: string
    open: boolean
    onOpenChange: (open: boolean) => void
    lines: readonly ApprovalPurchaseLine[]
    fulfillment?: ApprovalPurchaseFulfillment | null
    focusedLineId?: string | null
    onClearFocus?: () => void
}>

/** 为地址标识生成稳定的短 hex 片段，避免自动化 ID 依赖数组下标。 */
function stableAddressKeyHash(value: string): string {
    let hash = 2166136261
    for (let index = 0; index < value.length; index += 1) {
        hash ^= value.charCodeAt(index)
        hash = Math.imul(hash, 16777619)
    }
    return (hash >>> 0).toString(16).padStart(8, "0")
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

function copyRecipientAddress(payload: {
    recipient: string
    phone: string
    regionName?: string | null
    address: string
}) {
    const fullAddress = fullDeliveryAddress({
        region_name: payload.regionName?.trim(),
        address: payload.address,
    })
    const text = [payload.recipient, payload.phone, fullAddress]
        .filter(Boolean)
        .join(" ")
    if (typeof navigator === "undefined" || !navigator.clipboard) {
        toast.add({
            title: "当前环境不支持一键复制",
            type: "warning",
        })
        return
    }
    void navigator.clipboard.writeText(text).then(
        () => {
            toast.add({
                title: "收货地址已复制",
                description: `${payload.recipient} · ${payload.phone}`,
                type: "success",
            })
        },
        () => {
            toast.add({
                title: "复制失败，请手动选中复制",
                type: "error",
            })
        },
    )
}

function copyAddressGroup(group: DeliveryAddressGroup) {
    copyRecipientAddress({
        recipient: group.recipient,
        phone: group.phone,
        regionName: group.regionName,
        address: group.address,
    })
}

/** 采购单审批按需查看配送地址弹窗（默认不展开占屏，点击后支持检索与双视角核对）。 */
export function WorkspacePurchaseDeliveryDialog({
    idPrefix,
    open,
    onOpenChange,
    lines,
    fulfillment,
    focusedLineId,
    onClearFocus,
}: WorkspacePurchaseDeliveryDialogProps) {
    const [search, setSearch] = useState("")
    const [viewBy, setViewBy] = useState<"address" | "item">("address")

    const handleOpenChange = (nextOpen: boolean) => {
        if (!nextOpen) {
            setSearch("")
            setViewBy("address")
        }
        onOpenChange(nextOpen)
    }

    const deliveryLines = useMemo<readonly DeliveryAllocationLineInput[]>(
        () =>
            lines.map((line) => ({
                id: line.id,
                lineNo: line.line_no,
                itemName: displayName(line.title) || "未记录商品名称",
                specification: purchaseSpecification(line.specification),
                unit: line.unitName ?? line.unit,
                quantity: line.quantity,
                allocations: line.delivery_allocations,
            })),
        [lines],
    )

    const summary = useMemo(
        () => buildDeliveryAllocationSummary(deliveryLines),
        [deliveryLines],
    )

    const focusedLine = useMemo(
        () =>
            focusedLineId
                ? summary.itemGroups.find(
                      (item) => item.lineId === focusedLineId,
                  )
                : undefined,
        [focusedLineId, summary.itemGroups],
    )

    const filteredAddresses = useMemo(
        () =>
            filterDeliveryAddressGroups(
                summary.addressGroups,
                search,
                focusedLine?.lineId,
            ),
        [focusedLine?.lineId, search, summary.addressGroups],
    )
    const filteredItems = useMemo(
        () =>
            filterDeliveryItemGroups(
                summary.itemGroups,
                search,
                focusedLine?.lineId,
            ),
        [focusedLine?.lineId, search, summary.itemGroups],
    )

    const fulfillmentModeLabel = formatPurchaseFulfillmentSummary(
        fulfillment,
        0,
    )
    const addressStats =
        summary.totalRows > 0
            ? summary.uniqueAddressCount === summary.totalRows
                ? `共 ${summary.uniqueAddressCount} 处独立地址`
                : `共 ${summary.uniqueAddressCount} 处独立地址 · ${summary.totalRows} 条分配`
            : "暂未指定配送地址"
    const headerDescription = [fulfillmentModeLabel, addressStats]
        .filter(Boolean)
        .join(" · ")

    const showSearch = summary.uniqueAddressCount > 3 || Boolean(search)
    const isUniformSingle =
        summary.isUniformSingleAddress && !focusedLine && summary.totalRows > 0

    return (
        <Dialog open={open} onOpenChange={handleOpenChange}>
            <DialogContent
                size="lg"
                scroll="body"
                closeButtonId={`${idPrefix}-close`}
            >
                <DialogHeader>
                    <DialogTitle>配送地址明细</DialogTitle>
                    <DialogDescription className="num">
                        {headerDescription}
                    </DialogDescription>
                </DialogHeader>

                <DialogBody className="space-y-3">
                    {focusedLine ? (
                        <div className="flex flex-wrap items-center justify-between gap-2 rounded-md bg-info-soft/55 px-3 py-2 text-xs text-info-soft-foreground">
                            <span>
                                当前仅查看第{" "}
                                <span className="num font-medium">
                                    {focusedLine.lineNo}
                                </span>{" "}
                                行「
                                <span className="font-medium">
                                    {focusedLine.itemName}
                                </span>
                                」的{" "}
                                <span className="num font-medium">
                                    {focusedLine.uniqueAddressCount}
                                </span>{" "}
                                处配送地址
                            </span>
                            {onClearFocus ? (
                                <Button
                                    id={`${idPrefix}-clear-focus`}
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    className="h-control-xs px-2 text-xs text-info-soft-foreground hover:bg-info-soft"
                                    onClick={onClearFocus}
                                >
                                    <XIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    查看整单全部地址
                                </Button>
                            ) : null}
                        </div>
                    ) : null}

                    {summary.totalRows === 0 ? (
                        <p className="py-10 text-center text-xs text-muted-foreground">
                            本次提交暂未指定配送地址。
                        </p>
                    ) : isUniformSingle ? (
                        (() => {
                            const only = summary.addressGroups[0]!
                            return (
                                <div className="space-y-2 rounded-lg bg-surface-sunken p-4">
                                    <div className="flex flex-wrap items-start justify-between gap-3">
                                        <div className="flex min-w-0 flex-1 items-start gap-2.5">
                                            <MapPinIcon
                                                aria-hidden="true"
                                                className="mt-0.5 size-4 shrink-0 text-primary"
                                            />
                                            <div className="min-w-0 flex-1 space-y-1">
                                                <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1 text-body-compact">
                                                    <span className="font-medium text-foreground">
                                                        {only.recipient}
                                                    </span>
                                                    <span className="num text-muted-foreground">
                                                        {only.phone}
                                                    </span>
                                                    <Badge variant="neutral">
                                                        整单统一收货 · 覆盖全部{" "}
                                                        {
                                                            summary.assignedLineCount
                                                        }{" "}
                                                        项商品
                                                    </Badge>
                                                </div>
                                                <p className="text-body-compact wrap-anywhere text-foreground/85">
                                                    <AddressText
                                                        regionCode={
                                                            only.regionCode
                                                        }
                                                        regionName={
                                                            only.regionName
                                                        }
                                                        address={only.address}
                                                    />
                                                </p>
                                            </div>
                                        </div>
                                        <Button
                                            id={`${idPrefix}-copy-single`}
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            className={
                                                detailWorkspaceStyles.quietButton
                                            }
                                            onClick={() =>
                                                copyAddressGroup(only)
                                            }
                                        >
                                            <CopyIcon
                                                data-icon="inline-start"
                                                aria-hidden="true"
                                            />
                                            复制地址
                                        </Button>
                                    </div>
                                </div>
                            )
                        })()
                    ) : (
                        <>
                            {!focusedLine &&
                            summary.regionDistribution.length > 1 ? (
                                <div className="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
                                    <span>地区分布：</span>
                                    {summary.regionDistribution
                                        .slice(0, 5)
                                        .map((stat) => (
                                            <span
                                                key={stat.label}
                                                className="num rounded-sm bg-surface-sunken px-2 py-0.5 text-foreground/85"
                                            >
                                                {stat.label} {stat.count} 处
                                            </span>
                                        ))}
                                    {summary.regionDistribution.length > 5 ? (
                                        <span className="num">
                                            等{" "}
                                            {summary.regionDistribution.length}{" "}
                                            个地区
                                        </span>
                                    ) : null}
                                </div>
                            ) : null}

                            <div className="flex flex-wrap items-center justify-between gap-2.5">
                                {showSearch ? (
                                    <div className="relative min-w-52 flex-1 sm:max-w-xs">
                                        <SearchIcon
                                            aria-hidden="true"
                                            className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                                        />
                                        <Input
                                            id={`${idPrefix}-search`}
                                            value={search}
                                            onChange={(event) =>
                                                setSearch(event.target.value)
                                            }
                                            placeholder="搜索收货人、电话、地址或商品"
                                            aria-label="搜索收货人、电话、地址或商品"
                                            className="h-control-sm pr-8 pl-8 text-xs"
                                        />
                                        {search ? (
                                            <Button
                                                id={`${idPrefix}-search-clear`}
                                                type="button"
                                                variant="ghost"
                                                size="icon-xs"
                                                aria-label="清除配送地址搜索"
                                                className="absolute top-1/2 right-0.5 -translate-y-1/2"
                                                onClick={() => setSearch("")}
                                            >
                                                <XIcon aria-hidden="true" />
                                            </Button>
                                        ) : null}
                                    </div>
                                ) : (
                                    <div />
                                )}

                                <div className="flex flex-wrap items-center gap-2.5">
                                    {search.trim() ? (
                                        <span
                                            role="status"
                                            aria-live="polite"
                                            className="num text-xs text-muted-foreground"
                                        >
                                            {viewBy === "address"
                                                ? `匹配 ${filteredAddresses.length} / ${summary.uniqueAddressCount} 处地址`
                                                : `匹配 ${filteredItems.length} / ${summary.itemGroups.length} 项商品`}
                                        </span>
                                    ) : null}
                                    <div
                                        role="group"
                                        aria-label="配送地址查看维度"
                                        className={
                                            detailWorkspaceStyles.subTabs
                                        }
                                    >
                                        <button
                                            id={`${idPrefix}-view-address`}
                                            type="button"
                                            aria-pressed={viewBy === "address"}
                                            data-active={
                                                viewBy === "address"
                                                    ? "true"
                                                    : undefined
                                            }
                                            className={
                                                detailWorkspaceStyles.subTab
                                            }
                                            onClick={() => setViewBy("address")}
                                        >
                                            按收货地址
                                        </button>
                                        <button
                                            id={`${idPrefix}-view-item`}
                                            type="button"
                                            aria-pressed={viewBy === "item"}
                                            data-active={
                                                viewBy === "item"
                                                    ? "true"
                                                    : undefined
                                            }
                                            className={
                                                detailWorkspaceStyles.subTab
                                            }
                                            onClick={() => setViewBy("item")}
                                        >
                                            按商品
                                        </button>
                                    </div>
                                </div>
                            </div>

                            {summary.unassignedLineCount > 0 && !focusedLine ? (
                                <p className="text-xs text-warning-soft-foreground">
                                    另有 {summary.unassignedLineCount}{" "}
                                    项商品未指定配送地址。
                                </p>
                            ) : null}

                            {viewBy === "address" ? (
                                filteredAddresses.length > 0 ? (
                                    <ol className="divide-y divide-border/50">
                                        {filteredAddresses.map(
                                            (group, index) => (
                                                <AddressGroupRow
                                                    key={group.key}
                                                    index={index}
                                                    group={group}
                                                    copyButtonId={`${idPrefix}-copy-${stableAddressKeyHash(group.key)}`}
                                                />
                                            ),
                                        )}
                                    </ol>
                                ) : (
                                    <p className="py-10 text-center text-xs text-muted-foreground">
                                        未找到匹配的配送地址，请调整搜索条件。
                                    </p>
                                )
                            ) : filteredItems.length > 0 ? (
                                <div className="divide-y divide-border/50">
                                    {filteredItems.map((group) => (
                                        <ItemAddressGroupRow
                                            key={group.lineId}
                                            group={group}
                                            idPrefix={idPrefix}
                                        />
                                    ))}
                                </div>
                            ) : (
                                <p className="py-10 text-center text-xs text-muted-foreground">
                                    未找到匹配的商品配送记录，请调整搜索条件。
                                </p>
                            )}
                        </>
                    )}
                </DialogBody>

                <DialogFooter
                    showCloseButton
                    closeButtonId={`${idPrefix}-footer-close`}
                />
            </DialogContent>
        </Dialog>
    )
}

function AddressGroupRow({
    index,
    group,
    copyButtonId,
}: {
    index: number
    group: DeliveryAddressGroup
    copyButtonId: string
}) {
    return (
        <li className="space-y-2 py-3 first:pt-1 last:pb-1">
            <div className="flex flex-wrap items-start justify-between gap-2">
                <div className="min-w-0 flex-1 space-y-0.5">
                    <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5 text-body-compact">
                        <span className="num text-xs text-muted-foreground">
                            #{index + 1}
                        </span>
                        <span className="font-medium text-foreground">
                            {group.recipient}
                        </span>
                        <span className="num text-xs text-muted-foreground">
                            {group.phone}
                        </span>
                    </div>
                    <p className="text-xs leading-5 wrap-anywhere text-foreground/80">
                        <AddressText
                            regionCode={group.regionCode}
                            regionName={group.regionName}
                            address={group.address}
                        />
                    </p>
                </div>
                <Button
                    id={copyButtonId}
                    type="button"
                    variant="ghost"
                    size="xs"
                    aria-label={`复制 ${group.recipient} 的收货地址`}
                    className="h-control-xs px-1.5 text-xs text-muted-foreground hover:text-foreground"
                    onClick={() => copyAddressGroup(group)}
                >
                    <CopyIcon aria-hidden="true" />
                    <span className="sr-only">复制地址</span>
                </Button>
            </div>
            <div className="flex flex-wrap items-center gap-1.5">
                {group.items.map((item) => (
                    <span
                        key={item.lineId}
                        className="inline-flex max-w-full items-baseline gap-1.5 rounded-sm bg-surface-sunken px-2 py-0.5 text-xs text-foreground/90"
                    >
                        <span className="truncate">
                            <span className="num mr-1 text-muted-foreground">
                                #{item.lineNo}
                            </span>
                            {item.itemName}
                        </span>
                        <span className="num shrink-0 font-medium text-foreground">
                            ×{" "}
                            <QuantityValue
                                value={item.quantity}
                                unit={item.unit}
                            />
                        </span>
                    </span>
                ))}
            </div>
        </li>
    )
}

function ItemAddressGroupRow({
    group,
    idPrefix,
}: {
    group: DeliveryItemGroup
    idPrefix: string
}) {
    const lineSegment = toAutomationIdSegment(group.lineId)
    const keyCounts = new Map<string, number>()

    return (
        <div className="space-y-2 py-3 first:pt-1 last:pb-1">
            <div className="flex flex-wrap items-baseline justify-between gap-2 text-body-compact">
                <div className="min-w-0 font-medium text-foreground">
                    <span className="num mr-1.5 text-xs font-normal text-muted-foreground">
                        #{group.lineNo}
                    </span>
                    <span className="wrap-anywhere">{group.itemName}</span>
                    {group.specification ? (
                        <span className="ml-2 text-xs font-normal text-muted-foreground">
                            {group.specification}
                        </span>
                    ) : null}
                </div>
                <span className="num shrink-0 text-xs text-muted-foreground">
                    {group.allocations.length > 0 ? (
                        <>
                            {group.uniqueAddressCount} 处 · 共{" "}
                            <QuantityValue
                                value={group.allocatedTotal}
                                unit={group.unit}
                            />
                        </>
                    ) : (
                        "未指定收货地址"
                    )}
                </span>
            </div>
            {group.allocations.length > 0 ? (
                <ul className="divide-y divide-border/40 rounded-md bg-surface-sunken px-3">
                    {group.allocations.map((row) => {
                        const baseHash = stableAddressKeyHash(
                            deliveryAddressIdentityKey(row),
                        )
                        const seen = (keyCounts.get(baseHash) ?? 0) + 1
                        keyCounts.set(baseHash, seen)
                        const rowKey =
                            seen > 1 ? `${baseHash}-${seen}` : baseHash
                        const copyId = `${idPrefix}-item-${lineSegment}-copy-${rowKey}`

                        return (
                            <li
                                key={rowKey}
                                className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1 py-2 text-xs"
                            >
                                <div className="min-w-0 flex-1 space-y-0.5 wrap-anywhere">
                                    <p className="text-foreground">
                                        <span className="font-medium">
                                            {row.recipient}
                                        </span>
                                        <span className="num ml-2 text-muted-foreground">
                                            {row.phone}
                                        </span>
                                    </p>
                                    <p className="min-w-0 text-muted-foreground">
                                        <AddressText
                                            regionCode={row.region_code}
                                            regionName={row.region_name}
                                            address={row.address}
                                        />
                                    </p>
                                </div>
                                <div className="flex shrink-0 items-center gap-2">
                                    <span className="num font-medium text-foreground">
                                        <QuantityValue
                                            value={compactFixed(row.quantity)}
                                            unit={group.unit}
                                        />
                                    </span>
                                    <Button
                                        id={copyId}
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        aria-label={`复制 ${row.recipient} 的收货地址`}
                                        className="h-control-xs px-1.5 text-xs text-muted-foreground hover:text-foreground"
                                        onClick={() =>
                                            copyRecipientAddress({
                                                recipient: row.recipient,
                                                phone: row.phone,
                                                regionName: row.region_name,
                                                address: row.address,
                                            })
                                        }
                                    >
                                        <CopyIcon aria-hidden="true" />
                                        <span className="sr-only">
                                            复制地址
                                        </span>
                                    </Button>
                                </div>
                            </li>
                        )
                    })}
                </ul>
            ) : null}
        </div>
    )
}
