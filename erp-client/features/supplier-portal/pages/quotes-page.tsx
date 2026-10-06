"use client"
import { useMemo, useState } from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"
import { DataTable } from "@/components/business"
import { ListWorkSurface } from "@/components/business/list-workspace"
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { usePortalCatalog } from "../hooks/queries"
import type { PortalCatalogSku } from "../types"
import { PortalError, PortalSurface } from "../components/surface"
import { PortalTermsForm } from "../components/terms-form"
import { PortalImage } from "../components/portal-image"
import { PortalBatchDialog } from "../components/batch-dialog"
export function PortalQuotesPage() {
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
    const columns = useMemo<ColumnDef<PortalCatalogSku>[]>(
        () => [
            {
                id: "image",
                header: "图片",
                cell: ({ row }) => (
                    <PortalImage
                        assetId={row.original.image_asset_id}
                        source={{ sku_id: row.original.id }}
                        alt={row.original.name}
                    />
                ),
            },
            { accessorKey: "name", header: "商品名称" },
            { accessorKey: "sku_no", header: "公司 SKU 编号" },
            { accessorKey: "specification", header: "精确规格" },
            { accessorKey: "unit_name", header: "报价单位" },
            {
                id: "actions",
                header: "操作",
                enableHiding: false,
                cell: ({ row }) =>
                    row.original.own_offering_id ? (
                        <Link
                            id={`supplier-portal-catalog-own-${toAutomationIdSegment(row.original.id)}`}
                            href={`/supplier-portal/offerings/${encodeURIComponent(row.original.own_offering_id)}`}
                            className="text-primary"
                        >
                            查看自己的供给
                        </Link>
                    ) : (
                        <Button
                            id={`supplier-portal-catalog-quote-${toAutomationIdSegment(row.original.id)}`}
                            size="sm"
                            variant="outline"
                            onClick={() => setSelected(row.original)}
                        >
                            为此规格报价
                        </Button>
                    ),
            },
        ],
        [],
    )
    return (
        <PortalSurface
            title="已有商品报价"
            description="从采购向您开放的公司 SKU 中明确选择规格。公司销售价与其他供应商报价不对外开放。"
            actions={
                <Button
                    id="supplier-portal-quotes-batch"
                    variant="outline"
                    onClick={() => setBatch(true)}
                >
                    批量报价
                </Button>
            }
        >
            <ListWorkSurface
                ariaLabel="可报价目录"
                selectionBar={
                    <span className="text-sm">
                        共 {query.data?.total ?? 0} 个规格
                    </span>
                }
                toolbar={
                    <form
                        className="flex w-full max-w-xl gap-2"
                        onSubmit={(event) => {
                            event.preventDefault()
                            setQ(search.trim())
                            setPagination((p) => ({ ...p, pageIndex: 0 }))
                        }}
                    >
                        <Input
                            id="supplier-portal-catalog-search"
                            aria-label="商品或公司SKU编号"
                            value={search}
                            onChange={(event) => setSearch(event.target.value)}
                            placeholder="商品或公司 SKU 编号"
                        />
                        <Button
                            id="supplier-portal-catalog-search-submit"
                            type="submit"
                            variant="outline"
                        >
                            搜索
                        </Button>
                    </form>
                }
                table={
                    <DataTable
                        id="supplier-portal-catalog-table"
                        data={query.data?.items ?? []}
                        columns={columns}
                        getRowId={(row) => row.id}
                        rowCount={query.data?.total ?? 0}
                        pagination={pagination}
                        onPaginationChange={setPagination}
                        loading={query.isFetching}
                        errorState={
                            <PortalError
                                error={query.error}
                                retry={() => void query.refetch()}
                                id="supplier-portal-catalog-retry"
                            />
                        }
                        emptyState={
                            <p className="p-6 text-sm text-muted-foreground">
                                暂无开放的规格。未建档商品可从“新品提报”提交。
                            </p>
                        }
                    />
                }
            />
            {selected && (
                <section className="space-y-3">
                    <div className="flex items-center justify-between gap-3">
                        <h2 className="font-semibold">
                            {selected.name} · {selected.specification} ·{" "}
                            {selected.sku_no}
                        </h2>
                        <Button
                            id="supplier-portal-quote-cancel"
                            variant="ghost"
                            onClick={() => setSelected(null)}
                        >
                            取消选择
                        </Button>
                    </div>
                    <PortalTermsForm key={selected.id} sku={selected} />
                </section>
            )}
            {batch && (
                <PortalBatchDialog
                    mode="quote"
                    onClose={() => setBatch(false)}
                />
            )}
        </PortalSurface>
    )
}
