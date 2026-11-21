"use client"

import { PlusIcon } from "lucide-react"
import { useRef, useState } from "react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import {
    BusinessFailureState,
    BusinessStatusBadge,
    ListDataTable,
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspacePage,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
import { Alert } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { getErrorMessage, isPermissionDeniedError } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"
import { listFilterText } from "@/lib/ui-text"
import { useSearchHotkey } from "@/lib/use-search-hotkey"
import type { Company } from "./api"
import { CompanyForm } from "./company-form"
import { useCompaniesQuery, useSaveCompanyMutation } from "./queries"

const title = "公司主体"
const createId = "companies-create"
const emptyCompanies: Company[] = []
const columnOrder = [
    "legal_name",
    "short_name",
    "aliases",
    "unified_credit_code",
    "status",
    "actions",
]
const columnVisibility = { short_name: false }
const columnPinning = { left: ["legal_name"], right: ["actions"] }

export const CompaniesPage = () => {
    const account = useAccountProfileQuery()
    if (account.isPending) {
        return <ListWorkspacePage title={title} loading />
    }
    if (account.isError && !account.data) {
        return (
            <ListWorkspacePage
                title={title}
                alerts={
                    <BusinessFailureState
                        idPrefix="companies-account"
                        error={account.error}
                        onRetry={() => void account.refetch()}
                    />
                }
            />
        )
    }
    const permissions = account.data?.permissions
    if (!hasPermission(permissions, "company:list")) {
        return (
            <ListWorkspacePage
                title={title}
                alerts={
                    <BusinessFailureState
                        kind="permission"
                        title="无权查看公司主体"
                    />
                }
            />
        )
    }
    return (
        <CompanyList
            canCreate={hasPermission(permissions, "company:create")}
            canUpdate={hasPermission(permissions, "company:update")}
        />
    )
}

function CompanyList({
    canCreate,
    canUpdate,
}: {
    canCreate: boolean
    canUpdate: boolean
}) {
    const searchInputRef = useRef<HTMLInputElement>(null)
    useSearchHotkey(searchInputRef)
    const [search, setSearch] = useState("")
    const [keyword, setKeyword] = useState("")
    const [pagination, setPagination] = useState<PaginationState>({
        pageIndex: 0,
        pageSize: 20,
    })
    const [lastPageTotal, setLastPageTotal] = useState<number>()
    const [editing, setEditing] = useState<{
        company?: Company
        triggerId: string
    } | null>(null)
    const [error, setError] = useState("")
    const query = useCompaniesQuery({
        keyword,
        page: pagination.pageIndex + 1,
        page_size: pagination.pageSize,
    })
    const mutation = useSaveCompanyMutation()
    const hasPendingChanges = search.trim() !== keyword
    const showAppliedKeyword = Boolean(keyword) && hasPendingChanges
    const rowCount = query.data?.total ?? lastPageTotal
    const changePagination = (next: PaginationState) => {
        setLastPageTotal(rowCount)
        setPagination(next)
    }
    const applySearch = () => {
        const nextKeyword = search.trim()
        setSearch(nextKeyword)
        setKeyword(nextKeyword)
        setLastPageTotal(nextKeyword === keyword ? rowCount : undefined)
        setPagination((current) => ({ ...current, pageIndex: 0 }))
    }
    const clearSearch = () => {
        setSearch("")
        setKeyword("")
        setLastPageTotal(undefined)
        setPagination((current) => ({ ...current, pageIndex: 0 }))
        searchInputRef.current?.focus()
    }
    const changeStatus = async (company: Company) => {
        setError("")
        try {
            await mutation.mutateAsync({
                id: company.id,
                input: {
                    party_no: company.party_no,
                    version: company.version,
                    legal_name: company.legal_name,
                    short_name: company.short_name,
                    aliases: company.aliases,
                    unified_credit_code: company.unified_credit_code,
                    status: company.status === "active" ? "disabled" : "active",
                },
            })
        } catch (err) {
            setError(getErrorMessage(err, "公司状态修改失败"))
        }
    }
    const columns: ColumnDef<Company>[] = [
        {
            accessorKey: "legal_name",
            header: "公司全称",
            meta: { width: "flex" },
            cell: ({ row }) => (
                <TableIdentityCell
                    primary={row.original.legal_name}
                    primaryTitle={row.original.legal_name}
                    secondary={row.original.short_name}
                    secondaryTitle={row.original.short_name ?? undefined}
                    className="w-0 min-w-full"
                />
            ),
        },
        {
            accessorKey: "short_name",
            header: "简称",
            cell: ({ row }) => (
                <TableTextCell
                    value={row.original.short_name}
                    clamp={1}
                    className="w-0 min-w-full"
                />
            ),
        },
        {
            accessorKey: "aliases",
            header: "导入别名",
            meta: { width: "reference" },
            cell: ({ row }) => (
                <TableTextCell
                    value={row.original.aliases.join("、")}
                    clamp={1}
                    className="w-0 min-w-full"
                />
            ),
        },
        {
            accessorKey: "unified_credit_code",
            header: "统一社会信用代码",
            cell: ({ row }) => (
                <TableTextCell
                    value={row.original.unified_credit_code}
                    clamp={1}
                    numeric
                    className="w-0 min-w-full"
                />
            ),
        },
        {
            accessorKey: "status",
            header: "状态",
            meta: { width: "status" },
            cell: ({ row }) => {
                const active = row.original.status === "active"
                const disabledHint =
                    "停用后不再用于新资料选择，历史引用继续保留。"
                return (
                    <BusinessStatusBadge
                        context="list"
                        label={active ? "当前启用" : "当前停用"}
                        tone={active ? "success" : "neutral"}
                        appearance={active ? "quiet" : "badge"}
                        title={active ? undefined : disabledHint}
                        aria-description={active ? undefined : disabledHint}
                    />
                )
            },
        },
    ]
    if (canUpdate) {
        columns.push({
            id: "actions",
            header: "操作",
            meta: { align: "end", width: "status" },
            enableHiding: false,
            cell: ({ row }) => {
                const company = row.original
                const segment = toAutomationIdSegment(company.id)
                const editId = `company-edit-${segment}`
                return (
                    <TableRowActions
                        moreId={`company-more-${segment}`}
                        moreLabel={`${company.legal_name} 更多操作`}
                        actions={[
                            {
                                id: editId,
                                label: "编辑",
                                onClick: () =>
                                    setEditing({ company, triggerId: editId }),
                            },
                            {
                                id: `company-status-${segment}`,
                                label:
                                    company.status === "active"
                                        ? "停用"
                                        : "启用",
                                disabled: mutation.isPending,
                                disabledReason: mutation.isPending
                                    ? "正在更新公司状态，请稍候。"
                                    : undefined,
                                loading:
                                    mutation.isPending &&
                                    mutation.variables?.id === company.id,
                                onClick: () => void changeStatus(company),
                            },
                        ]}
                    />
                )
            },
        })
    }

    return (
        <ListWorkspacePage
            title={title}
            actions={
                canCreate
                    ? [
                          {
                              id: createId,
                              actionKey: "create",
                              tier: "primary",
                              label: "新建公司主体",
                              icon: PlusIcon,
                              onClick: () =>
                                  setEditing({ triggerId: createId }),
                          },
                      ]
                    : undefined
            }
            alerts={
                error ? <Alert variant="destructive">{error}</Alert> : undefined
            }
            surface={{
                ariaLabel: "公司主体列表",
                toolbar: (
                    <ListWorkspaceFilterBar
                        morePresentation="popover"
                        idPrefix="companies"
                        formAriaLabel="公司主体查询"
                        onSubmit={applySearch}
                        queryButtonId="companies-search-submit"
                        clearButtonId="companies-clear"
                        search={
                            <ListSearchField
                                id="companies-search"
                                searchInputRef={searchInputRef}
                                value={search}
                                onChange={setSearch}
                                placeholder="搜索公司名称或别名"
                                aria-label="搜索公司名称或别名"
                            />
                        }
                        resultStatus={
                            query.isError
                                ? "查询未完成"
                                : query.isFetching
                                  ? "查询中…"
                                  : null
                        }
                        chips={
                            showAppliedKeyword
                                ? [{ key: "q", label: `搜索：${keyword}` }]
                                : []
                        }
                        onClearChip={clearSearch}
                        onClearAll={clearSearch}
                        extraPrimary={
                            keyword && !showAppliedKeyword ? (
                                <Button
                                    id="companies-clear"
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    className="h-control font-normal text-muted-foreground"
                                    onClick={clearSearch}
                                >
                                    {listFilterText.clear}
                                </Button>
                            ) : undefined
                        }
                        hasPendingChanges={hasPendingChanges}
                    />
                ),
                tableClassName:
                    "[&_[data-column-id='unified_credit_code']]:w-48 [&_[data-column-id='unified_credit_code']]:min-w-44 max-sm:[&_[data-column-id='legal_name']]:min-w-36",
                table: (
                    <ListDataTable
                        id="companies-table"
                        entityName={title}
                        caption="公司主体列表"
                        data={query.data?.items ?? emptyCompanies}
                        columns={columns}
                        defaultColumnOrder={columnOrder}
                        defaultColumnVisibility={columnVisibility}
                        defaultColumnPinning={columnPinning}
                        getRowId={(row) => row.id}
                        rowLabel={(row) => row.legal_name}
                        rowCount={rowCount ?? 0}
                        pagination={pagination}
                        onPaginationChange={changePagination}
                        showPagination={
                            !query.isError && rowCount !== undefined
                        }
                        showToolbarSummary={false}
                        loading={query.isPending || query.isFetching}
                        isError={query.isError}
                        error={query.error ?? undefined}
                        onRetry={
                            isPermissionDeniedError(query.error)
                                ? undefined
                                : () => void query.refetch()
                        }
                        hasActiveFilters={Boolean(keyword)}
                        onClearFilters={clearSearch}
                        canCreate={canCreate}
                        onCreate={() =>
                            setEditing({
                                triggerId: "companies-table-empty-create",
                            })
                        }
                        emptyCopy={{
                            emptyDescription:
                                "公司主体用于我方签约、付款及导入匹配。",
                        }}
                    />
                ),
            }}
        >
            {editing && (editing.company ? canUpdate : canCreate) ? (
                <CompanyForm
                    company={editing.company}
                    onClose={() => setEditing(null)}
                    finalFocus={() =>
                        document.getElementById(editing.triggerId) ??
                        (!editing.company
                            ? document.getElementById(createId)
                            : null) ??
                        searchInputRef.current
                    }
                />
            ) : null}
        </ListWorkspacePage>
    )
}
