"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { PlusIcon, SearchIcon } from "lucide-react"
import type {
  ColumnDef,
  PaginationState,
  SortingState,
} from "@tanstack/react-table"

import {
  BusinessEmptyState,
  BusinessFailureState,
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  ListToolbar,
  MoneyValue,
  OptionCombobox,
  PageActions,
  PageHeader,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
} from "@/components/ui/card"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { CustomerCreateDialog } from "@/features/customers/customer-create-dialog"
import {
  parseCustomerScope,
  SCOPE_LABELS,
  SCOPE_ORDER,
} from "@/features/customers/filter-customers"
import { useCustomerDirectoryQuery } from "@/features/customers/queries"
import type {
  CustomerDirectoryItem,
  CustomerScope,
} from "@/features/customers/types"

type DirectoryStatus = "active" | "disabled" | "all"

function writeDirectoryUrl(
  pathname: string,
  params: {
    scope: CustomerScope
    status: DirectoryStatus
    q: string
    sort: string
    dir: "asc" | "desc"
    page: number
  }
): string {
  const sp = new URLSearchParams()
  if (params.scope !== "mine") sp.set("scope", params.scope)
  if (params.status !== "active") sp.set("status", params.status)
  if (params.q.trim()) sp.set("q", params.q.trim())
  if (params.sort && params.sort !== "business") sp.set("sort", params.sort)
  if (params.dir === "asc") sp.set("dir", "asc")
  if (params.page > 1) sp.set("page", String(params.page))
  const qs = sp.toString()
  return qs ? `${pathname}?${qs}` : pathname
}

/** 表头排序列 → 目录查询排序键（filter-customers 已实现的排序能力）。 */
const SORT_COLUMN_TO_FIELD: Record<string, string> = {
  customer: "name",
  overdue: "overdue_desc",
  business: "recent_business",
}

function parsePage(value: string | null): number {
  const page = Number.parseInt(value ?? "", 10)
  return Number.isFinite(page) && page > 0 ? page : 1
}

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
  const sortParam = searchParams.get("sort")
  const sort: string =
    sortParam === "customer" || sortParam === "overdue"
      ? sortParam
      : "business"
  const dir: "asc" | "desc" = searchParams.get("dir") === "asc" ? "asc" : "desc"
  const page = parsePage(searchParams.get("page"))

  const [searchDraft, setSearchDraft] = React.useState(q)
  const [createOpen, setCreateOpen] = React.useState(false)
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: page - 1,
    pageSize: 20,
  })

  React.useEffect(() => {
    setSearchDraft(q)
  }, [q])

  const directoryQuery = useCustomerDirectoryQuery({
    scope,
    status,
    query: q,
    sort:
      (SORT_COLUMN_TO_FIELD[sort] as
        | "recent_business"
        | "name"
        | "overdue_desc") ?? "recent_business",
    sortDir: dir,
  })

  const data = directoryQuery.data
  const items = React.useMemo(
    () => data?.items ?? [],
    [data?.items]
  )

  const pageRows = React.useMemo(() => {
    const start = pagination.pageIndex * pagination.pageSize
    return items.slice(start, start + pagination.pageSize)
  }, [items, pagination.pageIndex, pagination.pageSize])

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
          page: next.page ?? pagination.pageIndex + 1,
        })
      )
    },
    [dir, pagination.pageIndex, pathname, q, router, scope, sort, status]
  )

  const handlePaginationChange = React.useCallback(
    (next: PaginationState) => {
      setPagination(next)
      pushState({ page: next.pageIndex + 1 })
    },
    [pushState]
  )

  const sorting = React.useMemo<SortingState>(
    () => [{ id: sort, desc: dir === "desc" }],
    [dir, sort]
  )

  const handleSortingChange = React.useCallback(
    (next: SortingState) => {
      const head = next[0]
      if (!head || !SORT_COLUMN_TO_FIELD[head.id]) return
      setPagination((previous) =>
        previous.pageIndex === 0 ? previous : { ...previous, pageIndex: 0 }
      )
      pushState({
        sort: head.id,
        dir: head.desc ? "desc" : "asc",
        page: 1,
      })
    },
    [pushState]
  )

  const clearFilters = () => {
    setSearchDraft("")
    setPagination((previous) =>
      previous.pageIndex === 0 ? previous : { ...previous, pageIndex: 0 }
    )
    router.replace(pathname)
  }

  const hasActiveFilters =
    scope !== "mine" || status !== "active" || q.trim().length > 0

  const columns = React.useMemo<ColumnDef<CustomerDirectoryItem>[]>(
    () => [
      {
        id: "customer",
        accessorFn: (row) => row.shortName || row.legalName,
        header: "客户",
        meta: { label: "客户", width: "reference" },
        cell: ({ row }) => (
          <div className="min-w-0">
            <Link
              href={`/sales/customers/${row.original.id}`}
              className="font-medium text-foreground underline-offset-4 hover:underline"
            >
              {row.original.shortName || row.original.legalName}
            </Link>
            <div className="flex flex-wrap items-center gap-1.5">
              <span className="num text-xs text-muted-foreground">
                {row.original.customerNo}
              </span>
              {row.original.attentionTags?.map((tag) => (
                <Badge key={tag} variant="outline" className="text-[10px]">
                  {tag}
                </Badge>
              ))}
            </div>
          </div>
        ),
      },
      {
        id: "owner",
        accessorKey: "ownerName",
        header: "负责销售",
        meta: { label: "负责销售", width: "default" },
        enableSorting: false,
        cell: ({ row }) => (
          <div className="text-sm">
            <div>{row.original.ownerName}</div>
            {row.original.collaboratorCount > 0 ? (
              <div className="text-xs text-muted-foreground">
                协作 {row.original.collaboratorCount} 人
              </div>
            ) : null}
          </div>
        ),
      },
      {
        id: "status",
        accessorFn: (row) => row.statusLabel.label,
        header: "状态",
        meta: { label: "状态", width: "status" },
        enableSorting: false,
        cell: ({ row }) => (
          <BusinessStatusBadge context="list" {...row.original.statusLabel} />
        ),
      },
      {
        id: "contracts",
        accessorFn: (row) => row.metrics.activeContractCount,
        header: "有效合同",
        meta: { label: "有效合同", width: "status", numeric: true },
        enableSorting: false,
        cell: ({ row }) => (
          <span className="num text-sm">
            {row.original.metrics.activeContractCount}
          </span>
        ),
      },
      {
        id: "orders",
        accessorFn: (row) => row.metrics.inProgressSalesOrderCount,
        header: "进行中销售单",
        meta: { label: "进行中销售单", width: "status", numeric: true },
        enableSorting: false,
        cell: ({ row }) => (
          <span className="num text-sm">
            {row.original.metrics.inProgressSalesOrderCount}
          </span>
        ),
      },
      {
        id: "receivable",
        accessorFn: (row) => row.metrics.receivableBalance,
        header: "未结清",
        meta: { label: "未结清", width: "amount", numeric: true },
        enableSorting: false,
        cell: ({ row }) => (
          <MoneyValue value={row.original.metrics.receivableBalance} />
        ),
      },
      {
        id: "overdue",
        accessorFn: (row) => row.metrics.overdueAmount,
        header: "逾期",
        meta: { label: "逾期", width: "amount", numeric: true },
        cell: ({ row }) => (
          <span
            className={
              Number.parseFloat(row.original.metrics.overdueAmount) > 0
                ? "text-warning-foreground"
                : undefined
            }
          >
            <MoneyValue value={row.original.metrics.overdueAmount} />
          </span>
        ),
      },
      {
        id: "business",
        accessorFn: (row) => row.recentBusinessAt ?? row.updatedAt,
        header: "最近业务",
        meta: { label: "最近业务", width: "default", numeric: true },
        cell: ({ row }) => (
          <span className="num text-sm text-muted-foreground">
            {(row.original.recentBusinessAt ?? row.original.updatedAt).slice(
              0,
              10
            )}
          </span>
        ),
      },
    ],
    []
  )

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
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
            actions={[
              {
                actionKey: "create",
                label: "新建客户",
                icon: PlusIcon,
                onClick: () => setCreateOpen(true),
              },
            ]}
          />
        }
      />

      {directoryQuery.isError ? (
        <BusinessFailureState
          kind="system"
          title="客户目录加载失败"
          description="暂时拿不到客户数据，请重试；不会影响已保存的客户资料。"
          onRetry={() => {
            void directoryQuery.refetch()
          }}
        />
      ) : directoryQuery.isPending && !data ? (
        <Card size="sm">
          <CardContent className="p-8 text-sm text-muted-foreground">
            正在加载客户目录…
          </CardContent>
        </Card>
      ) : data && !data.hasCustomerScope ? (
        <BusinessEmptyState
          kind="no-scope"
          title="当前角色无客户范围"
          description="当前权限与数据范围内没有客户；不代表系统尚无客户。"
        />
      ) : data && data.totalInScope === 0 && !q.trim() && status === "active" ? (
        <BusinessEmptyState
          kind="no-data"
          title="当前范围尚无客户"
          description={`${SCOPE_LABELS[scope]}下还没有客户。有权时可新建客户。`}
          action={
            <Button type="button" onClick={() => setCreateOpen(true)}>
              新建客户
            </Button>
          }
        />
      ) : items.length === 0 ? (
        <BusinessEmptyState
          kind="filter"
          title="当前筛选无结果"
          description={`范围「${SCOPE_LABELS[scope]}」${status !== "active" ? ` · 状态 ${status}` : ""}${q ? ` · 关键词「${q}」` : ""} 下没有匹配客户。`}
          action={
            hasActiveFilters ? (
              <Button type="button" variant="outline" onClick={clearFilters}>
                清除筛选
              </Button>
            ) : null
          }
        />
      ) : (
        <BusinessTableFrame
          title="客户结果"
          description={
            scope !== "mine" || status !== "active" || q.trim()
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
                    value={searchDraft}
                    onChange={(e) => setSearchDraft(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        pushState({ q: searchDraft, page: 1 })
                      }
                    }}
                    placeholder="名称、编码、统一社会信用代码、负责销售"
                    aria-label="搜索客户"
                  />
                </InputGroup>
              }
              filters={
                <div className="flex flex-wrap items-center gap-2">
                  <ToggleGroup
                    value={[scope]}
                    onValueChange={(values) => {
                      const next = values[0] as CustomerScope | undefined
                      if (next) {
                        pushState({ scope: next, page: 1 })
                      }
                    }}
                    variant="outline"
                    size="sm"
                    spacing={0}
                    aria-label="客户范围"
                  >
                    {SCOPE_ORDER.map((key) => (
                      <ToggleGroupItem key={key} value={key}>
                        {SCOPE_LABELS[key]}
                      </ToggleGroupItem>
                    ))}
                  </ToggleGroup>
                  <OptionCombobox
                    aria-label="客户状态"
                    value={status}
                    onValueChange={(v) => {
                      pushState({
                        status: (v ?? "active") as DirectoryStatus,
                        page: 1,
                      })
                    }}
                    options={[
                      { value: "active", label: "启用" },
                      { value: "disabled", label: "停用" },
                      { value: "all", label: "全部状态" },
                    ]}
                    className="w-[7.5rem]"
                    size="sm"
                    allowClear={false}
                    placeholder="客户状态"
                  />
                </div>
              }
            />
          }
          table={
            <DataTable
              data={pageRows}
              columns={columns}
              getRowId={(row) => row.id}
              rowCount={items.length}
              sorting={sorting}
              onSortingChange={handleSortingChange}
              pagination={pagination}
              onPaginationChange={handlePaginationChange}
              layout="flush"
              density="compact"
              rowLabel={(row) => row.shortName || row.legalName}
              defaultColumnPinning={{
                left: ["customer"],
              }}
              onRowPreview={(row) => router.push(`/sales/customers/${row.id}`)}
              onRowOpen={(row) => router.push(`/sales/customers/${row.id}`)}
            />
          }
        />
      )}

      <CustomerCreateDialog
        open={createOpen}
        onOpenChange={setCreateOpen}
        onSucceeded={(customerId) => {
          setCreateOpen(false)
          router.push(`/sales/customers/${customerId}`)
        }}
      />
    </div>
  )
}
