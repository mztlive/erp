"use client"
import { useState } from "react"
import type { ColumnDef } from "@tanstack/react-table"
import {
    DataTable,
    PageHeader,
    PageScaffold,
    TableRowActions,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { CompanyForm } from "./company-form"
import { useCompaniesQuery, useSaveCompanyMutation } from "./queries"
import type { Company } from "./api"

export const CompaniesPage = () => {
    const [search, setSearch] = useState("")
    const [keyword, setKeyword] = useState("")
    const [pagination, setPagination] = useState({ pageIndex: 0, pageSize: 20 })
    const [editing, setEditing] = useState<Company | "new" | null>(null)
    const [error, setError] = useState("")
    const account = useAccountProfileQuery()
    const canCreate = hasPermission(account.data?.permissions, "company:create")
    const canUpdate = hasPermission(account.data?.permissions, "company:update")
    const query = useCompaniesQuery({
        keyword,
        page: pagination.pageIndex + 1,
        page_size: pagination.pageSize,
    })
    const mutation = useSaveCompanyMutation()
    const changeStatus = async (company: Company) => {
        setError("")
        const { id, ...input } = company
        try {
            await mutation.mutateAsync({
                id,
                input: {
                    ...input,
                    status: company.status === "active" ? "disabled" : "active",
                },
            })
        } catch (err) {
            setError(getErrorMessage(err, "公司状态修改失败"))
        }
    }
    const columns: ColumnDef<Company>[] = [
        { accessorKey: "legal_name", header: "公司全称" },
        {
            accessorKey: "short_name",
            header: "简称",
            cell: ({ row }) => row.original.short_name || "—",
        },
        {
            accessorKey: "aliases",
            header: "导入别名",
            cell: ({ row }) => row.original.aliases.join("、") || "—",
        },
        {
            accessorKey: "unified_credit_code",
            header: "统一社会信用代码",
            cell: ({ row }) => row.original.unified_credit_code || "—",
        },
        {
            accessorKey: "status",
            header: "状态",
            cell: ({ row }) =>
                row.original.status === "active" ? "启用" : "停用",
        },
        {
            id: "actions",
            header: "操作",
            cell: ({ row }) => {
                const segment = toAutomationIdSegment(row.original.id)
                return (
                    <TableRowActions
                        moreId={`company-more-${segment}`}
                        moreLabel={`${row.original.legal_name} 更多操作`}
                        actions={[
                            {
                                id: `company-edit-${segment}`,
                                label: "编辑",
                                disabled: !canUpdate,
                                onClick: () => setEditing(row.original),
                            },
                            {
                                id: `company-status-${segment}`,
                                label:
                                    row.original.status === "active"
                                        ? "停用"
                                        : "启用",
                                disabled: !canUpdate || mutation.isPending,
                                onClick: () => void changeStatus(row.original),
                            },
                        ]}
                    />
                )
            },
        },
    ]
    return (
        <PageScaffold density="compact">
            <PageHeader
                title="公司主体"
                description="维护我方签约、付款公司及导入别名。停用后不再用于新资料选择，历史引用继续保留。"
            />
            <form
                className="flex flex-wrap items-center gap-3"
                onSubmit={(e) => {
                    e.preventDefault()
                    setKeyword(search.trim())
                    setPagination((p) => ({ ...p, pageIndex: 0 }))
                }}
            >
                <Input
                    id="companies-search"
                    aria-label="搜索公司名称或别名"
                    placeholder="搜索公司名称或别名"
                    value={search}
                    onChange={(e) => setSearch(e.target.value)}
                    className="max-w-sm"
                />
                <Button
                    id="companies-search-submit"
                    type="submit"
                    variant="outline"
                >
                    查询
                </Button>
                <Button
                    id="companies-create"
                    type="button"
                    disabled={!canCreate}
                    onClick={() => setEditing("new")}
                >
                    新建公司主体
                </Button>
            </form>
            {(error || query.isError) && (
                <p role="alert" className="text-sm text-destructive">
                    {error || getErrorMessage(query.error, "公司主体加载失败")}
                </p>
            )}
            <DataTable
                id="companies-table"
                data={query.data?.items ?? []}
                columns={columns}
                getRowId={(row) => row.id}
                rowCount={query.data?.total ?? 0}
                pagination={pagination}
                onPaginationChange={setPagination}
                loading={query.isFetching}
            />
            {editing && (
                <CompanyForm
                    company={editing === "new" ? undefined : editing}
                    onClose={() => setEditing(null)}
                />
            )}
        </PageScaffold>
    )
}
