"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { PlusIcon, SearchIcon } from "lucide-react"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataFreshness,
    DataTable,
    ListToolbar,
    OptionCombobox,
    PageActions,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupButton,
    InputGroupInput,
} from "@/components/ui/input-group"
import { cn } from "@/lib/utils"
import { CustomerCreateDialog } from "@/features/customers/components/customer-create-dialog"
import {
    parseCustomerScope,
    SCOPE_LABELS,
    SCOPE_ORDER,
} from "@/features/customers/lib/filter-customers"
import {
    parsePage,
    SORT_COLUMN_TO_FIELD,
    writeDirectoryUrl,
} from "@/features/customers/lib/directory-url"
import type { DirectoryStatus } from "@/features/customers/lib/directory-url"
import { useCustomerDirectoryColumns } from "@/features/customers/hooks/use-directory-columns"
import { useCustomerDirectoryQuery } from "@/features/customers/hooks/queries"
import { useAccountProfileQuery } from "@/features/auth/queries"
import type { CustomerScope } from "@/features/customers/types"
import { hasPermission } from "@/lib/permissions"

export function CustomerCenterPage() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const scope = parseCustomerScope(searchParams.get("scope"))
    const statusParam = searchParams.get("status")
    const status: DirectoryStatus =
        statusParam === "disabled" || statusParam === "all"
            ? statusParam
            : "active"
    const q = searchParams.get("q") ?? ""
    const sort = "business"
    const dir: "asc" | "desc" =
        searchParams.get("dir") === "asc" ? "asc" : "desc"
    const page = parsePage(searchParams.get("page"))

    const [searchDraft, setSearchDraft] = React.useState(q)
    const [createOpen, setCreateOpen] = React.useState(false)
    const accountProfile = useAccountProfileQuery()
    const canCreate = hasPermission(
        accountProfile.data?.permissions,
        "customer:create",
    )
    const canReadAll = hasPermission(
        accountProfile.data?.permissions,
        "customer_scope:detail",
    )

    React.useEffect(() => {
        setSearchDraft(q)
    }, [q])

    React.useEffect(() => {
        if (
            !accountProfile.isPending &&
            scope === "all_authorized" &&
            !canReadAll
        ) {
            router.replace(
                writeDirectoryUrl(pathname, {
                    scope: "mine",
                    status,
                    q,
                    sort,
                    dir,
                    page: 1,
                }),
            )
        }
    }, [
        accountProfile.isPending,
        canReadAll,
        dir,
        page,
        pathname,
        q,
        router,
        scope,
        sort,
        status,
    ])

    React.useEffect(() => {
        const onKeyDown = (event: KeyboardEvent) => {
            const target = event.target as HTMLElement | null
            if (
                target &&
                (target.tagName === "INPUT" ||
                    target.tagName === "TEXTAREA" ||
                    target.tagName === "SELECT" ||
                    target.isContentEditable)
            ) {
                return
            }
            if (event.key === "/" && !event.metaKey && !event.ctrlKey) {
                event.preventDefault()
                document
                    .querySelector<HTMLInputElement>(
                        '[data-slot="customer-search"]',
                    )
                    ?.focus()
            }
        }
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [])

    const directoryQuery = useCustomerDirectoryQuery({
        scope,
        status,
        query: q,
        sort: (SORT_COLUMN_TO_FIELD[sort] as "updated_at") ?? "updated_at",
        sortDir: dir,
        page,
        pageSize: 20,
    })

    const data = directoryQuery.data
    const items = React.useMemo(() => data?.items ?? [], [data?.items])

    // 分页从 URL 派生（P6），筛选/搜索变更写 URL 并回第 1 页。
    const pagination = React.useMemo<PaginationState>(
        () => ({ pageIndex: Math.max(0, page - 1), pageSize: 20 }),
        [page],
    )

    const pushState = React.useCallback(
        (next: {
            scope?: CustomerScope
            status?: DirectoryStatus
            q?: string
            sort?: string
            dir?: "asc" | "desc"
            page?: number
        }) => {
            router.replace(
                writeDirectoryUrl(pathname, {
                    scope: next.scope ?? scope,
                    status: next.status ?? status,
                    q: next.q ?? q,
                    sort: next.sort ?? sort,
                    dir: next.dir ?? dir,
                    page: next.page ?? page,
                }),
            )
        },
        [dir, page, pathname, q, router, scope, sort, status],
    )

    const handlePaginationChange = React.useCallback(
        (next: PaginationState) => {
            pushState({ page: next.pageIndex + 1 })
        },
        [pushState],
    )

    const sorting = React.useMemo<SortingState>(
        () => [{ id: sort, desc: dir === "desc" }],
        [dir, sort],
    )

    const handleSortingChange = React.useCallback(
        (next: SortingState) => {
            const head = next[0]
            if (!head || !SORT_COLUMN_TO_FIELD[head.id]) return
            pushState({
                sort: head.id,
                dir: head.desc ? "desc" : "asc",
                page: 1,
            })
        },
        [pushState],
    )

    /** P4：清 q/status/分页，保留 scope（视图）与 sort/dir（排序）。 */
    const clearFilters = () => {
        setSearchDraft("")
        router.replace(
            writeDirectoryUrl(pathname, {
                scope,
                status: "active",
                q: "",
                sort,
                dir,
                page: 1,
            }),
        )
    }

    const hasActiveFilters = status !== "active" || q.trim().length > 0

    const columns = useCustomerDirectoryColumns()

    return (
        <PageScaffold>
            <PageHeader
                title="客户中心"
                breadcrumbs={[
                    { id: "sales", label: "销售", href: "/sales/orders" },
                    { id: "customers", label: "客户中心", current: true },
                ]}
                metadata={
                    <DataFreshness
                        updatedAt={data?.queriedAt?.slice(11, 16) ?? "—"}
                        dateTime={data?.queriedAt}
                        state={directoryQuery.isError ? "failed" : "fresh"}
                        label="客户目录"
                    />
                }
                actions={
                    <PageActions
                        actions={
                            canCreate
                                ? [
                                      {
                                          actionKey: "create",
                                          label: "新建客户",
                                          icon: PlusIcon,
                                          onClick: () => setCreateOpen(true),
                                      },
                                  ]
                                : []
                        }
                    />
                }
            />

            {directoryQuery.isError ? (
                <BusinessFailureState
                    title="客户目录加载失败"
                    error={directoryQuery.error}
                    onRetry={() => {
                        void directoryQuery.refetch()
                    }}
                />
            ) : directoryQuery.isPending && !data ? (
                <div
                    className="h-40 animate-pulse rounded-lg bg-muted"
                    aria-busy="true"
                    aria-label="正在加载客户目录"
                />
            ) : data && !data.hasCustomerScope ? (
                <BusinessEmptyState
                    kind="no-scope"
                    title="当前角色无客户范围"
                    description="当前权限与数据范围内没有客户；不代表系统尚无客户。"
                />
            ) : data ? (
                <BusinessTableFrame
                    title="客户结果"
                    description={
                        data.totalInScope === 0 &&
                        !q.trim() &&
                        status === "active"
                            ? `${SCOPE_LABELS[scope]}下还没有客户。有权时可新建客户。`
                            : items.length === 0
                              ? `当前筛选无结果：${SCOPE_LABELS[scope]}${
                                    status !== "active"
                                        ? ` · ${status === "disabled" ? "停用" : "全部状态"}`
                                        : ""
                                }${q.trim() ? ` · “${q.trim()}”` : ""}`
                              : scope !== "mine" ||
                                  status !== "active" ||
                                  q.trim()
                                ? `当前筛选：${SCOPE_LABELS[scope]}${
                                      status !== "active"
                                          ? ` · ${status === "disabled" ? "停用" : "全部状态"}`
                                          : ""
                                  }${q.trim() ? ` · “${q.trim()}”` : ""}`
                                : `${SCOPE_LABELS[scope]}下的全部客户；本页用于选择客户并进入其详情。`
                    }
                    toolbar={
                        <ListToolbar
                            search={
                                <InputGroup className="max-w-md">
                                    <InputGroupAddon>
                                        <SearchIcon aria-hidden="true" />
                                    </InputGroupAddon>
                                    <InputGroupInput
                                        data-slot="customer-search"
                                        value={searchDraft}
                                        onChange={(e) =>
                                            setSearchDraft(e.target.value)
                                        }
                                        onKeyDown={(e) => {
                                            if (e.key === "Enter") {
                                                pushState({
                                                    q: searchDraft.trim(),
                                                    page: 1,
                                                })
                                            }
                                        }}
                                        placeholder="客户名称或客户编号"
                                        aria-label="搜索客户"
                                    />
                                    <InputGroupAddon align="inline-end">
                                        <InputGroupButton
                                            aria-label="执行客户搜索"
                                            onClick={() =>
                                                pushState({
                                                    q: searchDraft.trim(),
                                                    page: 1,
                                                })
                                            }
                                        >
                                            搜索
                                        </InputGroupButton>
                                    </InputGroupAddon>
                                </InputGroup>
                            }
                            filters={
                                <div className="flex flex-wrap items-center gap-2">
                                    <div
                                        role="group"
                                        aria-label="客户范围"
                                        className="inline-flex items-center rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10"
                                    >
                                        {SCOPE_ORDER.filter(
                                            (key) =>
                                                key !== "all_authorized" ||
                                                canReadAll,
                                        ).map((key) => {
                                            const active = scope === key
                                            return (
                                                <button
                                                    key={key}
                                                    type="button"
                                                    aria-pressed={active}
                                                    onClick={() =>
                                                        pushState({
                                                            scope: key,
                                                            page: 1,
                                                        })
                                                    }
                                                    className={cn(
                                                        "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                                        active
                                                            ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                                                            : "font-normal text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                                                    )}
                                                >
                                                    {SCOPE_LABELS[key]}
                                                </button>
                                            )
                                        })}
                                    </div>
                                    <OptionCombobox
                                        aria-label="客户状态"
                                        value={status}
                                        onValueChange={(v) => {
                                            pushState({
                                                status: (v ??
                                                    "active") as DirectoryStatus,
                                                page: 1,
                                            })
                                        }}
                                        options={[
                                            { value: "active", label: "启用" },
                                            {
                                                value: "disabled",
                                                label: "停用",
                                            },
                                            { value: "all", label: "全部状态" },
                                        ]}
                                        className="w-[7.5rem]"
                                        size="sm"
                                        allowClear={false}
                                        placeholder="客户状态"
                                    />
                                </div>
                            }
                            actions={
                                <>
                                    <span
                                        className="text-xs text-muted-foreground"
                                        aria-live="polite"
                                    >
                                        共{" "}
                                        {data.totalInScope.toLocaleString(
                                            "zh-CN",
                                        )}{" "}
                                        条
                                    </span>
                                    {hasActiveFilters ? (
                                        <Button
                                            type="button"
                                            size="xs"
                                            variant="ghost"
                                            onClick={clearFilters}
                                        >
                                            清除筛选
                                        </Button>
                                    ) : null}
                                </>
                            }
                        />
                    }
                    table={
                        data.totalInScope === 0 &&
                        !q.trim() &&
                        status === "active" ? (
                            <BusinessEmptyState
                                kind="no-data"
                                title="当前范围尚无客户"
                                description={`${SCOPE_LABELS[scope]}下还没有客户。有权时可新建客户。`}
                                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                action={
                                    canCreate ? (
                                        <Button
                                            type="button"
                                            variant="secondary"
                                            className="rounded-lg shadow-none"
                                            onClick={() => setCreateOpen(true)}
                                        >
                                            新建客户
                                        </Button>
                                    ) : null
                                }
                            />
                        ) : items.length === 0 ? (
                            <BusinessEmptyState
                                kind="filter"
                                title="当前筛选无结果"
                                description={`范围“${SCOPE_LABELS[scope]}”${status !== "active" ? ` · 状态 ${status}` : ""}${q ? ` · 关键词“${q}”` : ""} 下没有匹配客户。`}
                                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                action={
                                    hasActiveFilters ? (
                                        <Button
                                            type="button"
                                            variant="secondary"
                                            className="rounded-lg shadow-none"
                                            onClick={clearFilters}
                                        >
                                            清除筛选
                                        </Button>
                                    ) : null
                                }
                            />
                        ) : (
                            <DataTable
                                data={[...items]}
                                columns={columns}
                                getRowId={(row) => row.id}
                                rowCount={data.totalInScope}
                                sorting={sorting}
                                onSortingChange={handleSortingChange}
                                pagination={pagination}
                                onPaginationChange={handlePaginationChange}
                                pageSizeOptions={[20]}
                                layout="flush"
                                density="compact"
                                rowLabel={(row) =>
                                    row.shortName || row.legalName
                                }
                                defaultColumnPinning={{
                                    left: ["customer"],
                                }}
                                onRowPreview={(row) =>
                                    router.push(`/sales/customers/${row.id}`)
                                }
                                onRowOpen={(row) =>
                                    router.push(`/sales/customers/${row.id}`)
                                }
                            />
                        )
                    }
                />
            ) : null}

            <CustomerCreateDialog
                open={createOpen}
                onOpenChange={setCreateOpen}
                onSucceeded={(customerId) => {
                    setCreateOpen(false)
                    router.push(`/sales/customers/${customerId}`)
                }}
            />
        </PageScaffold>
    )
}
