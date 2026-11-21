"use client"

import { useMemo, useState } from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
import {
    ListDataTable,
    ListWorkSurface,
    listWorkspaceFilterStatusText,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"

import { PortalBatchDialog } from "../components/batch-dialog"
import { PortalImage } from "../components/portal-image"
import { PortalListSearchForm } from "../components/portal-list-search-form"
import { usePortalProfile } from "../components/portal-session"
import { PortalSurface } from "../components/surface"
import { PortalTermsForm } from "../components/terms-form"
import { usePortalCatalog } from "../hooks/queries"
import type { PortalCatalogSku } from "../types"

const SEARCH_ID = "supplier-portal-catalog-search"

function quoteActionId(skuId: string) {
    return `supplier-portal-catalog-quote-${toAutomationIdSegment(skuId)}`
}

export function PortalQuotesPage() {
    const canMaintain = usePortalProfile()?.role === "maintainer"
    const [search, setSearch] = useState("")
    const [q, setQ] = useState("")
    const [selected, setSelected] = useState<PortalCatalogSku | null>(null)
    const [batch, setBatch] = useState(false)
    const [pagination, setPagination] = useState({ pageIndex: 0, pageSize: 50 })
    const query = usePortalCatalog({
        q,
        page: pagination.pageIndex + 1,
        page_size: pagination.pageSize,
    })
    const applySearch = (value: string) => {
        setQ(value.trim())
        setPagination((p) => ({ ...p, pageIndex: 0 }))
    }
    const columns = useMemo<ColumnDef<PortalCatalogSku>[]>(
        () => [
            {
                accessorKey: "name",
                header: "商品名称",
                meta: { width: "flex" },
                cell: ({ row: { original: sku } }) => (
                    <TableIdentityCell
                        primary={sku.name}
                        primaryTitle={sku.name}
                        secondary={sku.sku_no}
                        secondaryTitle={sku.sku_no}
                        numericSecondary
                    />
                ),
            },
            {
                id: "image",
                header: "图片",
                meta: { width: "rate" },
                cell: ({ row }) => (
                    <PortalImage
                        assetId={row.original.image_asset_id}
                        source={{ sku_id: row.original.id }}
                        alt={row.original.name}
                        className="size-control rounded-md object-contain"
                    />
                ),
            },
            {
                accessorKey: "sku_no",
                header: "公司 SKU 编号",
                meta: { width: "default", numeric: true },
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.sku_no}
                        clamp={1}
                        numeric
                    />
                ),
            },
            {
                accessorKey: "specification",
                header: "精确规格",
                meta: { width: "default" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.specification}
                        clamp={2}
                        className="whitespace-normal"
                    />
                ),
            },
            {
                accessorKey: "unit_name",
                header: "报价单位",
                meta: { width: "rate" },
                cell: ({ row }) => (
                    <TableTextCell value={row.original.unit_name} clamp={1} />
                ),
            },
            {
                id: "actions",
                header: "操作",
                enableHiding: false,
                meta: { align: "end", width: "default" },
                cell: ({ row: { original: sku } }) => {
                    const ownOfferingId = sku.own_offering_id
                    if (!ownOfferingId && !canMaintain) return null
                    const idSegment = toAutomationIdSegment(sku.id)
                    return (
                        <TableRowActions
                            moreId={`supplier-portal-catalog-more-${idSegment}`}
                            moreLabel={`${sku.name} 更多操作`}
                            actions={[
                                ownOfferingId
                                    ? {
                                          id: `supplier-portal-catalog-own-${idSegment}`,
                                          label: "查看自己的供给",
                                          href: `/supplier-portal/offerings/${encodeURIComponent(ownOfferingId)}`,
                                      }
                                    : {
                                          id: quoteActionId(sku.id),
                                          label: "为此规格报价",
                                          onClick: () => setSelected(sku),
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
        <PortalSurface
            variant="list"
            title="已有商品报价"
            actions={
                canMaintain ? (
                    <Button
                        id="supplier-portal-quotes-batch"
                        type="button"
                        variant="ghost"
                        className={listWorkspaceStyles.quietButton}
                        onClick={() => setBatch(true)}
                    >
                        批量报价
                    </Button>
                ) : undefined
            }
        >
            <ListWorkSurface
                ariaLabel="可报价目录"
                toolbar={
                    <PortalListSearchForm
                        searchId={SEARCH_ID}
                        searchLabel="商品名称或编号"
                        placeholder="商品名称或编号"
                        value={search}
                        onChange={setSearch}
                        submitId="supplier-portal-catalog-search-submit"
                        onSubmit={() => applySearch(search)}
                        hasPendingChanges={search.trim() !== q}
                        resultStatus={
                            query.isFetching || query.isError
                                ? listWorkspaceFilterStatusText({
                                      loading: query.isFetching,
                                      failed: query.isError,
                                      noun: "商品规格",
                                  })
                                : null
                        }
                    />
                }
                tableClassName="max-md:[&_[data-column-pinned]]:inset-x-auto! max-md:[&_[data-pinned-edge]]:shadow-none!"
                table={
                    <ListDataTable
                        id="supplier-portal-catalog-table"
                        entityName="商品规格"
                        caption="可报价目录"
                        data={query.data?.items ?? []}
                        columns={columns}
                        getRowId={(row) => row.id}
                        rowLabel={(row) => `${row.name}（${row.sku_no}）`}
                        defaultColumnVisibility={{ sku_no: false }}
                        defaultColumnPinning={{
                            left: ["name"],
                            right: ["actions"],
                        }}
                        rowCount={query.data?.total ?? 0}
                        pagination={pagination}
                        onPaginationChange={setPagination}
                        showToolbarSummary={false}
                        loading={query.isFetching}
                        isError={query.isError}
                        error={query.error}
                        onRetry={() => void query.refetch()}
                        retryButtonId="supplier-portal-catalog-retry"
                        hasActiveFilters={q !== ""}
                        onClearFilters={() => {
                            setSearch("")
                            applySearch("")
                            document.getElementById(SEARCH_ID)?.focus()
                        }}
                        clearFiltersButtonId="supplier-portal-catalog-empty-clear-filters"
                        emptyCopy={{
                            emptyTitle: "暂无开放询价的商品规格",
                            emptyDescription: canMaintain
                                ? "未建档商品可从「新品提报」提交。"
                                : undefined,
                        }}
                    />
                }
            />
            {canMaintain && selected && (
                <section className="space-y-3">
                    <div className="flex items-center justify-between gap-3">
                        <h2 className="font-semibold">
                            {selected.name} · {selected.specification} ·{" "}
                            {selected.sku_no}
                        </h2>
                        <Button
                            id="supplier-portal-quote-cancel"
                            variant="ghost"
                            onClick={() => {
                                setSelected(null)
                                requestAnimationFrame(() => {
                                    document
                                        .getElementById(
                                            quoteActionId(selected.id),
                                        )
                                        ?.focus()
                                })
                            }}
                        >
                            取消选择
                        </Button>
                    </div>
                    <PortalTermsForm key={selected.id} sku={selected} />
                </section>
            )}
            {canMaintain && batch && (
                <PortalBatchDialog
                    mode="quote"
                    onClose={() => setBatch(false)}
                />
            )}
        </PortalSurface>
    )
}
