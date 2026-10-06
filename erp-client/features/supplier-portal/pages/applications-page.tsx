"use client"
import { useMemo, useState } from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"
import { useRouter } from "next/navigation"
import { DataTable } from "@/components/business"
import { ListWorkSurface } from "@/components/business/list-workspace"
import { OptionCombobox } from "@/components/business/option-combobox"
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { usePortalApplications } from "../hooks/queries"
import { kindLabels, statusLabels, timeLabel } from "../lib/presentation"
import type { PortalApplication } from "../types"
import { PortalError, PortalSurface } from "../components/surface"
export function PortalApplicationsPage() {
    const router = useRouter()
    const [search, setSearch] = useState("")
    const [q, setQ] = useState("")
    const [status, setStatus] = useState("")
    const [pagination, setPagination] = useState({ pageIndex: 0, pageSize: 50 })
    const query = usePortalApplications({
        q,
        status,
        page: pagination.pageIndex + 1,
        page_size: pagination.pageSize,
    })
    const columns = useMemo<ColumnDef<PortalApplication>[]>(
        () => [
            {
                id: "title",
                header: "申请内容",
                accessorFn: (row) =>
                    row.title ??
                    (row.input.name as string) ??
                    kindLabels[row.kind],
            },
            {
                accessorKey: "kind",
                header: "申请类型",
                cell: ({ row }) =>
                    kindLabels[row.original.kind] ?? "供应合作申请",
            },
            {
                accessorKey: "status",
                header: "当前状态",
                cell: ({ row }) =>
                    statusLabels[row.original.status] ?? "待核对",
            },
            { accessorKey: "reason", header: "申请原因" },
            {
                accessorKey: "updated_at",
                header: "更新时间",
                cell: ({ row }) =>
                    timeLabel(
                        row.original.updated_at ?? row.original.created_at,
                    ),
            },
            {
                id: "actions",
                header: "操作",
                enableHiding: false,
                cell: ({ row }) => (
                    <Link
                        id={`supplier-portal-application-open-${toAutomationIdSegment(row.original.id)}`}
                        href={`/supplier-portal/applications/${encodeURIComponent(row.original.id)}`}
                        className="text-primary"
                    >
                        查看与处理
                    </Link>
                ),
            },
        ],
        [],
    )
    return (
        <PortalSurface
            title="我的申请"
            description="草稿可修改；送审后的供应商原稿保留，采购确认前不影响生效条款。"
        >
            <ListWorkSurface
                ariaLabel="我的申请"
                selectionBar={
                    <span className="text-sm">
                        共 {query.data?.total ?? 0} 条申请
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
                            id="supplier-portal-applications-search"
                            aria-label="搜索申请"
                            placeholder="搜索申请"
                            value={search}
                            onChange={(event) => setSearch(event.target.value)}
                        />
                        <OptionCombobox
                            id="supplier-portal-applications-status"
                            aria-label="申请状态"
                            value={status || null}
                            onValueChange={(value) => {
                                setStatus(value ?? "")
                                setPagination((p) => ({ ...p, pageIndex: 0 }))
                            }}
                            options={Object.entries(statusLabels).map(
                                ([value, label]) => ({ value, label }),
                            )}
                            placeholder="全部状态"
                        />
                        <Button
                            id="supplier-portal-applications-search-submit"
                            type="submit"
                            variant="outline"
                        >
                            搜索
                        </Button>
                    </form>
                }
                table={
                    <DataTable
                        id="supplier-portal-applications-table"
                        data={query.data?.items ?? []}
                        columns={columns}
                        getRowId={(row) => row.id}
                        rowCount={query.data?.total ?? 0}
                        pagination={pagination}
                        onPaginationChange={setPagination}
                        loading={query.isFetching}
                        onRowOpen={(row) =>
                            router.push(
                                `/supplier-portal/applications/${encodeURIComponent(row.id)}`,
                            )
                        }
                        errorState={
                            <PortalError
                                error={query.error}
                                retry={() => void query.refetch()}
                                id="supplier-portal-applications-retry"
                            />
                        }
                        emptyState={
                            <p className="p-6 text-sm text-muted-foreground">
                                暂无申请，已有商品报价和新品提报均可创建草稿。
                            </p>
                        }
                    />
                }
            />
        </PortalSurface>
    )
}
