"use client"

import * as React from "react"
import Link from "next/link"
import { useSearchParams } from "next/navigation"
import { PlusIcon, SearchIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    ListToolbar,
    PageHeader,
    PageScaffold,
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
import {
    CompanySkuSearchCombobox,
    SupplierSearchCombobox,
} from "@/features/entity-selectors"
import {
    RegisterSupplyForSkuDialog,
    ReviseOfferingDialog,
    UpdateAvailabilityDialog,
} from "@/features/supplier-offerings/offering-dialogs"
import { useSupplierOfferingsQuery } from "@/features/supplier-offerings/queries"
import type {
    OfferingStatus,
    SupplierOfferingView,
} from "@/features/supplier-offerings/types"
import {
    AVAILABILITY_STATUS_LABELS,
    OFFERING_STATUS_LABELS,
    SOURCE_TYPE_LABELS,
} from "@/features/supplier-offerings/types"

function statusVariant(status: OfferingStatus) {
    if (status === "ACTIVE") return "success" as const
    if (status === "STOPPED") return "destructive" as const
    return "secondary" as const
}

function money(value?: string | null): string {
    return value ? `¥${value}` : "—"
}

function isCurrentlyAvailable(offering: SupplierOfferingView): boolean {
    return (
        offering.availability_status === "AVAILABLE" &&
        (offering.available_quantity == null ||
            Number(offering.available_quantity) > 0)
    )
}

export function SupplierOfferingsPage() {
    const searchParams = useSearchParams()
    const skuId = searchParams.get("skuId") ?? undefined
    const returnTo = searchParams.get("returnTo")
    const [q, setQ] = React.useState("")
    const [skuFilter, setSkuFilter] = React.useState<string | undefined>()
    const [supplierId, setSupplierId] = React.useState<string | undefined>()
    const [status, setStatus] = React.useState<OfferingStatus | undefined>()
    const [page, setPage] = React.useState(1)
    const [createOpen, setCreateOpen] = React.useState(false)
    const [reviseOffering, setReviseOffering] =
        React.useState<SupplierOfferingView | null>(null)
    const [availabilityOffering, setAvailabilityOffering] =
        React.useState<SupplierOfferingView | null>(null)
    const query = useSupplierOfferingsQuery({
        q: q || undefined,
        skuId: skuId ?? skuFilter,
        supplierId,
        status,
        page,
        pageSize: 50,
    })
    const items = query.data?.items ?? []
    const activeCount = items.filter((item) => item.status === "ACTIVE").length
    const availableCount = items.filter(isCurrentlyAvailable).length
    const totalPages = Math.max(1, Math.ceil((query.data?.total ?? 0) / 50))
    const hasFilters = Boolean(q || skuFilter || supplierId || status)

    return (
        <PageScaffold density="compact">
            <PageHeader
                title={skuId ? "SKU 供给" : "供应商供给"}
                description={
                    skuId
                        ? "维护当前公司 SKU 的供应商、订货编码、商业条款与可供情况。"
                        : "每条记录直接连接一个公司 SKU 与一个供应商；不存在独立的供应商商品主档。"
                }
                actions={
                    <div className="flex items-center gap-2">
                        {returnTo ? (
                            <Button
                                type="button"
                                variant="outline"
                                render={<Link href={returnTo} />}
                            >
                                返回商品
                            </Button>
                        ) : null}
                        <Button
                            type="button"
                            onClick={() => setCreateOpen(true)}
                        >
                            <PlusIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            添加供给
                        </Button>
                    </div>
                }
            />

            <div className="grid gap-3 sm:grid-cols-3">
                <div className="rounded-lg border bg-card px-4 py-3">
                    <div className="text-xs text-muted-foreground">
                        当前结果
                    </div>
                    <div className="mt-1 text-xl font-semibold">
                        {query.data?.total ?? 0}
                    </div>
                </div>
                <div className="rounded-lg border bg-card px-4 py-3">
                    <div className="text-xs text-muted-foreground">
                        本页启用关系
                    </div>
                    <div className="mt-1 text-xl font-semibold">
                        {activeCount}
                    </div>
                </div>
                <div className="rounded-lg border bg-card px-4 py-3">
                    <div className="text-xs text-muted-foreground">
                        本页当前可供
                    </div>
                    <div className="mt-1 text-xl font-semibold">
                        {availableCount}
                    </div>
                </div>
            </div>

            <BusinessTableFrame
                title="供给关系列表"
                description="商业条款按版本追加；可供状态与数量独立更新。"
                toolbar={
                    <ListToolbar
                        search={
                            <div className="relative">
                                <SearchIcon
                                    className="pointer-events-none absolute left-2.5 top-2.5 size-4 text-muted-foreground"
                                    aria-hidden="true"
                                />
                                <Input
                                    value={q}
                                    onChange={(event) => {
                                        setQ(event.target.value)
                                        setPage(1)
                                    }}
                                    className="pl-8"
                                    placeholder="供应商 SKU 编码"
                                    aria-label="搜索供给"
                                />
                            </div>
                        }
                        filters={
                            <div className="flex flex-wrap items-center gap-1">
                                {!skuId ? (
                                    <CompanySkuSearchCombobox
                                        value={skuFilter}
                                        onValueChange={(value) => {
                                            setSkuFilter(value ?? undefined)
                                            setPage(1)
                                        }}
                                        placeholder="公司 SKU"
                                        className="w-52"
                                        aria-label="公司 SKU"
                                    />
                                ) : null}
                                <SupplierSearchCombobox
                                    value={supplierId}
                                    onValueChange={(value) => {
                                        setSupplierId(value ?? undefined)
                                        setPage(1)
                                    }}
                                    placeholder="供应商"
                                    className="w-48"
                                    aria-label="供应商"
                                />
                                <div className="flex items-center gap-1">
                                    {(
                                        [
                                            undefined,
                                            "ACTIVE",
                                            "PAUSED",
                                            "STOPPED",
                                        ] as const
                                    ).map((value) => (
                                        <Button
                                            key={value ?? "all"}
                                            type="button"
                                            size="sm"
                                            variant={
                                                status === value
                                                    ? "secondary"
                                                    : "ghost"
                                            }
                                            onClick={() => {
                                                setStatus(value)
                                                setPage(1)
                                            }}
                                        >
                                            {value
                                                ? OFFERING_STATUS_LABELS[value]
                                                : "全部"}
                                        </Button>
                                    ))}
                                </div>
                            </div>
                        }
                        actions={
                            <span className="text-xs text-muted-foreground">
                                共 {query.data?.total ?? 0} 条
                            </span>
                        }
                    />
                }
                table={
                    query.isError ? (
                        <BusinessFailureState
                            title="供给列表加载失败"
                            error={query.error}
                            onRetry={() => void query.refetch()}
                        />
                    ) : items.length === 0 && !query.isPending ? (
                        <BusinessEmptyState
                            kind={hasFilters ? "filter" : "no-data"}
                            title={hasFilters ? undefined : "还没有供应商供给"}
                            description={
                                hasFilters
                                    ? "没有符合当前筛选的供给关系。"
                                    : "先添加公司商品，再为具体 SKU 添加第一条供给。"
                            }
                            action={
                                <Button
                                    type="button"
                                    size="sm"
                                    onClick={() => setCreateOpen(true)}
                                >
                                    添加供给
                                </Button>
                            }
                        />
                    ) : (
                        <Table>
                            <TableHeader>
                                <TableRow>
                                    <TableHead>公司商品 / SKU</TableHead>
                                    <TableHead>供应商 / 订货编码</TableHead>
                                    <TableHead>供给价格</TableHead>
                                    <TableHead>起订量 / 区域</TableHead>
                                    <TableHead>当前可供</TableHead>
                                    <TableHead>关系状态</TableHead>
                                    <TableHead className="text-right">
                                        操作
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {items.map((item) => (
                                    <TableRow key={item.id}>
                                        <TableCell>
                                            <div className="font-medium">
                                                {item.sku_name ??
                                                    item.product_no ??
                                                    "公司商品"}
                                            </div>
                                            <div className="mt-1 text-xs text-muted-foreground">
                                                {item.sku_no ?? item.sku_id}
                                                {item.specification
                                                    ? ` · ${item.specification}`
                                                    : ""}
                                            </div>
                                        </TableCell>
                                        <TableCell>
                                            <div className="font-medium">
                                                {item.supplier_name ??
                                                    item.supplier_no ??
                                                    item.supplier_id}
                                            </div>
                                            <div className="mt-1 text-xs text-muted-foreground">
                                                {item.supplier_sku_code} ·{" "}
                                                {
                                                    SOURCE_TYPE_LABELS[
                                                        item.source_type
                                                    ]
                                                }
                                            </div>
                                        </TableCell>
                                        <TableCell>
                                            <div>
                                                代发{" "}
                                                {money(
                                                    item.dropship_supply_price_gross,
                                                )}
                                            </div>
                                            <div className="mt-1 text-xs text-muted-foreground">
                                                集采{" "}
                                                {money(
                                                    item.bulk_supply_price_gross,
                                                )}
                                            </div>
                                        </TableCell>
                                        <TableCell>
                                            <div>
                                                {item.bulk_minimum_order_quantity ??
                                                    "—"}
                                            </div>
                                            <div className="mt-1 max-w-48 truncate text-xs text-muted-foreground">
                                                {item.supply_region.join(
                                                    "、",
                                                ) || "—"}
                                            </div>
                                        </TableCell>
                                        <TableCell>
                                            <Badge
                                                variant={
                                                    item.availability_status ===
                                                    "AVAILABLE"
                                                        ? "success"
                                                        : "outline"
                                                }
                                            >
                                                {item.availability_status
                                                    ? AVAILABILITY_STATUS_LABELS[
                                                          item
                                                              .availability_status
                                                      ]
                                                    : "未更新"}
                                            </Badge>
                                            <div className="mt-1 text-xs text-muted-foreground">
                                                数量{" "}
                                                {item.available_quantity ??
                                                    "未提供"}
                                            </div>
                                        </TableCell>
                                        <TableCell>
                                            <Badge
                                                variant={statusVariant(
                                                    item.status,
                                                )}
                                            >
                                                {
                                                    OFFERING_STATUS_LABELS[
                                                        item.status
                                                    ]
                                                }
                                            </Badge>
                                            <div className="mt-1 text-xs text-muted-foreground">
                                                条款 v
                                                {item.current_revision_no ??
                                                    "—"}
                                            </div>
                                        </TableCell>
                                        <TableCell className="text-right">
                                            <div className="flex justify-end gap-1">
                                                <Button
                                                    type="button"
                                                    size="sm"
                                                    variant="ghost"
                                                    onClick={() =>
                                                        setAvailabilityOffering(
                                                            item,
                                                        )
                                                    }
                                                >
                                                    更新可供
                                                </Button>
                                                <Button
                                                    type="button"
                                                    size="sm"
                                                    variant="outline"
                                                    onClick={() =>
                                                        setReviseOffering(item)
                                                    }
                                                >
                                                    修订条款
                                                </Button>
                                            </div>
                                        </TableCell>
                                    </TableRow>
                                ))}
                            </TableBody>
                        </Table>
                    )
                }
                footer={
                    <>
                        <span className="text-xs text-muted-foreground">
                            第 {page} / {totalPages} 页
                        </span>
                        <div className="flex items-center gap-2">
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                disabled={page <= 1 || query.isPending}
                                onClick={() =>
                                    setPage((current) =>
                                        Math.max(1, current - 1),
                                    )
                                }
                            >
                                上一页
                            </Button>
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                disabled={page >= totalPages || query.isPending}
                                onClick={() =>
                                    setPage((current) => current + 1)
                                }
                            >
                                下一页
                            </Button>
                        </div>
                    </>
                }
            />

            <RegisterSupplyForSkuDialog
                key={createOpen ? `create-${skuId ?? "select"}` : "closed"}
                open={createOpen}
                onOpenChange={setCreateOpen}
                fixedSku={
                    skuId
                        ? {
                              skuId,
                              skuCode: items[0]?.sku_no ?? skuId,
                              skuName: items[0]?.sku_name ?? "当前公司 SKU",
                              specification:
                                  items[0]?.specification ?? "默认规格",
                              baseUnit: "",
                          }
                        : undefined
                }
            />
            {reviseOffering ? (
                <ReviseOfferingDialog
                    key={reviseOffering.id}
                    offering={reviseOffering}
                    onOpenChange={(open) => {
                        if (!open) setReviseOffering(null)
                    }}
                />
            ) : null}
            {availabilityOffering ? (
                <UpdateAvailabilityDialog
                    key={availabilityOffering.id}
                    offering={availabilityOffering}
                    onOpenChange={(open) => {
                        if (!open) setAvailabilityOffering(null)
                    }}
                />
            ) : null}
        </PageScaffold>
    )
}
