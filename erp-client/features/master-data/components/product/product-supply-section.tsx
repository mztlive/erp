"use client"

import * as React from "react"
import Link from "next/link"
import {
    ArrowUpRightIcon,
    ChevronDownIcon,
    ChevronUpIcon,
    PlusIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    MoneyValue,
    QuantityValue,
} from "@/components/business"
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
import { useAccountProfileQuery } from "@/features/auth/queries"
import { toFixedSku } from "@/features/master-data/lib/product-fixed-sku"
import { ProductSupplyIdentity } from "./product-supply-identity"
import type {
    ProductFields,
    ProductSkuFields,
} from "@/features/master-data/types"
import { useSupplierOfferingsForSkusQuery } from "@/features/supplier-offerings/queries"
import {
    AVAILABILITY_STATUS_LABELS,
    OFFERING_STATUS_LABELS,
    type FixedSku,
    type SupplierOfferingView,
} from "@/features/supplier-offerings/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"

const NO_COST = "当前账号无权查看采购成本"

/** 有效供给关系：启用且已有条款版本，不代表供应商当前可供。 */
function isActive(offering: SupplierOfferingView): boolean {
    return offering.status === "ACTIVE" && Boolean(offering.current_revision_id)
}

function sortOfferings(
    offerings: readonly SupplierOfferingView[],
): SupplierOfferingView[] {
    return [...offerings].sort(
        (left, right) =>
            Number(isActive(right)) - Number(isActive(left)) ||
            (left.supplier_name ?? "").localeCompare(
                right.supplier_name ?? "",
                "zh-CN",
            ),
    )
}

function OfferingRows({
    offerings,
    canViewCosts,
    unit,
}: {
    offerings: readonly SupplierOfferingView[]
    canViewCosts: boolean
    unit: string
}) {
    const hidden = canViewCosts ? undefined : NO_COST
    return (
        <Table className="min-w-5xl table-fixed">
            <colgroup>
                <col className="w-[28%]" />
                <col className="w-[11%]" />
                <col className="w-[10%]" />
                <col className="w-[17%]" />
                <col className="w-[19%]" />
                <col className="w-[8%]" />
                <col className="w-[7%]" />
            </colgroup>
            <TableHeader>
                <TableRow>
                    <TableHead className="min-w-52">供应商</TableHead>
                    <TableHead className="text-right">一件代发价</TableHead>
                    <TableHead className="text-right">集采价</TableHead>
                    <TableHead>当前可供</TableHead>
                    <TableHead>配送模板</TableHead>
                    <TableHead>关系状态</TableHead>
                    <TableHead className="w-24">
                        <span className="sr-only">操作</span>
                    </TableHead>
                </TableRow>
            </TableHeader>
            <TableBody>
                {offerings.map((offering) => (
                    <TableRow key={offering.id}>
                        <TableCell className="whitespace-normal wrap-anywhere">
                            <div className="font-medium">
                                {offering.supplier_name ??
                                    offering.supplier_no ??
                                    "供应商名称未返回"}
                            </div>
                            <div className="num mt-0.5 text-xs text-muted-foreground">
                                订货编码 {offering.supplier_sku_code}
                            </div>
                        </TableCell>
                        <TableCell className="text-right">
                            <MoneyValue
                                value={offering.dropship_supply_price_gross}
                                unavailableReason={hidden}
                            />
                        </TableCell>
                        <TableCell className="text-right">
                            <MoneyValue
                                value={offering.bulk_supply_price_gross}
                                unavailableReason={hidden}
                            />
                            {offering.bulk_minimum_order_quantity ? (
                                <div className="mt-0.5 text-xs text-muted-foreground">
                                    起订{" "}
                                    <QuantityValue
                                        value={
                                            offering.bulk_minimum_order_quantity
                                        }
                                        unit={unit}
                                    />
                                </div>
                            ) : null}
                        </TableCell>
                        <TableCell>
                            <div className="flex flex-wrap items-center gap-2">
                                <Badge
                                    variant={
                                        offering.availability_status ===
                                        "AVAILABLE"
                                            ? "success"
                                            : "outline"
                                    }
                                >
                                    {offering.availability_status
                                        ? AVAILABILITY_STATUS_LABELS[
                                              offering.availability_status
                                          ]
                                        : "未更新"}
                                </Badge>
                                <span className="text-xs text-muted-foreground">
                                    {offering.available_quantity != null ? (
                                        <QuantityValue
                                            value={offering.available_quantity}
                                            unit={unit}
                                        />
                                    ) : (
                                        "数量未提供"
                                    )}
                                </span>
                            </div>
                        </TableCell>
                        <TableCell className="text-sm whitespace-normal wrap-anywhere">
                            {offering.delivery_template?.display_name ?? "—"}
                        </TableCell>
                        <TableCell>
                            <Badge
                                variant={
                                    offering.status === "ACTIVE"
                                        ? "success"
                                        : "secondary"
                                }
                            >
                                {OFFERING_STATUS_LABELS[offering.status]}
                            </Badge>
                        </TableCell>
                        <TableCell className="text-right">
                            <Button
                                id={`master-data-product-supply-open-${toAutomationIdSegment(offering.id)}`}
                                variant="ghost"
                                size="sm"
                                render={
                                    <Link
                                        href={`/procurement/supplier-offerings/${encodeURIComponent(offering.id)}`}
                                    />
                                }
                            >
                                查看
                                <ArrowUpRightIcon
                                    data-icon="inline-end"
                                    aria-hidden
                                />
                            </Button>
                        </TableCell>
                    </TableRow>
                ))}
            </TableBody>
        </Table>
    )
}

function SkuSupplyGroup({
    sku,
    fields,
    productName,
    differingDimensions,
    collapsible,
    offerings,
    loading,
    failed,
    canCreate,
    canViewCosts,
    unit,
    onAdd,
}: {
    sku: ProductSkuFields & { skuId: string }
    fields: ProductFields
    productName: string
    differingDimensions: readonly boolean[]
    collapsible: boolean
    offerings: readonly SupplierOfferingView[]
    loading: boolean
    failed: boolean
    canCreate: boolean
    canViewCosts: boolean
    unit: string
    onAdd: () => void
}) {
    const [collapsed, setCollapsed] = React.useState(false)
    const expanded = !collapsible || !collapsed
    const segment = toAutomationIdSegment(sku.skuId)
    const activeSuppliers = new Set(
        offerings.filter(isActive).map((item) => item.supplier_id),
    ).size
    return (
        <section
            aria-labelledby={`master-data-product-supply-${segment}-title`}
            className="min-w-0 overflow-hidden rounded-lg border bg-card"
        >
            <header className="flex flex-col gap-4 bg-muted/30 px-5 py-5 lg:flex-row lg:items-start lg:justify-between">
                <ProductSupplyIdentity
                    sku={sku}
                    fields={fields}
                    productName={productName}
                    titleId={`master-data-product-supply-${segment}-title`}
                    differingDimensions={differingDimensions}
                />
                <div className="flex shrink-0 flex-wrap items-center gap-3 lg:pt-1">
                    {loading || failed ? null : (
                        <Badge
                            variant={
                                activeSuppliers > 0 ? "outline" : "warning"
                            }
                        >
                            {activeSuppliers > 0
                                ? `有效供给：${activeSuppliers} 家`
                                : offerings.length === 0
                                  ? "尚未添加供给"
                                  : "暂无有效供给"}
                        </Badge>
                    )}
                    {canCreate ? (
                        <Button
                            id={`master-data-product-sku-${segment}-add-supply`}
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={onAdd}
                        >
                            <PlusIcon data-icon="inline-start" aria-hidden />
                            添加供给
                        </Button>
                    ) : null}
                    {collapsible ? (
                        <Button
                            id={`master-data-product-supply-${segment}-toggle`}
                            type="button"
                            variant="ghost"
                            size="sm"
                            aria-expanded={expanded}
                            aria-controls={`master-data-product-supply-${segment}-content`}
                            aria-label={`${expanded ? "收起" : "展开"} ${sku.specLabel} 的供应商供给`}
                            onClick={() => setCollapsed(!collapsed)}
                        >
                            {expanded ? "收起" : "展开"}
                            {expanded ? (
                                <ChevronUpIcon aria-hidden />
                            ) : (
                                <ChevronDownIcon aria-hidden />
                            )}
                        </Button>
                    ) : null}
                </div>
            </header>
            <div
                id={`master-data-product-supply-${segment}-content`}
                hidden={!expanded}
                className="border-t"
            >
                {loading ? (
                    <p className="px-4 py-6 text-sm text-muted-foreground">
                        正在读取供给…
                    </p>
                ) : failed ? (
                    <p className="px-5 py-6 text-sm text-muted-foreground">
                        供给信息读取失败，请在上方重试。
                    </p>
                ) : offerings.length === 0 ? (
                    <p className="px-4 py-6 text-sm text-muted-foreground">
                        尚未添加供给。没有有效供给的规格不能进入商品池销售。
                    </p>
                ) : (
                    <OfferingRows
                        offerings={offerings}
                        canViewCosts={canViewCosts}
                        unit={unit}
                    />
                )}
            </div>
        </section>
    )
}

/** 商品资料「供应商供给」分区：按 SKU 列出供应商、报价、可供与配送，并就地添加供给。 */
export function ProductSupplySection({
    fields,
    name,
    onRegisterSupply,
}: {
    fields: ProductFields
    name: string
    onRegisterSupply: (sku: FixedSku) => void
}) {
    const [search, setSearch] = React.useState("")
    const account = useAccountProfileQuery()
    const permissions = account.data?.permissions
    const canCreate = hasPermission(permissions, "supplier_offering:create")
    const canViewCosts = hasPermission(
        permissions,
        "supplier_offering_cost:detail",
    )
    const savedSkus = fields.skus.filter(
        (sku): sku is ProductSkuFields & { skuId: string } =>
            Boolean(sku.skuId),
    )
    const unsavedCount = fields.skus.length - savedSkus.length
    const showSearch = savedSkus.length > 6
    const searchTerm = showSearch ? search.trim().toLocaleLowerCase() : ""
    const visibleSkus = savedSkus.filter((sku) =>
        [sku.skuNo, sku.name, sku.specLabel, ...sku.attributeValues]
            .join(" ")
            .toLocaleLowerCase()
            .includes(searchTerm),
    )
    const differingDimensions = fields.specs.map(
        (_, index) =>
            new Set(
                savedSkus.map(
                    (sku) => sku.attributeValues[index]?.trim() || "",
                ),
            ).size > 1,
    )
    const query = useSupplierOfferingsForSkusQuery(
        savedSkus.map((sku) => sku.skuId),
    )
    const bySku = React.useMemo(() => {
        const grouped = new Map<string, SupplierOfferingView[]>()
        for (const offering of query.data ?? []) {
            const rows = grouped.get(offering.sku_id) ?? []
            rows.push(offering)
            grouped.set(offering.sku_id, rows)
        }
        return grouped
    }, [query.data])
    const missing = savedSkus.filter(
        (sku) => !(bySku.get(sku.skuId) ?? []).some(isActive),
    ).length
    return (
        <div className="min-w-0 space-y-5">
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="space-y-1">
                    <div className="flex flex-wrap items-baseline gap-3">
                        <h2 className="text-base font-semibold">供应商供给</h2>
                        <span className="text-sm text-muted-foreground">
                            共 {savedSkus.length} 个已保存规格
                        </span>
                    </div>
                    <p className="text-sm text-muted-foreground">
                        供给单独保存，不随商品资料的「保存更新」提交。
                    </p>
                </div>
                {query.isSuccess && savedSkus.length > 0 ? (
                    <p
                        role="status"
                        className={
                            missing > 0
                                ? "text-sm text-warning-soft-foreground"
                                : "text-sm text-muted-foreground"
                        }
                    >
                        {missing > 0
                            ? `${savedSkus.length} 个规格中有 ${missing} 个缺少有效供给`
                            : `${savedSkus.length} 个规格均有有效供给`}
                    </p>
                ) : null}
            </div>
            {showSearch ? (
                <div className="flex flex-wrap items-center gap-3">
                    <Input
                        id="master-data-product-supply-search"
                        className="w-full sm:max-w-sm"
                        value={search}
                        onChange={(event) => setSearch(event.target.value)}
                        placeholder="搜索规格、SKU 名称或编号"
                        aria-label="搜索规格、SKU 名称或编号"
                    />
                    {search ? (
                        <Button
                            id="master-data-product-supply-search-clear"
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={() => setSearch("")}
                        >
                            清空搜索
                        </Button>
                    ) : null}
                    <span
                        role="status"
                        className="text-sm text-muted-foreground"
                    >
                        显示 {visibleSkus.length} / {savedSkus.length} 个规格
                    </span>
                </div>
            ) : null}
            {query.isError ? (
                <BusinessFailureState
                    title="供给信息加载失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : null}
            {visibleSkus.map((sku) => (
                <SkuSupplyGroup
                    key={sku.skuId}
                    sku={sku}
                    fields={fields}
                    productName={name}
                    differingDimensions={differingDimensions}
                    collapsible={showSearch}
                    offerings={sortOfferings(bySku.get(sku.skuId) ?? [])}
                    loading={query.isPending}
                    failed={query.isError}
                    canCreate={canCreate}
                    canViewCosts={canViewCosts}
                    unit={sku.baseUnit ?? fields.baseUnit}
                    onAdd={() =>
                        onRegisterSupply(toFixedSku(fields, sku, name))
                    }
                />
            ))}
            {searchTerm && visibleSkus.length === 0 ? (
                <p className="py-6 text-sm text-muted-foreground">
                    没有匹配的规格，请更换关键词或清空搜索。
                </p>
            ) : null}
            {unsavedCount > 0 ? (
                <p className="rounded-lg border border-dashed px-4 py-3 text-sm text-muted-foreground">
                    有 {unsavedCount}{" "}
                    个新规格尚未保存，保存商品后才能为它们添加供给。
                </p>
            ) : null}
        </div>
    )
}
