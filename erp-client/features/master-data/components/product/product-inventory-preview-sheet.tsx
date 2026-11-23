"use client"

import * as React from "react"
import Link from "next/link"
import { ExternalLinkIcon, RefreshCwIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    DataFreshness,
    OptionCombobox,
    QuickPreviewSheet,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { ScrollArea } from "@/components/ui/scroll-area"
import {
    Table,
    TableBody,
    TableCaption,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { useInventoryListQuery } from "@/features/inventory/queries"
import type { InventoryQuery } from "@/features/inventory/types"
import type { ProductKind } from "@/features/master-data/types"
import { formatDateTime } from "@/lib/datetime"

export type ProductInventoryPreviewSku = Readonly<{
    skuId: string
    skuNo: string
    specLabel: string
    baseUnit: string
}>

type ProductInventoryPreviewSheetProps = Readonly<{
    open: boolean
    onOpenChange: (open: boolean) => void
    productName: string
    productKind: ProductKind | ""
    skus: readonly ProductInventoryPreviewSku[]
    initialSkuId?: string
}>

const MAX_PREVIEW_ROWS = 100

function quantity(value: string, unit: string) {
    return (
        <span className="num whitespace-nowrap">
            {value}
            <span className="ml-1 text-xs font-normal text-muted-foreground">
                {unit}
            </span>
        </span>
    )
}

/** 商品编辑工作面内的只读库存摘要；正式库存事实仍由 W10 提供。 */
export function ProductInventoryPreviewSheet({
    open,
    onOpenChange,
    productName,
    productKind,
    skus,
    initialSkuId,
}: ProductInventoryPreviewSheetProps) {
    const [selectedSkuId, setSelectedSkuId] = React.useState<string | null>(
        null,
    )

    React.useEffect(() => {
        if (!open) return
        const initial =
            skus.find((sku) => sku.skuId === initialSkuId) ?? skus[0]
        setSelectedSkuId(initial?.skuId ?? null)
    }, [initialSkuId, open, skus])

    const selectedSku = React.useMemo(
        () => skus.find((sku) => sku.skuId === selectedSkuId),
        [selectedSkuId, skus],
    )
    const query = React.useMemo<InventoryQuery>(
        () => ({
            view: "balance",
            skuId: selectedSkuId ?? "",
            availability: "all",
            pageSize: MAX_PREVIEW_ROWS,
            sort: ["warehouseCode:asc", "skuCode:asc"],
        }),
        [selectedSkuId],
    )
    const inventoryQuery = useInventoryListQuery(
        query,
        open && productKind === "PHYSICAL" && Boolean(selectedSkuId),
    )
    const data = inventoryQuery.data
    const fullLedgerHref = selectedSkuId
        ? `/inventory?view=balance&skuId=${encodeURIComponent(selectedSkuId)}`
        : "/inventory?view=balance"

    const content = (() => {
        if (productKind !== "PHYSICAL") {
            return (
                <BusinessEmptyState
                    kind="no-data"
                    title="该商品不适用自有库存"
                    description="只有实物商品进入公司自有库存台账；虚拟商品、服务和卡券不在这里记录库存。"
                    className="m-5"
                />
            )
        }
        if (!selectedSku) {
            return (
                <BusinessEmptyState
                    kind="no-data"
                    title="保存商品后可查看库存"
                    description="新增规格需先保存商品后，才能查询库存记录。"
                    className="m-5"
                />
            )
        }
        if (inventoryQuery.isPending) {
            return (
                <div
                    className="space-y-3 p-5"
                    aria-busy="true"
                    aria-label="正在加载商品库存"
                >
                    <div className="h-20 animate-pulse rounded-xl bg-muted" />
                    <div className="h-48 animate-pulse rounded-xl bg-muted" />
                </div>
            )
        }
        if (inventoryQuery.isError || !data) {
            return (
                <BusinessFailureState
                    title="商品库存暂时无法读取"
                    error={inventoryQuery.error}
                    onRetry={() => void inventoryQuery.refetch()}
                    className="m-5"
                />
            )
        }
        if (!data.moduleAllowed || data.emptyReason === "PERMISSION_REVOKED") {
            return (
                <BusinessFailureState
                    kind="permission"
                    title="库存台账访问权限已收回"
                    description="当前账号不能查看该 SKU 的余额；商品编辑内容未受影响。"
                    className="m-5"
                />
            )
        }
        if (!data.hasWarehouseScope || data.emptyReason === "NO_DATA_SCOPE") {
            return (
                <BusinessEmptyState
                    kind="no-scope"
                    title="暂无仓库库存查看权限"
                    description="您当前尚未开通任何仓库的数据查看权限，请联系管理员配置。"
                    className="m-5"
                />
            )
        }
        if (data.balances.length === 0) {
            return (
                <BusinessEmptyState
                    kind="no-data"
                    title="当前 SKU 暂无自有库存记录"
                    description="这里只展示公司自有实物库存，不包含供应商外部可供数量。"
                    className="m-5"
                />
            )
        }

        return (
            <ScrollArea className="min-h-0 flex-1">
                <div className="space-y-6 px-sheet-inline py-5">
                    <div className="flex flex-wrap items-center justify-between gap-3">
                        <DataFreshness
                            updatedAt={formatDateTime(
                                data.queriedAt,
                                "full",
                                "passthrough",
                            )}
                            dateTime={data.queriedAt}
                            label="库存记录更新时间"
                        />
                        <Button
                            id="master-data-product-inventory-refresh"
                            type="button"
                            variant="ghost"
                            size="sm"
                            disabled={inventoryQuery.isFetching}
                            onClick={() => void inventoryQuery.refetch()}
                        >
                            <RefreshCwIcon
                                data-icon="inline-start"
                                aria-hidden
                                className={
                                    inventoryQuery.isFetching
                                        ? "animate-spin"
                                        : undefined
                                }
                            />
                            刷新
                        </Button>
                    </div>

                    <div className="overflow-hidden rounded-xl border border-border">
                        <Table data-density="compact">
                            <TableCaption className="sr-only">
                                {selectedSku.skuNo} 按仓库展示的库存余额
                            </TableCaption>
                            <TableHeader>
                                <TableRow>
                                    <TableHead>仓库</TableHead>
                                    <TableHead className="text-right">
                                        账面现存
                                    </TableHead>
                                    <TableHead className="text-right">
                                        有效预占
                                    </TableHead>
                                    <TableHead className="text-right">
                                        可用数量
                                    </TableHead>
                                    <TableHead>状态</TableHead>
                                    <TableHead>最后变动</TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {data.balances.map((balance) => (
                                    <TableRow key={balance.balanceId}>
                                        <TableCell>
                                            <div className="font-medium">
                                                {balance.warehouseName}
                                            </div>
                                            <div className="num text-xs text-muted-foreground">
                                                {balance.warehouseCode}
                                            </div>
                                        </TableCell>
                                        <TableCell className="text-right">
                                            {quantity(
                                                balance.onHandQuantity,
                                                balance.baseUnit,
                                            )}
                                        </TableCell>
                                        <TableCell className="text-right">
                                            {quantity(
                                                balance.reservedQuantity,
                                                balance.baseUnit,
                                            )}
                                        </TableCell>
                                        <TableCell className="text-right font-medium text-primary">
                                            {quantity(
                                                balance.availableQuantity,
                                                balance.baseUnit,
                                            )}
                                        </TableCell>
                                        <TableCell>
                                            <BusinessStatusBadge
                                                context="list"
                                                label={balance.statusLabel}
                                                tone={balance.statusTone}
                                            />
                                        </TableCell>
                                        <TableCell>
                                            <div>
                                                {balance.lastMovementTypeLabel}
                                            </div>
                                            <div className="num text-xs text-muted-foreground">
                                                {formatDateTime(
                                                    balance.lastMovementAt,
                                                    "full",
                                                    "passthrough",
                                                )}
                                            </div>
                                        </TableCell>
                                    </TableRow>
                                ))}
                            </TableBody>
                        </Table>
                    </div>

                    {data.total > data.balances.length ? (
                        <p className="text-xs text-muted-foreground">
                            当前仅展示前 {MAX_PREVIEW_ROWS}{" "}
                            个仓库组合；全部结果请在完整库存台账查看。
                        </p>
                    ) : null}
                </div>
            </ScrollArea>
        )
    })()

    return (
        <QuickPreviewSheet
            open={open}
            onOpenChange={onOpenChange}
            size="detail"
            title="商品库存"
            identity={
                selectedSku ? (
                    <span className="num">
                        {productName || "商品"} · {selectedSku.skuNo}
                    </span>
                ) : null
            }
            summary={
                <div className="space-y-3">
                    <div className="flex flex-wrap items-center gap-2">
                        <Badge variant="secondary">自有实物</Badge>
                        {data?.hasWarehouseScope && selectedSku ? (
                            <Badge variant="outline">
                                {data.metrics.balanceDimensionCount} 个仓库组合
                            </Badge>
                        ) : null}
                    </div>
                    <div className="max-w-md">
                        <OptionCombobox
                            id="master-data-product-inventory-sku-combobox"
                            options={skus.map((sku) => ({
                                value: sku.skuId,
                                label: `${sku.specLabel} · ${sku.skuNo}`,
                                keywords: sku.skuNo,
                            }))}
                            value={selectedSkuId}
                            onValueChange={setSelectedSkuId}
                            allowClear={false}
                            disabled={skus.length <= 1}
                            placeholder="选择已保存 SKU"
                            emptyLabel="没有可查询库存的已保存 SKU"
                            aria-label="选择要查看库存的 SKU"
                        />
                    </div>
                </div>
            }
            footer={
                <>
                    <Button
                        id="master-data-product-inventory-close"
                        type="button"
                        variant="outline"
                        onClick={() => onOpenChange(false)}
                    >
                        关闭
                    </Button>
                    <Button
                        id="master-data-product-inventory-open-ledger"
                        type="button"
                        disabled={!selectedSku || productKind !== "PHYSICAL"}
                        render={
                            <Link
                                href={fullLedgerHref}
                                target="_blank"
                                rel="noopener noreferrer"
                            />
                        }
                    >
                        <ExternalLinkIcon
                            data-icon="inline-start"
                            aria-hidden
                        />
                        新标签打开完整台账
                    </Button>
                </>
            }
        >
            {content}
        </QuickPreviewSheet>
    )
}
