"use client"

import { useMemo, useRef, useState } from "react"
import type { ColumnDef } from "@tanstack/react-table"
import { useRouter } from "next/navigation"

import {
    BusinessStatusBadge,
    MoneyValue,
    QuantityValue,
    TableEmptyCell,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
import {
    ListDataTable,
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspacePage,
} from "@/components/business/list-workspace"
import type { StatusTone } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useSearchHotkey } from "@/lib/use-search-hotkey"
import { PortalBatchDialog } from "../components/batch-dialog"
import { PortalImage } from "../components/portal-image"
import { usePortalProfile } from "../components/portal-session"
import { usePortalOfferings } from "../hooks/queries"
import {
    availabilityLabels,
    offeringName,
    relationLabels,
    timeLabel,
} from "../lib/presentation"
import type { PortalOffering } from "../types"

const SEARCH_ID = "supplier-portal-offerings-search"

const relationTones: Record<PortalOffering["status"], StatusTone> = {
    ACTIVE: "success",
    PAUSED: "warning",
    STOPPED: "neutral",
}

const availabilityTones: Partial<Record<string, StatusTone>> = {
    AVAILABLE: "success",
    OUT_OF_STOCK: "warning",
    UNAVAILABLE: "warning",
    STOPPED: "neutral",
    STALE: "warning",
}

function offeringHref(id: string) {
    return `/supplier-portal/offerings/${encodeURIComponent(id)}`
}

export function PortalOfferingsPage() {
    const router = useRouter()
    const canMaintain = usePortalProfile()?.role === "maintainer"
    const [search, setSearch] = useState("")
    const [q, setQ] = useState("")
    const [pagination, setPagination] = useState({ pageIndex: 0, pageSize: 50 })
    const [batch, setBatch] = useState<"terms" | "availability" | null>(null)
    const searchInputRef = useRef<HTMLInputElement>(null)
    useSearchHotkey(searchInputRef)
    const query = usePortalOfferings({
        q,
        page: pagination.pageIndex + 1,
        page_size: pagination.pageSize,
    })
    const applySearch = (value: string) => {
        setQ(value.trim())
        setPagination((current) => ({ ...current, pageIndex: 0 }))
    }
    const columns = useMemo<ColumnDef<PortalOffering>[]>(
        () => [
            {
                id: "name",
                header: "商品及规格",
                accessorFn: offeringName,
                meta: { width: "flex" },
                cell: ({ row }) => {
                    const name = offeringName(row.original)
                    const specification = [
                        row.original.specification,
                        row.original.unit_name,
                    ]
                        .filter(Boolean)
                        .join(" · ")
                    return (
                        <TableIdentityCell
                            primary={name}
                            primaryTitle={name}
                            secondary={specification}
                            secondaryTitle={specification}
                        />
                    )
                },
            },
            {
                id: "image",
                header: "图片",
                meta: { width: "rate" },
                cell: ({ row }) => (
                    <PortalImage
                        assetId={row.original.image_asset_id}
                        source={{ offering_id: row.original.id }}
                        alt={offeringName(row.original)}
                        className="size-8 rounded-md object-contain"
                    />
                ),
            },
            {
                accessorKey: "supplier_sku_code",
                header: "供应商订货编码",
                meta: { width: "default" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.supplier_sku_code}
                        clamp={1}
                        numeric
                    />
                ),
            },
            {
                id: "price",
                header: "代发含税供货价",
                meta: { width: "amount", align: "end", numeric: true },
                cell: ({ row }) => (
                    <MoneyValue
                        value={
                            row.original.terms?.dropship_supply_price_gross ??
                            row.original.dropship_supply_price_gross
                        }
                        taxBasis="gross"
                        display="compact"
                    />
                ),
            },
            {
                accessorKey: "available_quantity",
                header: "可供数量",
                meta: { width: "quantity", align: "end", numeric: true },
                cell: ({ row }) =>
                    row.original.available_quantity == null ? (
                        <TableEmptyCell variant="label" label="未提供" />
                    ) : (
                        <QuantityValue
                            value={row.original.available_quantity}
                            unit={row.original.unit_name ?? ""}
                        />
                    ),
            },
            {
                accessorKey: "status",
                header: "供给关系",
                meta: { width: "status" },
                cell: ({ row }) => {
                    const { status } = row.original
                    return (
                        <BusinessStatusBadge
                            context="list"
                            label={relationLabels[status] ?? "待核对"}
                            tone={relationTones[status] ?? "warning"}
                            appearance={status === "ACTIVE" ? "quiet" : "badge"}
                        />
                    )
                },
            },
            {
                accessorKey: "availability_status",
                header: "可供情况",
                meta: { width: "status" },
                cell: ({ row }) => {
                    const status = row.original.availability_status
                    return (
                        <BusinessStatusBadge
                            context="list"
                            label={availabilityLabels[status] ?? "待核对"}
                            tone={availabilityTones[status] ?? "warning"}
                            appearance={
                                status === "AVAILABLE" ? "quiet" : "badge"
                            }
                        />
                    )
                },
            },
            {
                accessorKey: "availability_source_updated_at",
                header: "最近报送",
                meta: { width: "default" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={timeLabel(
                            row.original.availability_source_updated_at,
                        )}
                        clamp={1}
                        numeric
                    />
                ),
            },
            {
                id: "actions",
                header: "操作",
                enableHiding: false,
                meta: { width: "status", align: "end" },
                cell: ({ row }) => {
                    const { id } = row.original
                    const rowId = toAutomationIdSegment(id)
                    return (
                        <TableRowActions
                            moreId={`supplier-portal-offering-more-${rowId}`}
                            moreLabel={`${offeringName(row.original)}的更多操作`}
                            actions={[
                                {
                                    id: `supplier-portal-offering-open-${rowId}`,
                                    href: offeringHref(id),
                                    label: canMaintain
                                        ? "查看与维护"
                                        : "查看供给",
                                },
                            ]}
                        />
                    )
                },
            },
        ],
        [canMaintain],
    )
    return (
        <>
            <ListWorkspacePage
                title="供货商品目录"
                actions={
                    canMaintain
                        ? [
                              {
                                  id: "supplier-portal-batch-terms",
                                  actionKey: "batch-terms",
                                  label: "批量申请调价",
                                  tier: "quiet",
                                  onClick: () => setBatch("terms"),
                              },
                              {
                                  id: "supplier-portal-batch-availability",
                                  actionKey: "batch-availability",
                                  label: "批量更新可供",
                                  tier: "quiet",
                                  onClick: () => setBatch("availability"),
                              },
                          ]
                        : undefined
                }
                surface={{
                    ariaLabel: "供货商品目录",
                    toolbar: (
                        <ListWorkspaceFilterBar
                            idPrefix="supplier-portal-offerings-toolbar"
                            formAriaLabel="供货商品目录查询"
                            queryButtonId="supplier-portal-offerings-search-submit"
                            morePresentation="popover"
                            hasPendingChanges={search.trim() !== q}
                            resultStatus={
                                query.isFetching
                                    ? "查询中…"
                                    : query.isError
                                      ? "查询未完成"
                                      : null
                            }
                            onSubmit={() => applySearch(search)}
                            search={
                                <ListSearchField
                                    id={SEARCH_ID}
                                    searchInputRef={searchInputRef}
                                    aria-label="商品名称或订货编码"
                                    placeholder="商品名称或订货编码"
                                    value={search}
                                    onChange={setSearch}
                                />
                            }
                        />
                    ),
                    tableClassName:
                        "max-md:[&_[data-column-pinned]]:inset-x-auto! max-md:[&_[data-pinned-edge]]:shadow-none!",
                    table: (
                        <ListDataTable
                            id="supplier-portal-offerings-table"
                            entityName="供货商品"
                            caption="供货商品目录"
                            data={query.data?.items ?? []}
                            columns={columns}
                            defaultColumnVisibility={{
                                availability_source_updated_at: false,
                            }}
                            defaultColumnPinning={{
                                left: ["name"],
                                right: ["actions"],
                            }}
                            getRowId={(row) => row.id}
                            rowLabel={(row) =>
                                row.supplier_sku_code
                                    ? `${offeringName(row)}（${row.supplier_sku_code}）`
                                    : offeringName(row)
                            }
                            pagination={pagination}
                            onPaginationChange={setPagination}
                            rowCount={query.data?.total ?? 0}
                            showToolbarSummary={false}
                            loading={query.isFetching}
                            onRowOpen={(row) =>
                                router.push(offeringHref(row.id))
                            }
                            isError={query.isError}
                            error={query.error}
                            retryButtonId="supplier-portal-offerings-retry"
                            onRetry={() => void query.refetch()}
                            hasActiveFilters={q !== ""}
                            clearFiltersButtonId="supplier-portal-offerings-empty-clear-filters"
                            onClearFilters={() => {
                                setSearch("")
                                applySearch("")
                                searchInputRef.current?.focus()
                            }}
                            emptyCopy={{
                                emptyTitle: "当前暂无供货商品",
                                emptyDescription: canMaintain
                                    ? "可从「已有商品报价」或「新品提报」发起。"
                                    : undefined,
                            }}
                        />
                    ),
                }}
            />
            {canMaintain && batch && (
                <PortalBatchDialog
                    mode={batch}
                    onClose={() => setBatch(null)}
                />
            )}
        </>
    )
}
