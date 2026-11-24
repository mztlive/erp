"use client"

import { useEffect, useMemo, useState } from "react"
import {
    ArrowUpIcon,
    ChevronDownIcon,
    ChevronUpIcon,
    CopyIcon,
    MapPinIcon,
    SearchIcon,
    WarehouseIcon,
    XIcon,
} from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toast } from "@/components/ui/toast"
import { RegionName } from "@/features/regions"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { fullDeliveryAddress } from "@/lib/delivery-address"
import {
    buildDeliveryAllocationSummary,
    filterDeliveryAddressGroups,
    filterDeliveryItemGroups,
    type DeliveryAddressGroup,
    type DeliveryAllocationLineInput,
    type DeliveryAllocationSummary,
    type DeliveryItemGroup,
} from "@/lib/delivery-allocation-summary"
import { compactFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import { DetailWorkspaceSection } from "./detail-workspace"
import { detailWorkspaceStyles } from "./detail-workspace-styles"
import { QuantityValue } from "./values"

export type DeliveryFulfillmentDescriptor = Readonly<{
    responsibility:
        | "WAREHOUSE"
        | "SUPPLIER_DIRECT"
        | "ELECTRONIC"
        | "SERVICE"
        | "NONE"
    targetWarehouseName?: string | null
}>

const INITIAL_VISIBLE_COUNT = 5

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

function copyAddressText(group: DeliveryAddressGroup) {
    const text = [group.recipient, group.phone, group.fullAddress]
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
                description: `${group.recipient} · ${group.phone}`,
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

/**
 * 销售单审批与采购单审批共用的「交付与收货地址」独立分区：
 * - 自动区分入仓/无需履约/未指定、整单统一收货、2–5 处少量多址、6+ 处大批量多址；
 * - 默认按收货地址聚合（支持按商品切换与内联检索），零弹窗/抽屉嵌套，兼容右栏合同 PDF 对照。
 */
export function DeliveryAllocationOverview({
    idPrefix,
    sectionId,
    lines,
    summary: externalSummary,
    fulfillment,
    focusedLineId,
    onClearFocus,
    onReturnToLine,
    density = "default",
    compactSimpleStates = false,
    emptyMessage = "本次提交暂未指定收货地址，将在履约环节补充。",
    title = "交付与收货地址",
}: {
    idPrefix: string
    sectionId?: string
    lines: readonly DeliveryAllocationLineInput[]
    summary?: DeliveryAllocationSummary
    fulfillment?: DeliveryFulfillmentDescriptor | null
    focusedLineId?: string | null
    onClearFocus?: () => void
    /** 多址定位后点击返回明细行锚点。 */
    onReturnToLine?: (lineId: string) => void
    /** 地址分组列表的展示密度。 */
    density?: "default" | "compact"
    /** 将入仓、未指定地址与整单统一 1 处地址收敛为单行轻量提示条，避免审批页过度拉长。 */
    compactSimpleStates?: boolean
    emptyMessage?: string
    title?: string
}) {
    const summary = useMemo(
        () => externalSummary ?? buildDeliveryAllocationSummary(lines),
        [externalSummary, lines],
    )
    const [viewBy, setViewBy] = useState<"address" | "item">("address")
    const [search, setSearch] = useState("")
    const [expandedAll, setExpandedAll] = useState(false)

    useEffect(() => {
        if (focusedLineId) {
            setExpandedAll(true)
        }
    }, [focusedLineId])

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

    // 费用采购单无需履约且无地址分配时不展示空分区
    if (fulfillment?.responsibility === "NONE" && summary.totalRows === 0) {
        return null
    }

    // 形态 0A：入仓采购单（明确显示目标仓）
    if (
        fulfillment?.responsibility === "WAREHOUSE" &&
        summary.totalRows === 0
    ) {
        if (compactSimpleStates) {
            return (
                <section
                    id={sectionId}
                    aria-label={title}
                    className="flex flex-wrap items-center gap-x-2.5 gap-y-1 border-t border-border/60 py-3.5 text-xs text-muted-foreground"
                >
                    <WarehouseIcon
                        aria-hidden="true"
                        className="size-3.5 shrink-0 text-muted-foreground"
                    />
                    <span className="font-medium text-foreground">
                        入库目标仓：
                        {fulfillment.targetWarehouseName ||
                            "以采购入库安排为准"}
                    </span>
                    <span
                        aria-hidden="true"
                        className="text-muted-foreground/45"
                    >
                        ·
                    </span>
                    <span>
                        供应商发货至公司仓库入库，客户配送由仓库另行安排
                    </span>
                </section>
            )
        }
        return (
            <DetailWorkspaceSection
                id={sectionId}
                title={title}
                ariaLabel={title}
                subtitle="采购入库"
            >
                <div className="flex items-start gap-2.5 rounded-md border border-border/60 bg-surface-sunken px-3.5 py-3 text-body-compact">
                    <WarehouseIcon
                        aria-hidden="true"
                        className="mt-0.5 size-4 shrink-0 text-muted-foreground"
                    />
                    <div className="min-w-0 space-y-0.5">
                        <p className="font-medium text-foreground">
                            入库目标仓：
                            {fulfillment.targetWarehouseName ||
                                "以采购入库安排为准"}
                        </p>
                        <p className="text-xs text-muted-foreground">
                            供应商发货至公司仓库入库，客户配送由仓库另行安排。
                        </p>
                    </div>
                </div>
            </DetailWorkspaceSection>
        )
    }

    // 形态 0B：无任何收货地址分配
    if (summary.totalRows === 0) {
        const modeHint =
            fulfillment?.responsibility === "ELECTRONIC"
                ? "电子交付单据，无需填写实物收货地址。"
                : fulfillment?.responsibility === "SERVICE"
                  ? "线下服务单据，未指定具体收货地址，按约定服务地点履约。"
                  : emptyMessage
        if (compactSimpleStates) {
            return (
                <section
                    id={sectionId}
                    aria-label={title}
                    className="flex items-center gap-2 border-t border-border/60 py-3.5 text-xs text-muted-foreground"
                >
                    <MapPinIcon
                        aria-hidden="true"
                        className="size-3.5 shrink-0"
                    />
                    <span>交付地址：{modeHint}</span>
                </section>
            )
        }
        return (
            <DetailWorkspaceSection
                id={sectionId}
                title={title}
                ariaLabel={title}
                subtitle="未指定地址"
            >
                <p className="flex items-center gap-2 text-xs text-muted-foreground">
                    <MapPinIcon
                        aria-hidden="true"
                        className="size-3.5 shrink-0"
                    />
                    <span>{modeHint}</span>
                </p>
            </DetailWorkspaceSection>
        )
    }

    // 形态 1：整单统一收货（1 处独立地址覆盖所有需配送商品）
    if (summary.isUniformSingleAddress && !focusedLine) {
        const only = summary.addressGroups[0]!
        if (compactSimpleStates) {
            return (
                <section
                    id={sectionId}
                    aria-label={title}
                    className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1.5 border-t border-border/60 py-3.5 text-xs"
                >
                    <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-2 gap-y-1">
                        <MapPinIcon
                            aria-hidden="true"
                            className="size-3.5 shrink-0 text-primary"
                        />
                        <Badge variant="neutral">整单统一收货</Badge>
                        <span className="font-medium text-foreground">
                            {only.recipient}
                        </span>
                        <span className="num text-muted-foreground">
                            {only.phone}
                        </span>
                        <span
                            aria-hidden="true"
                            className="text-muted-foreground/45"
                        >
                            ·
                        </span>
                        <span className="min-w-0 wrap-anywhere text-foreground/85">
                            <AddressText
                                regionCode={only.regionCode}
                                regionName={only.regionName}
                                address={only.address}
                            />
                        </span>
                    </div>
                    <Button
                        id={`${idPrefix}-copy-single`}
                        type="button"
                        variant="ghost"
                        size="xs"
                        className={detailWorkspaceStyles.quietButton}
                        onClick={() => copyAddressText(only)}
                    >
                        <CopyIcon data-icon="inline-start" aria-hidden="true" />
                        复制地址
                    </Button>
                </section>
            )
        }
        return (
            <DetailWorkspaceSection
                id={sectionId}
                title={title}
                ariaLabel={title}
                subtitle="整单统一收货"
            >
                <div className="flex flex-wrap items-start justify-between gap-3 rounded-md border border-border/60 bg-surface-sunken/70 px-3.5 py-3">
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
                                    覆盖全部 {summary.assignedLineCount} 项商品
                                </Badge>
                            </div>
                            <p className="text-body-compact wrap-anywhere text-foreground/85">
                                <AddressText
                                    regionCode={only.regionCode}
                                    regionName={only.regionName}
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
                        className={detailWorkspaceStyles.quietButton}
                        onClick={() => copyAddressText(only)}
                    >
                        <CopyIcon data-icon="inline-start" aria-hidden="true" />
                        复制地址
                    </Button>
                </div>
            </DetailWorkspaceSection>
        )
    }

    // 形态 2（2–5 处）& 形态 3（>5 处）
    const isLargeSet = summary.uniqueAddressCount > INITIAL_VISIBLE_COUNT
    const showSearch = isLargeSet || Boolean(search)
    const compact = density === "compact"
    const activeCount =
        viewBy === "address" ? filteredAddresses.length : filteredItems.length
    const isCollapsed =
        isLargeSet &&
        !expandedAll &&
        !search.trim() &&
        !focusedLine &&
        activeCount > INITIAL_VISIBLE_COUNT
    const visibleAddresses = isCollapsed
        ? filteredAddresses.slice(0, INITIAL_VISIBLE_COUNT)
        : filteredAddresses
    const visibleItems = isCollapsed
        ? filteredItems.slice(0, INITIAL_VISIBLE_COUNT)
        : filteredItems

    const subtitleText =
        summary.uniqueAddressCount === summary.totalRows
            ? `共 ${summary.uniqueAddressCount} 处独立地址`
            : `共 ${summary.uniqueAddressCount} 处独立地址 · ${summary.totalRows} 条分配`

    return (
        <DetailWorkspaceSection
            id={sectionId}
            title={title}
            ariaLabel={title}
            subtitle={subtitleText}
            className={compact ? "py-4 last:pb-5" : undefined}
            contentClassName={compact ? "space-y-2" : undefined}
            actions={
                <div
                    role="group"
                    aria-label="收货地址查看维度"
                    className={detailWorkspaceStyles.subTabs}
                >
                    <button
                        id={`${idPrefix}-view-address`}
                        type="button"
                        aria-pressed={viewBy === "address"}
                        data-active={viewBy === "address" ? "true" : undefined}
                        className={detailWorkspaceStyles.subTab}
                        onClick={() => setViewBy("address")}
                    >
                        按收货地址
                    </button>
                    <button
                        id={`${idPrefix}-view-item`}
                        type="button"
                        aria-pressed={viewBy === "item"}
                        data-active={viewBy === "item" ? "true" : undefined}
                        className={detailWorkspaceStyles.subTab}
                        onClick={() => setViewBy("item")}
                    >
                        按商品
                    </button>
                </div>
            }
        >
            <div className={compact ? "space-y-2" : "space-y-3"}>
                {summary.unassignedLineCount > 0 && !focusedLine ? (
                    <p className="text-xs text-warning-soft-foreground">
                        另有 {summary.unassignedLineCount}{" "}
                        项商品尚未指定收货地址，将在履约前补齐。
                    </p>
                ) : null}

                {focusedLine ? (
                    <div className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-info-border bg-info-soft/50 px-3 py-1.5 text-xs text-info-soft-foreground">
                        <span>
                            已定位到第 {focusedLine.lineNo} 行「
                            <span className="font-medium">
                                {focusedLine.itemName}
                            </span>
                            」的 {focusedLine.uniqueAddressCount} 处收货地址
                        </span>
                        <div className="flex flex-wrap items-center gap-1">
                            {onReturnToLine ? (
                                <Button
                                    id={`${idPrefix}-return-line`}
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    className="h-control-xs px-2 text-xs text-info-soft-foreground hover:bg-info-soft"
                                    onClick={() =>
                                        onReturnToLine(focusedLine.lineId)
                                    }
                                >
                                    <ArrowUpIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    返回第 {focusedLine.lineNo} 行
                                </Button>
                            ) : null}
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
                    </div>
                ) : null}

                {showSearch ? (
                    <div
                        className={
                            compact
                                ? "flex flex-wrap items-center gap-x-4 gap-y-2"
                                : "space-y-3"
                        }
                    >
                        {isLargeSet &&
                        !focusedLine &&
                        summary.regionDistribution.length > 0 ? (
                            <div className="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
                                <span>地区分布：</span>
                                {summary.regionDistribution
                                    .slice(0, 5)
                                    .map((stat) => (
                                        <span
                                            key={stat.label}
                                            className="num rounded-sm bg-muted/65 px-2 py-0.5 text-foreground/85"
                                        >
                                            {stat.label} {stat.count} 处
                                        </span>
                                    ))}
                                {summary.regionDistribution.length > 5 ? (
                                    <span className="num">
                                        等 {summary.regionDistribution.length}{" "}
                                        个地区
                                    </span>
                                ) : null}
                            </div>
                        ) : null}

                        <div
                            className={cn(
                                "flex flex-wrap items-center justify-between gap-2",
                                compact && "min-w-0 flex-1 basis-80",
                            )}
                        >
                            <div
                                className={cn(
                                    "relative flex-1 sm:max-w-sm",
                                    compact ? "min-w-0 basis-52" : "min-w-52",
                                )}
                            >
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
                                        aria-label="清除收货地址搜索"
                                        className="absolute top-1/2 right-0.5 -translate-y-1/2"
                                        onClick={() => setSearch("")}
                                    >
                                        <XIcon aria-hidden="true" />
                                    </Button>
                                ) : null}
                            </div>
                            <span
                                role="status"
                                aria-live="polite"
                                className="num text-xs text-muted-foreground"
                            >
                                {viewBy === "address"
                                    ? `匹配 ${filteredAddresses.length} / ${summary.uniqueAddressCount} 处地址`
                                    : `匹配 ${filteredItems.length} / ${summary.itemGroups.length} 项商品`}
                            </span>
                        </div>
                    </div>
                ) : null}

                <div
                    className={cn(
                        compact
                            ? detailWorkspaceStyles.inlineTableWrapper
                            : "rounded-md border border-border/65 bg-card",
                        expandedAll &&
                            isLargeSet &&
                            "max-h-96 overflow-y-auto overscroll-contain",
                    )}
                >
                    {viewBy === "address" ? (
                        visibleAddresses.length > 0 ? (
                            <ol className="divide-y divide-border/60">
                                {visibleAddresses.map((group, index) => (
                                    <AddressGroupRow
                                        key={group.key}
                                        index={index}
                                        group={group}
                                        compact={compact}
                                        idPrefix={`${idPrefix}-addr-${toAutomationIdSegment(String(index + 1))}`}
                                    />
                                ))}
                            </ol>
                        ) : (
                            <p className="py-8 text-center text-xs text-muted-foreground">
                                未找到匹配的收货地址，请调整搜索条件。
                            </p>
                        )
                    ) : visibleItems.length > 0 ? (
                        <div className="divide-y divide-border/60">
                            {visibleItems.map((group) => (
                                <ItemAddressGroupRow
                                    key={group.lineId}
                                    group={group}
                                    compact={compact}
                                />
                            ))}
                        </div>
                    ) : (
                        <p className="py-8 text-center text-xs text-muted-foreground">
                            未找到匹配的商品配送记录，请调整搜索条件。
                        </p>
                    )}
                </div>

                {isLargeSet &&
                !search.trim() &&
                !focusedLine &&
                activeCount > INITIAL_VISIBLE_COUNT ? (
                    <div className="flex items-center justify-center">
                        <Button
                            id={`${idPrefix}-toggle-expand`}
                            type="button"
                            variant="ghost"
                            size="sm"
                            aria-expanded={expandedAll}
                            className={detailWorkspaceStyles.subtleButton}
                            onClick={() => setExpandedAll((open) => !open)}
                        >
                            {expandedAll ? (
                                <>
                                    收起至前 {INITIAL_VISIBLE_COUNT}{" "}
                                    {viewBy === "address" ? "处" : "项"}
                                    <ChevronUpIcon
                                        data-icon="inline-end"
                                        aria-hidden="true"
                                    />
                                </>
                            ) : (
                                <>
                                    展开全部 {activeCount}{" "}
                                    {viewBy === "address"
                                        ? "处收货地址"
                                        : "项商品分配"}
                                    <ChevronDownIcon
                                        data-icon="inline-end"
                                        aria-hidden="true"
                                    />
                                </>
                            )}
                        </Button>
                    </div>
                ) : null}
            </div>
        </DetailWorkspaceSection>
    )
}

function AddressGroupRow({
    index,
    group,
    idPrefix,
    compact,
}: {
    index: number
    group: DeliveryAddressGroup
    idPrefix: string
    compact: boolean
}) {
    return (
        <li className={compact ? "space-y-1 py-2" : "space-y-2 px-3.5 py-3"}>
            <div className="flex flex-wrap items-start justify-between gap-2">
                <div
                    className={cn(
                        "min-w-0 flex-1",
                        compact
                            ? "flex flex-wrap items-baseline gap-x-3 gap-y-0.5"
                            : "space-y-0.5",
                    )}
                >
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
                    <p
                        className={cn(
                            "text-xs wrap-anywhere text-foreground/80",
                            compact ? "min-w-0" : "leading-5",
                        )}
                    >
                        <AddressText
                            regionCode={group.regionCode}
                            regionName={group.regionName}
                            address={group.address}
                        />
                    </p>
                </div>
                <Button
                    id={`${idPrefix}-copy`}
                    type="button"
                    variant="ghost"
                    size="xs"
                    aria-label={`复制 ${group.recipient} 的收货地址`}
                    className="h-control-xs px-1.5 text-xs text-muted-foreground hover:text-foreground"
                    onClick={() => copyAddressText(group)}
                >
                    <CopyIcon aria-hidden="true" />
                    <span className="sr-only">复制地址</span>
                </Button>
            </div>
            <div className="flex flex-wrap items-center gap-1.5">
                {group.items.map((item) => (
                    <span
                        key={item.lineId}
                        className="inline-flex max-w-full items-baseline gap-1.5 rounded-sm bg-muted/55 px-2 py-0.5 text-xs text-foreground/90"
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
    compact,
}: {
    group: DeliveryItemGroup
    compact: boolean
}) {
    return (
        <div className={compact ? "space-y-1 py-2" : "space-y-2 px-3.5 py-3"}>
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
                <ul className="divide-y divide-border/45 rounded-sm bg-muted/30 px-3">
                    {group.allocations.map((row, idx) => (
                        <li
                            key={`${group.lineId}-${idx}`}
                            className={cn(
                                "flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 text-xs",
                                compact ? "py-1" : "py-2",
                            )}
                        >
                            <div
                                className={cn(
                                    "min-w-0 flex-1 wrap-anywhere",
                                    compact
                                        ? "flex flex-wrap items-baseline gap-x-3 gap-y-0.5"
                                        : "space-y-0.5",
                                )}
                            >
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
                            <span className="num shrink-0 font-medium text-foreground">
                                <QuantityValue
                                    value={compactFixed(row.quantity)}
                                    unit={group.unit}
                                />
                            </span>
                        </li>
                    ))}
                </ul>
            ) : null}
        </div>
    )
}

/** 供纸质单据预览（PaperDocument appendix）使用的静态只读收货地址附表。 */
export function DeliveryAllocationPrintAppendix({
    lines,
}: {
    lines: readonly DeliveryAllocationLineInput[]
}) {
    const summary = useMemo(
        () => buildDeliveryAllocationSummary(lines),
        [lines],
    )
    if (summary.totalRows === 0) return null

    if (summary.isUniformSingleAddress) {
        const only = summary.addressGroups[0]!
        return (
            <section
                aria-label="收货地址"
                className="space-y-1.5 border-t border-border/60 pt-5 text-body-compact"
            >
                <h3 className="font-semibold text-foreground">
                    收货地址（整单统一收货）
                </h3>
                <p className="text-foreground">
                    {only.recipient}{" "}
                    <span className="num ml-2 text-muted-foreground">
                        {only.phone}
                    </span>
                </p>
                <p className="text-xs wrap-anywhere text-muted-foreground">
                    <AddressText
                        regionCode={only.regionCode}
                        regionName={only.regionName}
                        address={only.address}
                    />
                </p>
            </section>
        )
    }

    return (
        <section
            aria-label="收货地址明细"
            className="space-y-2.5 border-t border-border/60 pt-5"
        >
            <div className="flex flex-wrap items-baseline justify-between gap-2">
                <h3 className="text-body-compact font-semibold text-foreground">
                    收货地址明细
                </h3>
                <span className="num text-xs text-muted-foreground">
                    共 {summary.uniqueAddressCount} 处独立地址 ·{" "}
                    {summary.totalRows} 条分配
                </span>
            </div>
            <div className={detailWorkspaceStyles.inlineTableWrapper}>
                <Table
                    data-density="comfortable"
                    aria-label="纸质单据收货地址明细"
                    className="w-full [&_:is(th,td):first-child]:ps-0"
                >
                    <TableHeader>
                        <TableRow>
                            <TableHead scope="col" className="w-9">
                                序号
                            </TableHead>
                            <TableHead scope="col">收货人 / 电话</TableHead>
                            <TableHead scope="col">收货地址</TableHead>
                            <TableHead scope="col">配送商品及数量</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {summary.addressGroups.map((group, index) => (
                            <TableRow key={group.key}>
                                <TableCell className="num py-2.5 align-top text-muted-foreground">
                                    {index + 1}
                                </TableCell>
                                <TableCell className="py-2.5 align-top whitespace-normal wrap-anywhere">
                                    <p className="font-medium">
                                        {group.recipient}
                                    </p>
                                    <p className="num text-xs text-muted-foreground">
                                        {group.phone}
                                    </p>
                                </TableCell>
                                <TableCell className="py-2.5 align-top whitespace-normal wrap-anywhere text-xs">
                                    <AddressText
                                        regionCode={group.regionCode}
                                        regionName={group.regionName}
                                        address={group.address}
                                    />
                                </TableCell>
                                <TableCell className="py-2.5 align-top whitespace-normal">
                                    <ul className="space-y-0.5 text-xs">
                                        {group.items.map((item) => (
                                            <li
                                                key={item.lineId}
                                                className="flex items-baseline justify-between gap-2"
                                            >
                                                <span className="wrap-anywhere">
                                                    {item.itemName}
                                                </span>
                                                <span className="num shrink-0 font-medium">
                                                    <QuantityValue
                                                        value={item.quantity}
                                                        unit={item.unit}
                                                    />
                                                </span>
                                            </li>
                                        ))}
                                    </ul>
                                </TableCell>
                            </TableRow>
                        ))}
                    </TableBody>
                </Table>
            </div>
        </section>
    )
}
