"use client"

import * as React from "react"
import Link from "next/link"
import { ArrowUpRightIcon, PlusIcon, SearchIcon } from "lucide-react"

import {
    BusinessFailureState,
    DetailInlineTable,
    MoneyValue,
    QuantityValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { StatusBadge } from "@/components/ui/status-badge"
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
import {
    compareDecimal,
    formatCurrencyFixed,
    subtractFixed,
} from "@/lib/fixed-decimal"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"

/** 有效供给关系：启用且已有条款版本，不代表供应商当前可供。 */
export function isActiveOffering(offering: SupplierOfferingView): boolean {
    return offering.status === "ACTIVE" && Boolean(offering.current_revision_id)
}

function sortOfferings(
    offerings: readonly SupplierOfferingView[],
): SupplierOfferingView[] {
    return [...offerings].sort(
        (left, right) =>
            Number(isActiveOffering(right)) - Number(isActiveOffering(left)) ||
            (left.supplier_name ?? "").localeCompare(
                right.supplier_name ?? "",
                "zh-CN",
            ),
    )
}

function computeSpread(
    referencePrice: string | undefined,
    supplyPrice: string | null | undefined,
): { label: string; negative: boolean } | null {
    if (!referencePrice?.trim() || !supplyPrice?.trim()) return null
    try {
        const diff = subtractFixed(referencePrice, supplyPrice, {
            maxScale: 4,
            outputScale: 2,
        })
        const sign = compareDecimal(diff, "0", 2)
        if (sign < 0) {
            return {
                label: `倒挂 ${formatCurrencyFixed(diff, { maxScale: 2 })}`,
                negative: true,
            }
        }
        return {
            label: `差价 +${formatCurrencyFixed(diff, { maxScale: 2 })}`,
            negative: false,
        }
    } catch {
        return null
    }
}

function SkuReferencePricesStrip({
    sku,
    unit,
}: {
    sku: ProductSkuFields
    unit: string
}) {
    const hasAny = Boolean(
        sku.salePrice || sku.bulkPriceGross || sku.marketPrice,
    )
    if (!hasAny) {
        return (
            <span className="text-xs text-muted-foreground/80">
                本规格暂未填写参考售价
            </span>
        )
    }
    return (
        <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1 text-xs text-muted-foreground">
            <span className="text-muted-foreground/75">对照售价</span>
            {sku.salePrice ? (
                <span className="inline-flex items-baseline gap-1">
                    <span>代发</span>
                    <span className="text-foreground/90">
                        <MoneyValue
                            value={sku.salePrice}
                            display="exact"
                            inheritColor
                        />
                    </span>
                </span>
            ) : null}
            {sku.bulkPriceGross ? (
                <>
                    {sku.salePrice ? (
                        <span
                            aria-hidden="true"
                            className="text-muted-foreground/40"
                        >
                            ·
                        </span>
                    ) : null}
                    <span className="inline-flex items-baseline gap-1">
                        <span>集采</span>
                        <span className="text-foreground/90">
                            <MoneyValue
                                value={sku.bulkPriceGross}
                                display="exact"
                                inheritColor
                            />
                        </span>
                        {sku.bulkMinQuantity ? (
                            <span className="num text-muted-foreground">
                                （起订 {sku.bulkMinQuantity}
                                {unit ? ` ${unit}` : ""}）
                            </span>
                        ) : null}
                    </span>
                </>
            ) : null}
            {sku.marketPrice ? (
                <>
                    {sku.salePrice || sku.bulkPriceGross ? (
                        <span
                            aria-hidden="true"
                            className="text-muted-foreground/40"
                        >
                            ·
                        </span>
                    ) : null}
                    <span className="inline-flex items-baseline gap-1">
                        <span>市场</span>
                        <span className="text-foreground/80">
                            <MoneyValue
                                value={sku.marketPrice}
                                display="exact"
                                inheritColor
                            />
                        </span>
                    </span>
                </>
            ) : null}
        </div>
    )
}

function OfferingRowsTable({
    sku,
    offerings,
    canViewCosts,
    canDetail,
    unit,
}: {
    sku: ProductSkuFields
    offerings: readonly SupplierOfferingView[]
    canViewCosts: boolean
    canDetail: boolean
    unit: string
}) {
    const colWidths = canViewCosts
        ? canDetail
            ? {
                  supplier: "w-[26%]",
                  dropship: "w-[14%]",
                  bulk: "w-[14%]",
                  availability: "w-[15%]",
                  template: "w-[15%]",
                  status: "w-[9%]",
                  action: "w-[7%]",
              }
            : {
                  supplier: "w-[28%]",
                  dropship: "w-[15%]",
                  bulk: "w-[15%]",
                  availability: "w-[16%]",
                  template: "w-[16%]",
                  status: "w-[10%]",
                  action: "",
              }
        : canDetail
          ? {
                supplier: "w-[36%]",
                dropship: "",
                bulk: "",
                availability: "w-[22%]",
                template: "w-[22%]",
                status: "w-[12%]",
                action: "w-[8%]",
            }
          : {
                supplier: "w-[38%]",
                dropship: "",
                bulk: "",
                availability: "w-[24%]",
                template: "w-[24%]",
                status: "w-[14%]",
                action: "",
            }

    return (
        <DetailInlineTable>
            <Table
                data-density="comfortable"
                className="min-w-[56rem] table-fixed [&_thead_th]:!static"
            >
                <colgroup>
                    <col className={colWidths.supplier} />
                    {canViewCosts ? (
                        <>
                            <col className={colWidths.dropship} />
                            <col className={colWidths.bulk} />
                        </>
                    ) : null}
                    <col className={colWidths.availability} />
                    <col className={colWidths.template} />
                    <col className={colWidths.status} />
                    {canDetail ? <col className={colWidths.action} /> : null}
                </colgroup>
                <TableHeader>
                    <TableRow>
                        <TableHead>供应商 / 订货编码</TableHead>
                        {canViewCosts ? (
                            <>
                                <TableHead data-align="end">
                                    代发供货价
                                </TableHead>
                                <TableHead data-align="end">
                                    集采供货价
                                </TableHead>
                            </>
                        ) : null}
                        <TableHead>当前可供</TableHead>
                        <TableHead>配送模板</TableHead>
                        <TableHead>关系状态</TableHead>
                        {canDetail ? (
                            <TableHead data-align="end">
                                <span className="sr-only">操作</span>
                            </TableHead>
                        ) : null}
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {offerings.map((offering) => {
                        const dropshipSpread = canViewCosts
                            ? computeSpread(
                                  sku.salePrice,
                                  offering.dropship_supply_price_gross,
                              )
                            : null
                        const bulkSpread = canViewCosts
                            ? computeSpread(
                                  sku.bulkPriceGross,
                                  offering.bulk_supply_price_gross,
                              )
                            : null

                        return (
                            <TableRow key={offering.id}>
                                <TableCell className="whitespace-normal wrap-anywhere">
                                    <div className="font-medium text-foreground">
                                        {offering.supplier_name ??
                                            offering.supplier_no ??
                                            "供应商名称未返回"}
                                    </div>
                                    <div className="num mt-0.5 text-xs text-muted-foreground">
                                        {offering.supplier_sku_code}
                                    </div>
                                </TableCell>
                                {canViewCosts ? (
                                    <>
                                        <TableCell
                                            data-align="end"
                                            className="whitespace-normal"
                                        >
                                            <MoneyValue
                                                value={
                                                    offering.dropship_supply_price_gross
                                                }
                                                display="exact"
                                            />
                                            {dropshipSpread ? (
                                                <div
                                                    className={cn(
                                                        "num mt-0.5 text-xs",
                                                        dropshipSpread.negative
                                                            ? "font-medium text-destructive"
                                                            : "text-success-soft-foreground",
                                                    )}
                                                >
                                                    {dropshipSpread.label}
                                                </div>
                                            ) : null}
                                        </TableCell>
                                        <TableCell
                                            data-align="end"
                                            className="whitespace-normal"
                                        >
                                            <MoneyValue
                                                value={
                                                    offering.bulk_supply_price_gross
                                                }
                                                display="exact"
                                            />
                                            {offering.bulk_minimum_order_quantity ||
                                            bulkSpread ? (
                                                <div className="mt-0.5 flex flex-wrap items-center justify-end gap-x-1.5 text-xs text-muted-foreground">
                                                    {offering.bulk_minimum_order_quantity ? (
                                                        <span>
                                                            起订{" "}
                                                            <QuantityValue
                                                                value={
                                                                    offering.bulk_minimum_order_quantity
                                                                }
                                                                unit={unit}
                                                            />
                                                        </span>
                                                    ) : null}
                                                    {bulkSpread ? (
                                                        <span
                                                            className={cn(
                                                                "num",
                                                                bulkSpread.negative
                                                                    ? "font-medium text-destructive"
                                                                    : "text-success-soft-foreground",
                                                            )}
                                                        >
                                                            {bulkSpread.label}
                                                        </span>
                                                    ) : null}
                                                </div>
                                            ) : null}
                                        </TableCell>
                                    </>
                                ) : null}
                                <TableCell>
                                    <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                                        <StatusBadge
                                            tone={
                                                offering.availability_status ===
                                                "AVAILABLE"
                                                    ? "success"
                                                    : "neutral"
                                            }
                                            appearance={
                                                offering.availability_status ===
                                                "AVAILABLE"
                                                    ? "quiet"
                                                    : "badge"
                                            }
                                            label={
                                                offering.availability_status
                                                    ? AVAILABILITY_STATUS_LABELS[
                                                          offering
                                                              .availability_status
                                                      ]
                                                    : "未更新"
                                            }
                                        />
                                        <span className="text-xs text-muted-foreground">
                                            {offering.available_quantity !=
                                            null ? (
                                                <QuantityValue
                                                    value={
                                                        offering.available_quantity
                                                    }
                                                    unit={unit}
                                                />
                                            ) : (
                                                "数量未提供"
                                            )}
                                        </span>
                                    </div>
                                </TableCell>
                                <TableCell className="whitespace-normal wrap-anywhere">
                                    {offering.delivery_template
                                        ?.display_name ?? (
                                        <span className="text-muted-foreground/60">
                                            —
                                        </span>
                                    )}
                                </TableCell>
                                <TableCell>
                                    <StatusBadge
                                        tone={
                                            offering.status === "ACTIVE"
                                                ? "success"
                                                : "neutral"
                                        }
                                        appearance={
                                            offering.status === "ACTIVE"
                                                ? "quiet"
                                                : "badge"
                                        }
                                        label={
                                            OFFERING_STATUS_LABELS[
                                                offering.status
                                            ]
                                        }
                                    />
                                </TableCell>
                                {canDetail ? (
                                    <TableCell data-align="end">
                                        <Button
                                            id={`master-data-product-supply-open-${toAutomationIdSegment(offering.id)}`}
                                            variant="ghost"
                                            size="sm"
                                            className="h-7 px-2 text-body-compact font-normal text-muted-foreground hover:bg-sidebar-accent/70 hover:text-primary group-hover/row:text-primary"
                                            render={
                                                <Link
                                                    href={`/procurement/supplier-offerings/${encodeURIComponent(offering.id)}`}
                                                />
                                            }
                                        >
                                            查看
                                            <ArrowUpRightIcon
                                                data-icon="inline-end"
                                                className="size-3.5"
                                                aria-hidden
                                            />
                                        </Button>
                                    </TableCell>
                                ) : null}
                            </TableRow>
                        )
                    })}
                </TableBody>
            </Table>
        </DetailInlineTable>
    )
}

function SkuSupplyWorkspace({
    sku,
    offerings,
    loading,
    failed,
    canCreate,
    canViewCosts,
    canDetail,
    unit,
    showSkuHeading,
    onAdd,
}: {
    sku: ProductSkuFields & { skuId: string }
    offerings: readonly SupplierOfferingView[]
    loading: boolean
    failed: boolean
    canCreate: boolean
    canViewCosts: boolean
    canDetail: boolean
    unit: string
    showSkuHeading: boolean
    onAdd: () => void
}) {
    const segment = toAutomationIdSegment(sku.skuId)
    const activeSuppliers = new Set(
        offerings.filter(isActiveOffering).map((item) => item.supplier_id),
    ).size
    const specTitle = sku.specLabel.trim() || "默认规格"

    return (
        <div className="min-w-0 space-y-4">
            <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-2.5">
                <div className="min-w-0 space-y-1.5">
                    <div className="flex flex-wrap items-center gap-2">
                        {showSkuHeading ? (
                            <>
                                <h3 className="text-section-title font-semibold tracking-tight text-foreground">
                                    {specTitle}
                                </h3>
                                {sku.skuNo ? (
                                    <span className="num text-xs text-muted-foreground">
                                        {sku.skuNo}
                                    </span>
                                ) : null}
                            </>
                        ) : (
                            <h2 className={detailWorkspaceStyles.sectionTitle}>
                                供应商供给
                            </h2>
                        )}
                        {!loading && !failed ? (
                            <StatusBadge
                                tone={
                                    activeSuppliers > 0 ? "success" : "warning"
                                }
                                appearance={
                                    activeSuppliers > 0 ? "quiet" : "badge"
                                }
                                label={
                                    activeSuppliers > 0
                                        ? `有效供给 ${activeSuppliers} 家`
                                        : offerings.length === 0
                                          ? "尚未添加供给"
                                          : "暂无有效供给"
                                }
                            />
                        ) : null}
                    </div>
                    <SkuReferencePricesStrip sku={sku} unit={unit} />
                </div>

                <div className="flex shrink-0 items-center gap-3">
                    {!showSkuHeading ? (
                        <span className="hidden text-xs text-muted-foreground/80 md:inline">
                            供给实时独立保存
                        </span>
                    ) : null}
                    {canCreate ? (
                        <Button
                            id={`master-data-product-sku-${segment}-add-supply`}
                            type="button"
                            size="sm"
                            variant="default"
                            className="h-control px-3.5 shadow-none"
                            onClick={onAdd}
                        >
                            <PlusIcon
                                data-icon="inline-start"
                                className="size-3.5"
                                aria-hidden
                            />
                            添加供给
                        </Button>
                    ) : null}
                </div>
            </div>

            {loading ? (
                <div className="space-y-2.5 border-t border-border/60 py-5">
                    <div className="h-9 animate-pulse rounded bg-muted/70" />
                    <div className="h-9 animate-pulse rounded bg-muted/45" />
                </div>
            ) : failed ? (
                <p className="border-t border-border/60 py-6 text-body-compact text-muted-foreground">
                    供给信息读取失败，请在上方点击重试。
                </p>
            ) : offerings.length === 0 ? (
                <div className="flex flex-col items-center justify-center gap-2 border-y border-border/60 py-12 text-center">
                    <p className="text-body-compact font-medium text-foreground/85">
                        当前规格尚未配置供应商供给
                    </p>
                    <p className="max-w-md text-xs text-muted-foreground">
                        没有有效供给关系的规格无法进入公司商品池销售。
                    </p>
                    {canCreate ? (
                        <Button
                            id={`master-data-product-sku-${segment}-add-supply-empty`}
                            type="button"
                            variant="secondary"
                            size="sm"
                            className="mt-1.5 rounded-lg shadow-none"
                            onClick={onAdd}
                        >
                            <PlusIcon
                                data-icon="inline-start"
                                className="size-3.5"
                                aria-hidden
                            />
                            添加第一家供给
                        </Button>
                    ) : null}
                </div>
            ) : (
                <OfferingRowsTable
                    sku={sku}
                    offerings={offerings}
                    canViewCosts={canViewCosts}
                    canDetail={canDetail}
                    unit={unit}
                />
            )}
        </div>
    )
}

/**
 * 商品资料「供应商供给」分区：
 * - 单 SKU：直接铺开纯白通栏表 + 售价差额对照，零卡片套娃；
 * - 多 SKU：左侧规格目录（搜索 / 仅看缺供给）+ 右侧纯白通栏供给表。
 */
export function ProductSupplySection({
    fields,
    name,
    selectedSkuId,
    onSelectSkuId,
    onRegisterSupply,
}: {
    fields: ProductFields
    name: string
    selectedSkuId?: string
    onSelectSkuId?: (skuId: string) => void
    onRegisterSupply: (sku: FixedSku) => void
}) {
    const [search, setSearch] = React.useState("")
    const [onlyMissing, setOnlyMissing] = React.useState(false)
    const [internalSkuId, setInternalSkuId] = React.useState<string>()

    const account = useAccountProfileQuery()
    const permissions = account.data?.permissions
    const canCreate = hasPermission(permissions, "supplier_offering:create")
    const canDetail = hasPermission(permissions, "supplier_offering:detail")
    const canViewCosts = hasPermission(
        permissions,
        "supplier_offering_cost:detail",
    )

    const savedSkus = React.useMemo(
        () =>
            fields.skus.filter(
                (sku): sku is ProductSkuFields & { skuId: string } =>
                    Boolean(sku.skuId),
            ),
        [fields.skus],
    )
    const unsavedCount = fields.skus.length - savedSkus.length

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

    const activeSupplierCount = React.useCallback(
        (skuId: string) =>
            new Set(
                (bySku.get(skuId) ?? [])
                    .filter(isActiveOffering)
                    .map((item) => item.supplier_id),
            ).size,
        [bySku],
    )

    const missingCount = savedSkus.filter(
        (sku) => activeSupplierCount(sku.skuId) === 0,
    ).length

    const searchTerm = search.trim().toLocaleLowerCase()
    const filteredSkus = savedSkus.filter((sku) => {
        if (onlyMissing && activeSupplierCount(sku.skuId) > 0) return false
        if (!searchTerm) return true
        return [sku.skuNo, sku.name, sku.specLabel, ...sku.attributeValues]
            .join(" ")
            .toLocaleLowerCase()
            .includes(searchTerm)
    })

    const requestedSkuId = selectedSkuId ?? internalSkuId
    const activeSku =
        filteredSkus.find((sku) => sku.skuId === requestedSkuId) ??
        filteredSkus[0] ??
        savedSkus[0]

    const selectSku = (skuId: string) => {
        setInternalSkuId(skuId)
        onSelectSkuId?.(skuId)
    }

    if (savedSkus.length === 0) {
        return (
            <div className="space-y-2 py-8 text-center">
                <p className="text-body-compact font-medium text-foreground/85">
                    暂无已保存的规格
                </p>
                <p className="text-xs text-muted-foreground">
                    请先保存商品资料生成 SKU 后，再为对应规格添加供应商供给。
                </p>
            </div>
        )
    }

    // 单 SKU：极净通栏单表，零外层卡片壳
    if (savedSkus.length === 1) {
        const sku = savedSkus[0]
        return (
            <div className="min-w-0 space-y-4">
                {query.isError ? (
                    <BusinessFailureState
                        title="供给信息加载失败"
                        error={query.error}
                        onRetry={() => void query.refetch()}
                    />
                ) : null}
                <SkuSupplyWorkspace
                    sku={sku}
                    offerings={sortOfferings(bySku.get(sku.skuId) ?? [])}
                    loading={query.isPending}
                    failed={query.isError}
                    canCreate={canCreate}
                    canViewCosts={canViewCosts}
                    canDetail={canDetail}
                    unit={sku.baseUnit ?? fields.baseUnit}
                    showSkuHeading={false}
                    onAdd={() =>
                        onRegisterSupply(toFixedSku(fields, sku, name))
                    }
                />
                {unsavedCount > 0 ? (
                    <p className="text-xs text-muted-foreground">
                        另有 {unsavedCount}{" "}
                        个新规格尚未保存，保存商品后才能为它们添加供给。
                    </p>
                ) : null}
            </div>
        )
    }

    // 多 SKU：左侧规格目录 + 右侧纯白通栏供给表
    return (
        <div className="min-w-0 space-y-4">
            <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border/60 pb-3.5">
                <div className="flex flex-wrap items-center gap-2.5">
                    <h2 className={detailWorkspaceStyles.sectionTitle}>
                        供应商供给
                    </h2>
                    <span className="num rounded-sm bg-muted/80 px-1.5 py-0.5 text-xs font-medium text-muted-foreground">
                        {savedSkus.length} 个规格
                    </span>
                    {query.isSuccess ? (
                        <StatusBadge
                            tone={missingCount > 0 ? "warning" : "success"}
                            appearance={missingCount > 0 ? "badge" : "quiet"}
                            label={
                                missingCount > 0
                                    ? `${missingCount} 个规格缺有效供给`
                                    : "全部规格已有有效供给"
                            }
                        />
                    ) : null}
                </div>
                <span className="text-xs text-muted-foreground/80">
                    供给实时独立保存，不随「保存更新」提交
                </span>
            </div>

            {query.isError ? (
                <BusinessFailureState
                    title="供给信息加载失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : null}

            <div className="grid min-w-0 items-start gap-6 lg:grid-cols-[260px_minmax(0,1fr)]">
                {/* 左侧规格目录 */}
                <aside
                    aria-label="规格目录"
                    className="min-w-0 space-y-2.5 lg:border-r lg:border-border/60 lg:pr-5"
                >
                    <div className="flex items-center gap-1.5">
                        <button
                            id="master-data-product-supply-filter-all"
                            type="button"
                            aria-pressed={!onlyMissing}
                            onClick={() => setOnlyMissing(false)}
                            className={cn(
                                "h-7 flex-1 rounded-md px-2 text-xs font-medium transition-colors",
                                !onlyMissing
                                    ? "bg-sidebar-accent text-sidebar-accent-foreground"
                                    : "bg-muted/55 text-muted-foreground hover:bg-muted hover:text-foreground",
                            )}
                        >
                            全部{" "}
                            <span className="num opacity-80">
                                {savedSkus.length}
                            </span>
                        </button>
                        <button
                            id="master-data-product-supply-filter-missing"
                            type="button"
                            aria-pressed={onlyMissing}
                            onClick={() => setOnlyMissing(true)}
                            className={cn(
                                "h-7 flex-1 rounded-md px-2 text-xs font-medium transition-colors",
                                onlyMissing
                                    ? "bg-warning-soft text-warning-soft-foreground"
                                    : "bg-muted/55 text-muted-foreground hover:bg-muted hover:text-foreground",
                            )}
                        >
                            缺供给{" "}
                            <span className="num opacity-80">
                                {missingCount}
                            </span>
                        </button>
                    </div>

                    {savedSkus.length > 4 ? (
                        <div className="relative">
                            <SearchIcon
                                className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground/75"
                                aria-hidden
                            />
                            <Input
                                id="master-data-product-supply-search"
                                size="sm"
                                value={search}
                                onChange={(event) =>
                                    setSearch(event.target.value)
                                }
                                placeholder="筛选规格或 SKU 编码"
                                aria-label="筛选规格或 SKU 编码"
                                className="border-transparent bg-muted/65 pl-8 shadow-none hover:bg-muted focus-visible:border-primary/60 focus-visible:bg-card"
                            />
                        </div>
                    ) : null}

                    <nav
                        aria-label="选择规格查看供给"
                        className="max-h-[60dvh] space-y-1 overflow-y-auto overscroll-contain pr-0.5"
                    >
                        {filteredSkus.length === 0 ? (
                            <p className="px-2 py-6 text-center text-xs text-muted-foreground">
                                无匹配规格
                            </p>
                        ) : (
                            filteredSkus.map((sku) => {
                                const selected = activeSku?.skuId === sku.skuId
                                const count = activeSupplierCount(sku.skuId)
                                const seg = toAutomationIdSegment(sku.skuId)
                                return (
                                    <button
                                        key={sku.skuId}
                                        id={`master-data-product-supply-sku-nav-${seg}`}
                                        type="button"
                                        aria-current={
                                            selected ? "true" : undefined
                                        }
                                        onClick={() => selectSku(sku.skuId)}
                                        className={cn(
                                            "flex w-full items-center justify-between gap-2 rounded-md px-2.5 py-2 text-left transition-colors",
                                            selected
                                                ? "bg-sidebar-accent/80 text-sidebar-accent-foreground"
                                                : "text-foreground/85 hover:bg-muted/65",
                                        )}
                                    >
                                        <div className="min-w-0 flex-1">
                                            <p className="truncate text-body-compact font-medium">
                                                {sku.specLabel.trim() ||
                                                    sku.name ||
                                                    "默认规格"}
                                            </p>
                                            {sku.skuNo ? (
                                                <p
                                                    className={cn(
                                                        "num truncate text-xs",
                                                        selected
                                                            ? "text-sidebar-accent-foreground/75"
                                                            : "text-muted-foreground",
                                                    )}
                                                >
                                                    {sku.skuNo}
                                                </p>
                                            ) : null}
                                        </div>
                                        {query.isPending ? null : count > 0 ? (
                                            <span
                                                className={cn(
                                                    "num shrink-0 rounded-sm px-1.5 py-0.5 text-tiny font-medium",
                                                    selected
                                                        ? "bg-primary/12 text-primary"
                                                        : "bg-muted text-muted-foreground",
                                                )}
                                            >
                                                {count} 家
                                            </span>
                                        ) : (
                                            <span className="shrink-0 rounded-sm bg-warning-soft px-1.5 py-0.5 text-tiny font-medium text-warning-soft-foreground">
                                                缺供给
                                            </span>
                                        )}
                                    </button>
                                )
                            })
                        )}
                    </nav>
                </aside>

                {/* 右侧选中 SKU 的供给工作区 */}
                <div className="min-w-0">
                    {activeSku ? (
                        <SkuSupplyWorkspace
                            sku={activeSku}
                            offerings={sortOfferings(
                                bySku.get(activeSku.skuId) ?? [],
                            )}
                            loading={query.isPending}
                            failed={query.isError}
                            canCreate={canCreate}
                            canViewCosts={canViewCosts}
                            canDetail={canDetail}
                            unit={activeSku.baseUnit ?? fields.baseUnit}
                            showSkuHeading
                            onAdd={() =>
                                onRegisterSupply(
                                    toFixedSku(fields, activeSku, name),
                                )
                            }
                        />
                    ) : null}
                </div>
            </div>

            {unsavedCount > 0 ? (
                <p className="border-t border-border/60 pt-3 text-xs text-muted-foreground">
                    另有 {unsavedCount}{" "}
                    个新规格尚未保存，保存商品后才能为它们添加供给。
                </p>
            ) : null}
        </div>
    )
}
