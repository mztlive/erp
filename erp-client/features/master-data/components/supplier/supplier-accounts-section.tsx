"use client"

import { useMemo, useState } from "react"
import { PlusIcon, Settings2Icon, UsersIcon } from "lucide-react"
import type { ColumnDef } from "@tanstack/react-table"
import { BusinessEmptyState, DataTable } from "@/components/business"
import { ListWorkSurface } from "@/components/business/list-workspace"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { toast } from "@/components/ui/toast"
import { useAccountProfileQuery } from "@/features/auth/queries"
import type { PortalAccount } from "@/features/master-data/api/supplier-portal-accounts"
import { usePortalAccounts } from "@/features/master-data/hooks/use-supplier-portal-accounts"
import { PortalError } from "@/features/supplier-portal/components/surface"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"
import {
    SupplierAccountCreateDialog,
    SupplierAccountEditDialog,
} from "./supplier-account-dialogs"

export function SupplierAccountsSection({
    supplierId,
    supplierEnabled,
}: {
    supplierId: string
    supplierEnabled: boolean
}) {
    const profile = useAccountProfileQuery()
    const permissions = profile.isSuccess ? profile.data.permissions : undefined
    const canList = hasPermission(permissions, "supplier_portal_account:list")
    const canCreate = hasPermission(
        permissions,
        "supplier_portal_account:create",
    )
    const canUpdate = hasPermission(
        permissions,
        "supplier_portal_account:update",
    )
    const [pagination, setPagination] = useState({ pageIndex: 0, pageSize: 50 })
    const [creating, setCreating] = useState(false)
    const [editing, setEditing] = useState<PortalAccount | null>(null)
    const query = usePortalAccounts(
        {
            supplier_id: supplierId,
            page: pagination.pageIndex + 1,
            page_size: pagination.pageSize,
        },
        canList && !!supplierId,
    )
    const columns = useMemo<ColumnDef<PortalAccount>[]>(
        () => [
            {
                accessorKey: "account",
                header: "登录账号",
                cell: ({ row }) => (
                    <span className="font-medium">{row.original.account}</span>
                ),
            },
            { accessorKey: "name", header: "姓名" },
            {
                accessorKey: "role",
                header: "门户岗位",
                cell: ({ row }) =>
                    row.original.role === "maintainer"
                        ? "供给维护员"
                        : "只读人员",
            },
            {
                accessorKey: "active",
                header: "状态",
                cell: ({ row }) => (
                    <Badge
                        variant={row.original.active ? "success" : "neutral"}
                    >
                        {row.original.active ? "启用" : "停用"}
                    </Badge>
                ),
            },
            ...(canUpdate
                ? ([
                      {
                          id: "actions",
                          header: "操作",
                          enableHiding: false,
                          cell: ({ row }) => (
                              <Button
                                  id={`supplier-portal-account-edit-${toAutomationIdSegment(row.original.account_id)}`}
                                  variant="ghost"
                                  size="sm"
                                  onClick={() => setEditing(row.original)}
                              >
                                  <Settings2Icon
                                      data-icon="inline-start"
                                      aria-hidden
                                  />
                                  配置
                              </Button>
                          ),
                      },
                  ] satisfies ColumnDef<PortalAccount>[])
                : []),
        ],
        [canUpdate],
    )

    if (!canList) return null

    return (
        <section
            className="space-y-5"
            aria-labelledby="supplier-portal-accounts-title"
        >
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="space-y-1">
                    <h2
                        id="supplier-portal-accounts-title"
                        className="text-section-title font-semibold tracking-tight"
                    >
                        门户账号
                    </h2>
                    <p className="text-xs text-muted-foreground">
                        为供应商人员开通登录账号，按岗位分配维护或查看权限。
                    </p>
                    {!supplierEnabled && (
                        <p className="text-xs text-muted-foreground">
                            供应商已停用，暂不能开通新账号。
                        </p>
                    )}
                </div>
                {canCreate && (
                    <Button
                        id="supplier-portal-account-create"
                        type="button"
                        disabled={!supplierEnabled}
                        onClick={() => setCreating(true)}
                    >
                        <PlusIcon data-icon="inline-start" aria-hidden />
                        开通账号
                    </Button>
                )}
            </div>
            {query.isSuccess && query.data.total === 0 ? (
                <BusinessEmptyState
                    kind="no-data"
                    icon={UsersIcon}
                    title="还没有登录账号"
                    description={
                        canCreate
                            ? "点击右上角「开通账号」，添加需要使用供应商门户的人员。"
                            : "此供应商暂未开通门户账号。"
                    }
                    className="py-12"
                />
            ) : (
                <ListWorkSurface
                    ariaLabel="门户账号列表"
                    selectionBar={
                        <span className="text-xs text-muted-foreground">
                            共 {query.data?.total ?? 0} 个账号
                        </span>
                    }
                    table={
                        <DataTable
                            id="supplier-portal-admin-accounts-table"
                            data={query.data?.items ?? []}
                            columns={columns}
                            getRowId={(row) => row.account_id}
                            pagination={pagination}
                            onPaginationChange={setPagination}
                            rowCount={query.data?.total ?? 0}
                            showPagination={
                                (query.data?.total ?? 0) >
                                    pagination.pageSize ||
                                pagination.pageIndex > 0
                            }
                            loading={query.isFetching}
                            errorState={
                                query.isError ? (
                                    <PortalError
                                        error={query.error}
                                        retry={() => void query.refetch()}
                                        id="supplier-portal-admin-accounts-retry"
                                    />
                                ) : undefined
                            }
                        />
                    }
                />
            )}
            {canCreate && creating && (
                <SupplierAccountCreateDialog
                    supplierId={supplierId}
                    supplierEnabled={supplierEnabled}
                    onReload={() => query.refetch({ throwOnError: true })}
                    onSaved={() => {
                        setCreating(false)
                        setPagination((current) => ({
                            ...current,
                            pageIndex: 0,
                        }))
                        toast.add({ title: "账号已开通", type: "success" })
                    }}
                    onClose={() => setCreating(false)}
                />
            )}
            {canUpdate && editing && (
                <SupplierAccountEditDialog
                    key={editing.account_id}
                    account={
                        query.data?.items.find(
                            (item) => item.account_id === editing.account_id,
                        ) ?? editing
                    }
                    onReload={async () => {
                        const result = await query.refetch({
                            throwOnError: true,
                        })
                        const current = result.data?.items.find(
                            (item) => item.account_id === editing.account_id,
                        )
                        if (!current)
                            throw new Error(
                                "此账号已不在当前列表中，请核对绑定或查看资格后继续。",
                            )
                        return current
                    }}
                    onClose={() => setEditing(null)}
                />
            )}
        </section>
    )
}
