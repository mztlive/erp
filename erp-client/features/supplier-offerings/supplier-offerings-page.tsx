"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { ChevronDownIcon, FilterIcon, PlusIcon, SearchIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    FilterChip,
    FixedOptionRadioFilter,
    ListToolbar,
    OptionCombobox,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
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
    AvailabilityStatus,
    OfferingSourceType,
    OfferingStatus,
    SupplierOfferingView,
} from "@/features/supplier-offerings/types"
import {
    AVAILABILITY_STATUS_LABELS,
    OFFERING_STATUS_LABELS,
    SOURCE_TYPE_LABELS,
} from "@/features/supplier-offerings/types"
import {
    buildSupplierOfferingsSearchParams,
    parseSupplierOfferingsSearchParams,
    type SupplierOfferingsUrlState,
} from "@/features/supplier-offerings/url-state"

type OfferingStatusFilter = OfferingStatus | "all"
type OfferingSourceFilter = OfferingSourceType | "all"

const OFFERING_STATUS_FILTER_OPTIONS = [
    { value: "all", label: "全部" },
    { value: "ACTIVE", label: OFFERING_STATUS_LABELS.ACTIVE },
    { value: "PAUSED", label: OFFERING_STATUS_LABELS.PAUSED },
    { value: "STOPPED", label: OFFERING_STATUS_LABELS.STOPPED },
] as const

const SOURCE_TYPE_FILTER_OPTIONS = [
    { value: "all", label: "全部" },
    { value: "MANUAL", label: SOURCE_TYPE_LABELS.MANUAL },
    { value: "EXCEL", label: SOURCE_TYPE_LABELS.EXCEL },
    { value: "API", label: SOURCE_TYPE_LABELS.API },
] as const

const AVAILABILITY_STATUS_FILTER_OPTIONS = Object.entries(
    AVAILABILITY_STATUS_LABELS,
).map(([value, label]) => ({ value, label }))

/** 返回关系状态对应的徽标样式。 */
const statusVariant = (status: OfferingStatus) => {
    if (status === "ACTIVE") return "success" as const
    if (status === "STOPPED") return "destructive" as const
    return "secondary" as const
}

/** 格式化可选金额。 */
const money = (value?: string | null): string => {
    return value ? `¥${value}` : "—"
}

/** 判断供给是否处于可用且未耗尽状态。 */
const isCurrentlyAvailable = (offering: SupplierOfferingView): boolean => {
    return (
        offering.availability_status === "AVAILABLE" &&
        (offering.available_quantity == null ||
            Number(offering.available_quantity) > 0)
    )
}

/** 供应商供给列表与维护入口。 */
export const SupplierOfferingsPage = () => {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const urlState = React.useMemo(
        () => parseSupplierOfferingsSearchParams(searchParams),
        [searchParams],
    )
    const skuLocked = Boolean(urlState.skuId && urlState.returnTo)
    const hasStructuredFilters = Boolean(
        (!skuLocked && urlState.skuId) ||
        urlState.supplierId ||
        urlState.status ||
        urlState.sourceType ||
        urlState.availabilityStatus,
    )
    const hasFilters = Boolean(urlState.q || hasStructuredFilters)
    const [searchDraft, setSearchDraft] = React.useState(urlState.q ?? "")
    const [skuIdDraft, setSkuIdDraft] = React.useState<string | null>(
        urlState.skuId ?? null,
    )
    const [supplierIdDraft, setSupplierIdDraft] = React.useState<string | null>(
        urlState.supplierId ?? null,
    )
    const [statusDraft, setStatusDraft] = React.useState<OfferingStatusFilter>(
        urlState.status ?? "all",
    )
    const [sourceTypeDraft, setSourceTypeDraft] =
        React.useState<OfferingSourceFilter>(urlState.sourceType ?? "all")
    const [availabilityStatusDraft, setAvailabilityStatusDraft] =
        React.useState<AvailabilityStatus | null>(
            urlState.availabilityStatus ?? null,
        )
    const [filterPanelOpen, setFilterPanelOpen] =
        React.useState(hasStructuredFilters)
    const [createOpen, setCreateOpen] = React.useState(false)
    const [reviseOffering, setReviseOffering] =
        React.useState<SupplierOfferingView | null>(null)
    const [availabilityOffering, setAvailabilityOffering] =
        React.useState<SupplierOfferingView | null>(null)
    const query = useSupplierOfferingsQuery({
        q: urlState.q,
        skuId: urlState.skuId,
        supplierId: urlState.supplierId,
        status: urlState.status,
        sourceType: urlState.sourceType,
        availabilityStatus: urlState.availabilityStatus,
        page: urlState.page,
        pageSize: 50,
    })
    const items = query.data?.items ?? []
    const activeCount = items.filter((item) => item.status === "ACTIVE").length
    const availableCount = items.filter(isCurrentlyAvailable).length
    const totalPages = Math.max(1, Math.ceil((query.data?.total ?? 0) / 50))

    /** 合并 URL 补丁并保留未变的导航上下文。 */
    const patchUrl = React.useCallback(
        (patch: Partial<SupplierOfferingsUrlState>) => {
            const next = { ...urlState, ...patch }
            router.replace(
                `${pathname}${buildSupplierOfferingsSearchParams(next)}`,
                { scroll: false },
            )
        },
        [pathname, router, urlState],
    )

    /** 一次提交关键词与全部结构化筛选草稿。 */
    const applyFilters = React.useCallback(() => {
        patchUrl({
            q: searchDraft.trim() || undefined,
            skuId: skuIdDraft || undefined,
            supplierId: supplierIdDraft || undefined,
            status: statusDraft === "all" ? undefined : statusDraft,
            sourceType: sourceTypeDraft === "all" ? undefined : sourceTypeDraft,
            availabilityStatus: availabilityStatusDraft ?? undefined,
            page: 1,
        })
    }, [
        availabilityStatusDraft,
        patchUrl,
        searchDraft,
        skuIdDraft,
        sourceTypeDraft,
        statusDraft,
        supplierIdDraft,
    ])

    /** 清空可编辑筛选；来源页锁定的 SKU 与返回地址保持不变。 */
    const clearFilters = React.useCallback(() => {
        setSearchDraft("")
        setSkuIdDraft(skuLocked ? (urlState.skuId ?? null) : null)
        setSupplierIdDraft(null)
        setStatusDraft("all")
        setSourceTypeDraft("all")
        setAvailabilityStatusDraft(null)
        setFilterPanelOpen(false)
        patchUrl({
            q: undefined,
            skuId: skuLocked ? urlState.skuId : undefined,
            supplierId: undefined,
            status: undefined,
            sourceType: undefined,
            availabilityStatus: undefined,
            page: 1,
        })
    }, [patchUrl, skuLocked, urlState.skuId])

    React.useEffect(() => {
        setSearchDraft(urlState.q ?? "")
        setSkuIdDraft(urlState.skuId ?? null)
        setSupplierIdDraft(urlState.supplierId ?? null)
        setStatusDraft(urlState.status ?? "all")
        setSourceTypeDraft(urlState.sourceType ?? "all")
        setAvailabilityStatusDraft(urlState.availabilityStatus ?? null)
        setFilterPanelOpen(hasStructuredFilters)
    }, [hasStructuredFilters, urlState])

    const appliedFilterLabels = [
        urlState.q ? `订货编码包含“${urlState.q}”` : null,
        !skuLocked && urlState.skuId ? "已选择公司 SKU" : null,
        urlState.supplierId ? "已选择供应商" : null,
        urlState.status
            ? `关系状态：${OFFERING_STATUS_LABELS[urlState.status]}`
            : null,
        urlState.sourceType
            ? `登记来源：${SOURCE_TYPE_LABELS[urlState.sourceType]}`
            : null,
        urlState.availabilityStatus
            ? `当前可供：${AVAILABILITY_STATUS_LABELS[urlState.availabilityStatus]}`
            : null,
    ].filter(Boolean)

    return (
        <PageScaffold density="compact">
            <PageHeader
                title={skuLocked ? "SKU 供给" : "供应商供给"}
                description={
                    skuLocked
                        ? "维护当前公司 SKU 的供应商、订货编码、商业条款与可供情况。"
                        : "每条记录直接连接一个公司 SKU 与一个供应商；不存在独立的供应商商品主档。"
                }
                actions={
                    <div className="flex items-center gap-2">
                        {urlState.returnTo ? (
                            <Button
                                type="button"
                                variant="outline"
                                render={<Link href={urlState.returnTo} />}
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
                description={
                    appliedFilterLabels.length > 0
                        ? `筛选条件：${appliedFilterLabels.join("、")}`
                        : "商业条款按版本追加；可供状态与数量独立更新。"
                }
                toolbar={
                    <form
                        onSubmit={(event) => {
                            event.preventDefault()
                            applyFilters()
                        }}
                    >
                        <ListToolbar
                            search={
                                <InputGroup className="w-full sm:w-80">
                                    <InputGroupAddon>
                                        <SearchIcon aria-hidden="true" />
                                    </InputGroupAddon>
                                    <InputGroupInput
                                        value={searchDraft}
                                        onChange={(event) =>
                                            setSearchDraft(event.target.value)
                                        }
                                        placeholder="供应商订货编码"
                                        aria-label="搜索供给"
                                    />
                                </InputGroup>
                            }
                            filters={
                                <>
                                    {!filterPanelOpen ? (
                                        <Button type="submit" size="sm">
                                            <SearchIcon
                                                data-icon="inline-start"
                                                aria-hidden="true"
                                            />
                                            搜索
                                        </Button>
                                    ) : null}
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        aria-expanded={filterPanelOpen}
                                        onClick={() =>
                                            setFilterPanelOpen((open) => !open)
                                        }
                                    >
                                        <FilterIcon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                        />
                                        高级筛选
                                        {hasStructuredFilters ? (
                                            <Badge variant="info">已启用</Badge>
                                        ) : null}
                                        <ChevronDownIcon
                                            data-icon="inline-end"
                                            aria-hidden="true"
                                            className={
                                                filterPanelOpen
                                                    ? "rotate-180 transition-transform"
                                                    : "transition-transform"
                                            }
                                        />
                                    </Button>
                                </>
                            }
                            secondary={
                                filterPanelOpen || skuLocked ? (
                                    <div className="flex w-full flex-col gap-2">
                                        {skuLocked ? (
                                            <div>
                                                <FilterChip
                                                    label={
                                                        items[0]?.sku_no
                                                            ? `公司 SKU：${items[0].sku_no}`
                                                            : "来自商品页的公司 SKU"
                                                    }
                                                    clearLabel="移除公司 SKU 限定"
                                                    onClear={() => {
                                                        setSkuIdDraft(null)
                                                        patchUrl({
                                                            skuId: undefined,
                                                            page: 1,
                                                        })
                                                    }}
                                                />
                                            </div>
                                        ) : null}
                                        {filterPanelOpen ? (
                                            <div
                                                className="flex w-full flex-col gap-3 rounded-lg border border-border/60 bg-muted/30 px-3 py-3"
                                                aria-label="供应商供给筛选条件"
                                            >
                                                <FixedOptionRadioFilter
                                                    label="关系状态"
                                                    value={statusDraft}
                                                    onValueChange={
                                                        setStatusDraft
                                                    }
                                                    options={
                                                        OFFERING_STATUS_FILTER_OPTIONS
                                                    }
                                                />
                                                <FixedOptionRadioFilter
                                                    label="登记来源"
                                                    value={sourceTypeDraft}
                                                    onValueChange={
                                                        setSourceTypeDraft
                                                    }
                                                    options={
                                                        SOURCE_TYPE_FILTER_OPTIONS
                                                    }
                                                />
                                                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
                                                    {!skuLocked ? (
                                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                                            <span className="text-muted-foreground">
                                                                公司 SKU
                                                            </span>
                                                            <CompanySkuSearchCombobox
                                                                value={
                                                                    skuIdDraft ??
                                                                    undefined
                                                                }
                                                                onValueChange={(
                                                                    value,
                                                                ) =>
                                                                    setSkuIdDraft(
                                                                        value ??
                                                                            null,
                                                                    )
                                                                }
                                                                placeholder="全部公司 SKU"
                                                                className="w-full"
                                                                aria-label="公司 SKU"
                                                            />
                                                        </div>
                                                    ) : null}
                                                    <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                                        <span className="text-muted-foreground">
                                                            供应商
                                                        </span>
                                                        <SupplierSearchCombobox
                                                            value={
                                                                supplierIdDraft ??
                                                                undefined
                                                            }
                                                            onValueChange={(
                                                                value,
                                                            ) =>
                                                                setSupplierIdDraft(
                                                                    value ??
                                                                        null,
                                                                )
                                                            }
                                                            placeholder="全部供应商"
                                                            className="w-full"
                                                            aria-label="供应商"
                                                        />
                                                    </div>
                                                    <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                                        <span className="text-muted-foreground">
                                                            当前可供
                                                        </span>
                                                        <OptionCombobox
                                                            value={
                                                                availabilityStatusDraft
                                                            }
                                                            onValueChange={(
                                                                value,
                                                            ) =>
                                                                setAvailabilityStatusDraft(
                                                                    value as AvailabilityStatus | null,
                                                                )
                                                            }
                                                            options={
                                                                AVAILABILITY_STATUS_FILTER_OPTIONS
                                                            }
                                                            placeholder="全部可供状态"
                                                            searchPlaceholder="搜索可供状态"
                                                            aria-label="当前可供状态"
                                                            className="w-full"
                                                        />
                                                    </div>
                                                </div>
                                                <div className="flex justify-end">
                                                    <Button
                                                        type="submit"
                                                        size="sm"
                                                    >
                                                        <SearchIcon
                                                            data-icon="inline-start"
                                                            aria-hidden="true"
                                                        />
                                                        搜索
                                                    </Button>
                                                </div>
                                            </div>
                                        ) : null}
                                    </div>
                                ) : undefined
                            }
                            actions={
                                <>
                                    <span className="text-xs text-muted-foreground">
                                        共 {query.data?.total ?? 0} 条
                                    </span>
                                    {hasFilters ? (
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="ghost"
                                            onClick={clearFilters}
                                        >
                                            清除筛选
                                        </Button>
                                    ) : null}
                                </>
                            }
                        />
                    </form>
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
                                hasFilters ? (
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        onClick={clearFilters}
                                    >
                                        清除筛选
                                    </Button>
                                ) : (
                                    <Button
                                        type="button"
                                        size="sm"
                                        onClick={() => setCreateOpen(true)}
                                    >
                                        添加供给
                                    </Button>
                                )
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
                            第 {urlState.page} / {totalPages} 页
                        </span>
                        <div className="flex items-center gap-2">
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                disabled={urlState.page <= 1 || query.isPending}
                                onClick={() =>
                                    patchUrl({
                                        page: Math.max(1, urlState.page - 1),
                                    })
                                }
                            >
                                上一页
                            </Button>
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                disabled={
                                    urlState.page >= totalPages ||
                                    query.isPending
                                }
                                onClick={() =>
                                    patchUrl({ page: urlState.page + 1 })
                                }
                            >
                                下一页
                            </Button>
                        </div>
                    </>
                }
            />

            <RegisterSupplyForSkuDialog
                key={
                    createOpen
                        ? `create-${urlState.skuId ?? "select"}`
                        : "closed"
                }
                open={createOpen}
                onOpenChange={setCreateOpen}
                fixedSku={
                    urlState.skuId
                        ? {
                              skuId: urlState.skuId,
                              skuCode: items[0]?.sku_no ?? "当前公司 SKU",
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
