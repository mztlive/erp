"use client"

import * as React from "react"
import {
    ChevronDownIcon,
    ChevronRightIcon,
    ChevronUpIcon,
    DownloadIcon,
    MapPinIcon,
    SearchIcon,
    XIcon,
} from "lucide-react"

import { QuantityValue, QuickPreviewSheet } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { LoadingButton } from "@/components/ui/loading-button"
import { SheetClose } from "@/components/ui/sheet"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toast } from "@/components/ui/toast"
import { fullDeliveryAddress } from "@/lib/delivery-address"
import { compactFixed } from "@/lib/fixed-decimal"
import {
    buildPurchaseDeliveryPlanSummary,
    IMAGE_INLINE_ADDRESS_LIMIT,
    PREVIEW_INLINE_ADDRESS_LIMIT,
    type PurchaseDeliveryAddressGroup,
    type PurchaseDeliveryItemGroup,
    type PurchaseDeliveryPlanLine,
} from "../lib/purchase-delivery-plan"

export type PurchaseDeliveryPlanMode =
    | "interactive"
    | "export-full"
    | "export-summary"

/** 对外单据只使用本采购行已经确认的交付安排。 */
export function PurchaseDeliveryPlan({
    lines,
    warehouse,
    warehouseName,
    supplierName,
    documentLabel,
    mode = "interactive",
    attachmentFileName,
    idPrefix = "purchase-delivery-plan",
}: {
    lines: readonly PurchaseDeliveryPlanLine[]
    warehouse: boolean
    warehouseName?: string
    supplierName?: string
    documentLabel?: string
    mode?: PurchaseDeliveryPlanMode
    /** export-summary 模式下提示查阅的配套 Excel 文件名。 */
    attachmentFileName?: string
    idPrefix?: string
}) {
    const summary = React.useMemo(
        () => buildPurchaseDeliveryPlanSummary(lines),
        [lines],
    )
    const isInteractive = mode === "interactive"
    const isLarge = summary.totalRows > PREVIEW_INLINE_ADDRESS_LIMIT
    const [expanded, setExpanded] = React.useState(false)
    const [sheetOpen, setSheetOpen] = React.useState(false)
    const [query, setQuery] = React.useState("")
    const [viewBy, setViewBy] = React.useState<"item" | "address">("item")
    const [exporting, setExporting] = React.useState(false)

    React.useEffect(() => {
        setExpanded(false)
        setSheetOpen(false)
        setQuery("")
        setViewBy("item")
    }, [idPrefix, documentLabel, supplierName])

    const normalizedQuery = query.trim().toLocaleLowerCase()
    const filteredItemGroups = React.useMemo(
        () => filterItemGroups(summary.itemGroups, normalizedQuery),
        [summary.itemGroups, normalizedQuery],
    )
    const filteredAddressGroups = React.useMemo(
        () => filterAddressGroups(summary.addressGroups, normalizedQuery),
        [summary.addressGroups, normalizedQuery],
    )
    const matchedRows = filteredItemGroups.reduce(
        (total, group) => total + group.allocations.length,
        0,
    )

    async function handleExportWorkbook() {
        if (exporting) return
        setExporting(true)
        try {
            const { downloadPurchaseDeliveryWorkbook } =
                await import("../lib/purchase-delivery-workbook")
            await downloadPurchaseDeliveryWorkbook({
                documentLabel,
                supplierName: supplierName || "采购单",
                lines,
            })
            toast.add({
                title: "发货地址明细表已下载",
                description: "已保存为 Excel（.xlsx），可直接用于发单打单。",
                type: "success",
            })
        } catch {
            toast.add({
                title: "发货地址明细表导出失败",
                description: "请稍后重试。",
                type: "error",
            })
        } finally {
            setExporting(false)
        }
    }

    if (warehouse) {
        return (
            <section
                aria-label="交付安排"
                className="mt-8 space-y-2 border-t border-border pt-6 text-body-compact"
            >
                <h3 className="font-semibold">交付安排</h3>
                <p>
                    采购入库目标仓：{warehouseName || "以采购入库安排为准"}
                    。客户配送由仓库另行安排。
                </p>
            </section>
        )
    }

    const addressStats =
        summary.totalRows > 0
            ? summary.uniqueAddressCount === summary.totalRows
                ? `共 ${summary.totalRows} 处收货地址`
                : `共 ${summary.totalRows} 条分配 · ${summary.uniqueAddressCount} 处独立地址`
            : "暂未指定收货地址"

    const showSummaryOnly =
        mode === "export-summary" || (isInteractive && isLarge && !expanded)

    return (
        <section
            aria-label="交付安排"
            className="mt-8 space-y-3 border-t border-border pt-6 text-body-compact"
        >
            <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                    <h3 className="font-semibold">
                        {mode === "export-summary"
                            ? "交付安排（摘要）"
                            : "交付安排"}
                    </h3>
                    <span className="num text-xs text-muted-foreground">
                        {addressStats}
                    </span>
                </div>

                {isInteractive && summary.totalRows > 0 && isLarge ? (
                    <div className="flex flex-wrap items-center gap-2 print:hidden">
                        <Button
                            id={`${idPrefix}-open-sheet`}
                            type="button"
                            variant="outline"
                            size="xs"
                            onClick={() => setSheetOpen(true)}
                        >
                            <MapPinIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            核对全部地址（{summary.totalRows}）
                            <ChevronRightIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                            />
                        </Button>
                        <Button
                            id={`${idPrefix}-toggle-expand`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            onClick={() => setExpanded((prev) => !prev)}
                        >
                            {expanded ? (
                                <>
                                    <ChevronUpIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    收起纸面明细
                                </>
                            ) : (
                                <>
                                    <ChevronDownIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    纸面展开
                                </>
                            )}
                        </Button>
                    </div>
                ) : null}
            </div>

            {summary.itemGroups.length === 0 ? (
                <p className="text-xs text-muted-foreground">
                    暂无商品交付安排
                </p>
            ) : showSummaryOnly ? (
                <div className="space-y-3">
                    <DeliverySummaryTable groups={summary.itemGroups} />
                    {mode === "export-summary" ? (
                        <div className="rounded-sm border border-border bg-muted/30 px-3 py-2.5 text-xs leading-5 text-foreground">
                            本采购单共含{" "}
                            <span className="num font-medium">
                                {summary.totalRows}
                            </span>{" "}
                            条收件分配（
                            <span className="num font-medium">
                                {summary.uniqueAddressCount}
                            </span>{" "}
                            处独立地址）。为保证单据图片清晰可读，完整收货人、联系电话、详细地址及数量请查阅同包配套表格：
                            <span className="font-medium">
                                《{attachmentFileName || "发货地址明细.xlsx"}》
                            </span>
                            。
                        </div>
                    ) : (
                        <div className="flex flex-wrap items-center justify-between gap-2 rounded-sm border border-border/80 bg-muted/30 px-3 py-2 text-xs text-muted-foreground print:hidden">
                            <span>
                                {summary.totalRows > IMAGE_INLINE_ADDRESS_LIMIT
                                    ? `地址超过 ${IMAGE_INLINE_ADDRESS_LIMIT} 条：下载采购单时将自动导出清晰主单图片及配套 Excel 发货地址明细表。`
                                    : "可在右侧抽屉按收货人、电话、地址或商品检索核对。"}
                            </span>
                            {summary.topRegions.length > 0 ? (
                                <span className="num">
                                    主要地区：
                                    {summary.topRegions
                                        .slice(0, 2)
                                        .map(
                                            (region) =>
                                                `${region.name} ${region.count} 处`,
                                        )
                                        .join("、")}
                                    {summary.topRegions.length > 2
                                        ? ` 等 ${summary.topRegions.length} 个地区`
                                        : ""}
                                </span>
                            ) : null}
                        </div>
                    )}
                </div>
            ) : (
                <GroupedDeliveryTables groups={summary.itemGroups} />
            )}

            {isInteractive && summary.totalRows > 0 ? (
                <QuickPreviewSheet
                    idPrefix={`${idPrefix}-sheet`}
                    open={sheetOpen}
                    onOpenChange={setSheetOpen}
                    size="detail"
                    identity={
                        supplierName ? `交付安排 · ${supplierName}` : "交付安排"
                    }
                    title="收货地址明细"
                    description={documentLabel}
                    summary={
                        <p className="num text-xs text-muted-foreground">
                            {addressStats} · 涉及 {summary.itemGroups.length}{" "}
                            项商品
                        </p>
                    }
                    footer={
                        <>
                            <LoadingButton
                                id={`${idPrefix}-sheet-export-xlsx`}
                                type="button"
                                variant="outline"
                                size="sm"
                                loading={exporting}
                                disabled={exporting}
                                onClick={() => void handleExportWorkbook()}
                            >
                                <DownloadIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                {exporting ? "导出中…" : "导出地址表 (.xlsx)"}
                            </LoadingButton>
                            <SheetClose
                                id={`${idPrefix}-sheet-footer-close`}
                                render={
                                    <Button
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                    />
                                }
                            >
                                关闭
                            </SheetClose>
                        </>
                    }
                >
                    <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-border px-7 py-3">
                        <div className="relative min-w-52 flex-1">
                            <SearchIcon
                                aria-hidden="true"
                                className="pointer-events-none absolute top-2.5 left-3 size-4 text-muted-foreground"
                            />
                            <Input
                                id={`${idPrefix}-sheet-search`}
                                aria-label="搜索收货人、电话、地址或商品"
                                placeholder="搜索收货人、电话、地址或商品…"
                                className="h-9 pr-9 pl-9"
                                value={query}
                                onChange={(event) =>
                                    setQuery(event.target.value)
                                }
                            />
                            {query ? (
                                <Button
                                    id={`${idPrefix}-sheet-search-clear`}
                                    type="button"
                                    size="icon-sm"
                                    variant="ghost"
                                    aria-label="清除搜索"
                                    className="absolute top-0.5 right-0.5"
                                    onClick={() => setQuery("")}
                                >
                                    <XIcon />
                                </Button>
                            ) : null}
                        </div>
                        <div
                            role="group"
                            aria-label="地址查看维度"
                            className="flex shrink-0 items-center gap-1"
                        >
                            <Button
                                id={`${idPrefix}-sheet-view-item`}
                                type="button"
                                size="sm"
                                variant={
                                    viewBy === "item" ? "secondary" : "ghost"
                                }
                                aria-pressed={viewBy === "item"}
                                onClick={() => setViewBy("item")}
                            >
                                按商品
                            </Button>
                            <Button
                                id={`${idPrefix}-sheet-view-address`}
                                type="button"
                                size="sm"
                                variant={
                                    viewBy === "address" ? "secondary" : "ghost"
                                }
                                aria-pressed={viewBy === "address"}
                                onClick={() => setViewBy("address")}
                            >
                                按收货地址
                            </Button>
                        </div>
                        <span className="num text-xs text-muted-foreground">
                            {viewBy === "item"
                                ? `显示 ${matchedRows} / ${summary.totalRows} 条`
                                : `显示 ${filteredAddressGroups.length} / ${summary.uniqueAddressCount} 处`}
                        </span>
                    </div>

                    <div className="min-h-0 flex-1 overflow-y-auto px-7 py-5">
                        {viewBy === "item" ? (
                            filteredItemGroups.length > 0 ? (
                                <GroupedDeliveryTables
                                    groups={filteredItemGroups}
                                />
                            ) : (
                                <p className="py-10 text-center text-xs text-muted-foreground">
                                    未找到匹配的收货地址，请调整搜索词后重试。
                                </p>
                            )
                        ) : filteredAddressGroups.length > 0 ? (
                            <AddressGroupedList
                                groups={filteredAddressGroups}
                            />
                        ) : (
                            <p className="py-10 text-center text-xs text-muted-foreground">
                                未找到匹配的收货地址，请调整搜索词后重试。
                            </p>
                        )}
                    </div>
                </QuickPreviewSheet>
            ) : null}
        </section>
    )
}

function DeliverySummaryTable({
    groups,
}: {
    groups: readonly PurchaseDeliveryItemGroup[]
}) {
    return (
        <Table className="w-full table-fixed border-collapse text-xs">
            <TableHeader>
                <TableRow className="bg-muted/40 text-left hover:bg-muted/40">
                    <TableHead className="w-[34%] border border-border p-2 font-medium">
                        商品 / 规格
                    </TableHead>
                    <TableHead className="w-[14%] border border-border p-2 text-right font-medium">
                        地址数
                    </TableHead>
                    <TableHead className="w-[16%] border border-border p-2 text-right font-medium">
                        分配数量
                    </TableHead>
                    <TableHead className="border border-border p-2 font-medium">
                        收货地区 / 示例
                    </TableHead>
                </TableRow>
            </TableHeader>
            <TableBody>
                {groups.map((group) => (
                    <TableRow
                        key={group.key}
                        className="break-inside-avoid hover:bg-transparent"
                    >
                        <TableCell className="whitespace-normal break-words border border-border p-2 align-top">
                            <div className="font-medium text-foreground">
                                {group.itemName}
                            </div>
                            {group.itemSku ? (
                                <div className="num mt-0.5 text-muted-foreground">
                                    {group.itemSku}
                                </div>
                            ) : null}
                        </TableCell>
                        <TableCell className="num border border-border p-2 text-right align-top">
                            {group.allocations.length > 0
                                ? `${group.allocations.length} 处`
                                : "未指定"}
                        </TableCell>
                        <TableCell className="border border-border p-2 text-right align-top">
                            {group.allocations.length > 0 ? (
                                <QuantityValue
                                    value={group.allocatedTotal}
                                    unit={group.unit}
                                />
                            ) : (
                                <span className="text-muted-foreground">—</span>
                            )}
                        </TableCell>
                        <TableCell className="whitespace-normal break-words border border-border p-2 align-top text-muted-foreground">
                            {group.regionSummary}
                        </TableCell>
                    </TableRow>
                ))}
            </TableBody>
        </Table>
    )
}

function GroupedDeliveryTables({
    groups,
}: {
    groups: readonly PurchaseDeliveryItemGroup[]
}) {
    return (
        <div className="space-y-4">
            {groups.map((group) => (
                <div
                    key={group.key}
                    className="overflow-hidden rounded-sm border border-border"
                >
                    <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 border-b border-border bg-muted/40 px-3 py-2 text-xs">
                        <div className="min-w-0 font-medium text-foreground">
                            <span>{group.itemName}</span>
                            {group.itemSku ? (
                                <span className="num ml-2 font-normal text-muted-foreground">
                                    {group.itemSku}
                                </span>
                            ) : null}
                        </div>
                        <div className="num shrink-0 text-muted-foreground">
                            {group.allocations.length > 0 ? (
                                <>
                                    {group.allocations.length} 处地址 · 合计{" "}
                                    <QuantityValue
                                        value={group.allocatedTotal}
                                        unit={group.unit}
                                    />
                                </>
                            ) : (
                                "未指定交付地址，履约前补齐"
                            )}
                        </div>
                    </div>

                    {group.allocations.length > 0 ? (
                        <Table className="w-full table-fixed border-collapse text-xs">
                            <TableHeader>
                                <TableRow className="bg-muted/20 text-left hover:bg-muted/20">
                                    <TableHead className="w-[8%] border-r border-b border-border px-2 py-1.5 font-medium">
                                        序号
                                    </TableHead>
                                    <TableHead className="w-[26%] border-r border-b border-border px-2 py-1.5 font-medium">
                                        收货人 / 电话
                                    </TableHead>
                                    <TableHead className="border-r border-b border-border px-2 py-1.5 font-medium">
                                        完整地址
                                    </TableHead>
                                    <TableHead className="w-[14%] border-b border-border px-2 py-1.5 text-right font-medium">
                                        数量
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {group.allocations.map((row, index) => (
                                    <TableRow
                                        key={`${group.key}-${index}`}
                                        className="break-inside-avoid hover:bg-transparent"
                                    >
                                        <TableCell className="num border-r border-b border-border px-2 py-1.5 align-top text-muted-foreground">
                                            {index + 1}
                                        </TableCell>
                                        <TableCell className="whitespace-normal break-words border-r border-b border-border px-2 py-1.5 align-top">
                                            <div>{row.recipient}</div>
                                            <div className="num mt-0.5 text-muted-foreground">
                                                {row.phone}
                                            </div>
                                        </TableCell>
                                        <TableCell className="whitespace-normal break-words border-r border-b border-border px-2 py-1.5 align-top">
                                            {fullDeliveryAddress(row)}
                                        </TableCell>
                                        <TableCell className="border-b border-border px-2 py-1.5 text-right align-top">
                                            <QuantityValue
                                                value={compactFixed(
                                                    row.quantity,
                                                )}
                                                unit={group.unit}
                                            />
                                        </TableCell>
                                    </TableRow>
                                ))}
                            </TableBody>
                        </Table>
                    ) : null}
                </div>
            ))}
        </div>
    )
}

function AddressGroupedList({
    groups,
}: {
    groups: readonly PurchaseDeliveryAddressGroup[]
}) {
    return (
        <ol className="divide-y divide-border/70">
            {groups.map((address, index) => (
                <li
                    key={address.key}
                    className="space-y-2 py-4 first:pt-0 last:pb-0"
                >
                    <div className="flex flex-wrap items-baseline justify-between gap-2">
                        <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
                            <span className="num text-xs text-muted-foreground">
                                #{index + 1}
                            </span>
                            <span className="font-medium text-foreground">
                                {address.recipient}
                            </span>
                            <span className="num text-xs text-muted-foreground">
                                {address.phone}
                            </span>
                        </div>
                        <span className="num text-xs text-muted-foreground">
                            {address.items.length} 项商品
                        </span>
                    </div>
                    <p className="text-xs leading-5 break-words text-muted-foreground">
                        {address.fullAddress}
                    </p>
                    <ul className="space-y-1 rounded-sm bg-muted/35 px-3 py-2 text-xs">
                        {address.items.map((item) => (
                            <li
                                key={`${address.key}-${item.itemKey}`}
                                className="flex items-baseline justify-between gap-4"
                            >
                                <span className="min-w-0 break-words text-foreground">
                                    {item.itemName}
                                    {item.itemSku ? (
                                        <span className="num ml-1.5 text-muted-foreground">
                                            {item.itemSku}
                                        </span>
                                    ) : null}
                                </span>
                                <span className="shrink-0 font-medium">
                                    <QuantityValue
                                        value={item.quantity}
                                        unit={item.unit}
                                    />
                                </span>
                            </li>
                        ))}
                    </ul>
                </li>
            ))}
        </ol>
    )
}

function filterItemGroups(
    groups: readonly PurchaseDeliveryItemGroup[],
    query: string,
): PurchaseDeliveryItemGroup[] {
    if (!query) return [...groups]
    const result: PurchaseDeliveryItemGroup[] = []
    for (const group of groups) {
        const itemMatched = `${group.itemName} ${group.itemSku ?? ""}`
            .toLocaleLowerCase()
            .includes(query)
        if (itemMatched) {
            result.push(group)
            continue
        }
        const matchedAllocations = group.allocations.filter((row) =>
            `${row.recipient} ${row.phone} ${fullDeliveryAddress(row)}`
                .toLocaleLowerCase()
                .includes(query),
        )
        if (matchedAllocations.length > 0) {
            result.push({
                ...group,
                allocations: matchedAllocations,
            })
        }
    }
    return result
}

function filterAddressGroups(
    groups: readonly PurchaseDeliveryAddressGroup[],
    query: string,
): PurchaseDeliveryAddressGroup[] {
    if (!query) return [...groups]
    return groups.filter((group) => {
        const headerMatched =
            `${group.recipient} ${group.phone} ${group.fullAddress}`
                .toLocaleLowerCase()
                .includes(query)
        if (headerMatched) return true
        return group.items.some((item) =>
            `${item.itemName} ${item.itemSku ?? ""}`
                .toLocaleLowerCase()
                .includes(query),
        )
    })
}
