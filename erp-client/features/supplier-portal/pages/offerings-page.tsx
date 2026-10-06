"use client"
import { useMemo, useState } from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"
import { useRouter } from "next/navigation"
import { DataTable, MoneyValue, QuantityValue } from "@/components/business"
import { ListWorkSurface } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { usePortalOfferings } from "../hooks/queries"
import {
    availabilityLabels,
    offeringName,
    relationLabels,
    timeLabel,
} from "../lib/presentation"
import type { PortalOffering } from "../types"
import { PortalError, PortalSurface } from "../components/surface"
import { PortalImage } from "../components/portal-image"
import { PortalBatchDialog } from "../components/batch-dialog"
export function PortalOfferingsPage() {
    const router = useRouter()
    const [search, setSearch] = useState("")
    const [q, setQ] = useState("")
    const [pagination, setPagination] = useState({ pageIndex: 0, pageSize: 50 })
    const [batch, setBatch] = useState<"terms" | "availability" | null>(null)
    const query = usePortalOfferings({
        q,
        page: pagination.pageIndex + 1,
        page_size: pagination.pageSize,
    })
    const columns = useMemo<ColumnDef<PortalOffering>[]>(
        () => [
            {
                id: "image",
                header: "图片",
                cell: ({ row }) => (
                    <PortalImage
                        assetId={row.original.image_asset_id}
                        source={{ offering_id: row.original.id }}
                        alt={offeringName(row.original)}
                    />
                ),
            },
            {
                id: "name",
                header: "商品及规格",
                accessorFn: offeringName,
                cell: ({ row }) => (
                    <div>
                        <p>{offeringName(row.original)}</p>
                        <p className="text-xs text-muted-foreground">
                            {row.original.specification} ·{" "}
                            {row.original.unit_name}
                        </p>
                    </div>
                ),
            },
            { accessorKey: "supplier_sku_code", header: "供应商订货编码" },
            {
                id: "price",
                header: "代发含税供货价",
                cell: ({ row }) => (
                    <MoneyValue
                        value={
                            row.original.terms?.dropship_supply_price_gross ??
                            row.original.dropship_supply_price_gross
                        }
                        taxBasis="gross"
                    />
                ),
            },
            {
                accessorKey: "available_quantity",
                header: "可供数量",
                cell: ({ row }) =>
                    row.original.available_quantity == null ? (
                        "未提供"
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
                cell: ({ row }) =>
                    relationLabels[row.original.status] ?? "待核对",
            },
            {
                accessorKey: "availability_status",
                header: "可供情况",
                cell: ({ row }) =>
                    availabilityLabels[row.original.availability_status] ??
                    "待核对",
            },
            {
                accessorKey: "availability_source_updated_at",
                header: "最近报送",
                cell: ({ row }) =>
                    timeLabel(row.original.availability_source_updated_at),
            },
            {
                id: "actions",
                header: "操作",
                enableHiding: false,
                cell: ({ row }) => (
                    <Link
                        id={`supplier-portal-offering-open-${toAutomationIdSegment(row.original.id)}`}
                        href={`/supplier-portal/offerings/${encodeURIComponent(row.original.id)}`}
                        className="text-primary hover:underline"
                    >
                        查看与维护
                    </Link>
                ),
            },
        ],
        [],
    )
    return (
        <PortalSurface
            title="我的供给"
            description="分别查看当前生效条款、可供情况与待采购确认的变更。"
            actions={
                <>
                    <Button
                        id="supplier-portal-batch-terms"
                        variant="outline"
                        onClick={() => setBatch("terms")}
                    >
                        批量申请调价
                    </Button>
                    <Button
                        id="supplier-portal-batch-availability"
                        variant="outline"
                        onClick={() => setBatch("availability")}
                    >
                        批量更新可供
                    </Button>
                </>
            }
        >
            <ListWorkSurface
                ariaLabel="我的供给"
                selectionBar={
                    <span className="text-sm">
                        共 {query.data?.total ?? 0} 条供给
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
                            id="supplier-portal-offerings-search"
                            aria-label="商品名称或订货编码"
                            placeholder="商品名称或订货编码"
                            value={search}
                            onChange={(event) => setSearch(event.target.value)}
                        />
                        <Button
                            id="supplier-portal-offerings-search-submit"
                            type="submit"
                            variant="outline"
                        >
                            搜索
                        </Button>
                    </form>
                }
                table={
                    <DataTable
                        id="supplier-portal-offerings-table"
                        data={query.data?.items ?? []}
                        columns={columns}
                        getRowId={(row) => row.id}
                        pagination={pagination}
                        onPaginationChange={setPagination}
                        rowCount={query.data?.total ?? 0}
                        loading={query.isFetching}
                        onRowOpen={(row) =>
                            router.push(
                                `/supplier-portal/offerings/${encodeURIComponent(row.id)}`,
                            )
                        }
                        errorState={
                            <PortalError
                                error={query.error}
                                retry={() => void query.refetch()}
                                id="supplier-portal-offerings-retry"
                            />
                        }
                        emptyState={
                            <p className="p-6 text-sm text-muted-foreground">
                                当前没有可查看的供给，可从“已有商品报价”或“新品提报”开始。
                            </p>
                        }
                    />
                }
            />
            {batch && (
                <PortalBatchDialog
                    mode={batch}
                    onClose={() => setBatch(null)}
                />
            )}
        </PortalSurface>
    )
}
