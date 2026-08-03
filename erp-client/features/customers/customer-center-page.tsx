"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { PlusIcon, SearchIcon } from "lucide-react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import {
  BusinessEmptyState,
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

function writeDirectoryUrl(
  pathname: string,
  params: {
    scope: CustomerScope
    status: "active" | "disabled" | "all"
    q: string
  }
): string {
  const sp = new URLSearchParams()
  if (params.scope !== "mine") sp.set("scope", params.scope)
  if (params.status !== "active") sp.set("status", params.status)
  if (params.q.trim()) sp.set("q", params.q.trim())
  const qs = sp.toString()
  return qs ? `${pathname}?${qs}` : pathname
}

export function CustomerCenterPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()

  const scope = parseCustomerScope(searchParams.get("scope"))
  const statusParam = searchParams.get("status")
  const status: "active" | "disabled" | "all" =
    statusParam === "disabled" || statusParam === "all"
      ? statusParam
      : "active"
  const q = searchParams.get("q") ?? ""

  const [searchDraft, setSearchDraft] = React.useState(q)
  const [createOpen, setCreateOpen] = React.useState(false)
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: 0,
    pageSize: 20,
  })

  React.useEffect(() => {
    setSearchDraft(q)
  }, [q])

  const resetPagination = React.useCallback(() => {
    setPagination((previous) =>
      previous.pageIndex === 0 ? previous : { ...previous, pageIndex: 0 }
    )
  }, [])

  const directoryQuery = useCustomerDirectoryQuery({
    scope,
    status,
    query: q,
    sort: "recent_business",
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
        cell: ({ row }) => (
          <BusinessStatusBadge context="list" {...row.original.statusLabel} />
        ),
      },
      {
        id: "contracts",
        accessorFn: (row) => row.metrics.activeContractCount,
        header: "有效合同",
        meta: { label: "有效合同", width: "status", numeric: true },
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

  const pushState = React.useCallback(
    (next: {
      scope?: CustomerScope
      status?: "active" | "disabled" | "all"
      q?: string
    }) => {
      router.replace(
        writeDirectoryUrl(pathname, {
          scope: next.scope ?? scope,
          status: next.status ?? status,
          q: next.q ?? q,
        })
      )
    },
    [pathname, q, router, scope, status]
  )

  const clearFilters = () => {
    setSearchDraft("")
    router.replace(pathname)
  }

  const hasActiveFilters =
    scope !== "mine" || status !== "active" || q.trim().length > 0

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
            updatedAt="刚刚"
            dateTime={data?.queriedAt}
            state="fresh"
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
                mobileVisibility: "hide",
                onClick: () => setCreateOpen(true),
              },
            ]}
          />
        }
      />

      {directoryQuery.isPending ? (
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
                        pushState({ q: searchDraft })
                        resetPagination()
                      }
                    }}
                    placeholder="名称、编码、统一社会信用代码"
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
                        pushState({ scope: next })
                        resetPagination()
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
                        status: (v ?? "active") as
                          | "active"
                          | "disabled"
                          | "all",
                      })
                      resetPagination()
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
                  <Button
                    type="button"
                    size="sm"
                    variant="secondary"
                    onClick={() => {
                      pushState({ q: searchDraft })
                      resetPagination()
                    }}
                  >
                    搜索
                  </Button>
                </div>
              }
              actions={
                <span
                  className="text-xs text-muted-foreground"
                  aria-live="polite"
                >
                  共 {items.length.toLocaleString("zh-CN")} 家
                </span>
              }
            />
          }
          table={
            <DataTable
              data={pageRows}
              columns={columns}
              getRowId={(row) => row.id}
              rowCount={items.length}
              pagination={pagination}
              onPaginationChange={setPagination}
              layout="flush"
              density="compact"
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
