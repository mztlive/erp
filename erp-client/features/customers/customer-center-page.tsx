"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { PlusIcon, SearchIcon } from "lucide-react"

import {
  BusinessEmptyState,
  BusinessStatusBadge,
  DataFreshness,
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
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { CustomerCreateSheet } from "@/features/customers/customer-form-sheet"
import {
  parseCustomerScope,
  SCOPE_LABELS,
  SCOPE_ORDER,
} from "@/features/customers/filter-customers"
import { useCustomerDirectoryQuery } from "@/features/customers/queries"
import type { CustomerScope } from "@/features/customers/types"

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

  React.useEffect(() => {
    setSearchDraft(q)
  }, [q])

  const directoryQuery = useCustomerDirectoryQuery({
    scope,
    status,
    query: q,
    sort: "recent_business",
  })

  const data = directoryQuery.data
  const items = data?.items ?? []

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
            dateTime={data?.queriedAt ?? new Date().toISOString()}
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
                if (next) pushState({ scope: next })
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
              onValueChange={(v) =>
                pushState({
                  status: (v ?? "active") as "active" | "disabled" | "all",
                })
              }
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
              onClick={() => pushState({ q: searchDraft })}
            >
              搜索
            </Button>
          </div>
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
          description="你可以进入客户中心，但当前权限与数据范围内没有任何客户。这与「尚未创建客户」不同。"
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
        <Card size="sm">
          <CardHeader className="border-b">
            <CardTitle>客户结果</CardTitle>
            <CardDescription>
              共 {items.length} 家 · {SCOPE_LABELS[scope]}
              {status !== "active" ? ` · ${status === "disabled" ? "停用" : "全部状态"}` : ""}
              。本页是进入稳定客户对象的选择器，不是第二套全功能客户表。
            </CardDescription>
          </CardHeader>
          <CardContent className="divide-y p-0" role="list" aria-label="客户列表">
            {items.map((item) => (
              <div
                key={item.id}
                role="listitem"
                className="flex flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:justify-between"
              >
                <div className="min-w-0 space-y-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <Link
                      href={`/sales/customers/${item.id}`}
                      className="font-medium text-foreground underline-offset-4 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    >
                      {item.shortName || item.legalName}
                    </Link>
                    <BusinessStatusBadge context="list" {...item.statusLabel} />
                    {item.attentionTags?.map((tag) => (
                      <Badge key={tag} variant="outline">
                        {tag}
                      </Badge>
                    ))}
                  </div>
                  <div className="text-xs text-muted-foreground">
                    <span className="num">{item.customerNo}</span>
                    {" · 负责销售 "}
                    {item.ownerName}
                    {item.collaboratorCount > 0
                      ? ` · 协作 ${item.collaboratorCount} 人`
                      : ""}
                  </div>
                  <div
                    className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground"
                    aria-label="关系指标摘要"
                  >
                    <span>
                      有效合同{" "}
                      <span className="num text-foreground">
                        {item.metrics.activeContractCount}
                      </span>
                    </span>
                    <span>
                      进行中销售单{" "}
                      <span className="num text-foreground">
                        {item.metrics.inProgressSalesOrderCount}
                      </span>
                    </span>
                    <span className="inline-flex items-center gap-1">
                      未结清
                      <MoneyValue value={item.metrics.receivableBalance} />
                    </span>
                    {Number.parseFloat(item.metrics.overdueAmount) > 0 ? (
                      <span className="inline-flex items-center gap-1 text-warning-foreground">
                        逾期
                        <MoneyValue value={item.metrics.overdueAmount} />
                      </span>
                    ) : null}
                  </div>
                </div>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  render={<Link href={`/sales/customers/${item.id}`} />}
                >
                  打开
                </Button>
              </div>
            ))}
          </CardContent>
        </Card>
      )}

      <CustomerCreateSheet
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
