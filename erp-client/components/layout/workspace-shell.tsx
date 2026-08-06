"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { useQueryClient } from "@tanstack/react-query"
import {
  Building2Icon,
  ListTodoIcon,
  LogOutIcon,
} from "lucide-react"

import {
  ErpAppShell,
  GlobalTopbar,
} from "@/components/business"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Badge } from "@/components/ui/badge"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar"
import { isNavItemActive } from "@/lib/nav-active"
import {
  WORKSPACE_NAV_GROUPS,
  type WorkspaceNavBadgeKey,
} from "@/lib/workspace-registry"
import { logoutAndRedirect } from "@/components/providers/auth-session-provider"
import { useCustomerDirectoryQuery } from "@/features/customers/queries"
import { useFulfillmentCountQuery } from "@/features/fulfillment-operations/queries"
import { useProcurementConfirmCountQuery } from "@/features/procurement-confirmation/queries"
import { useUnifiedTaskCountQuery } from "@/features/unified-task-queue/queries"

type NavBadgeCounts = {
  todo: number
  confirm: number
  delivery: number
  warehouse: number
}

function badgeCountFor(
  key: WorkspaceNavBadgeKey,
  counts: NavBadgeCounts
): number | undefined {
  switch (key) {
    case "todo-count":
      return counts.todo
    case "confirm-count":
      return counts.confirm
    case "delivery-count":
      return counts.delivery
    case "warehouse-count":
      return counts.warehouse
  }
}

function AppSidebarNav() {
  const pathname = usePathname()
  // 必须订阅 searchParams：同 path 只改 lane 时 pathname 不变，否则高亮卡死
  const searchParams = useSearchParams()
  const search = searchParams.toString()
  const allHrefs = WORKSPACE_NAV_GROUPS.flatMap((group) =>
    group.items.map((item) => item.href)
  )
  const todoCountQuery = useUnifiedTaskCountQuery()
  const confirmCountQuery = useProcurementConfirmCountQuery()
  const deliveryCountQuery = useFulfillmentCountQuery("procurement")
  const warehouseCountQuery = useFulfillmentCountQuery("warehouse")
  const counts: NavBadgeCounts = {
    todo: todoCountQuery.data?.mine ?? 0,
    confirm: confirmCountQuery.data?.mine ?? 0,
    delivery: deliveryCountQuery.data?.pending ?? 0,
    warehouse: warehouseCountQuery.data?.pending ?? 0,
  }

  return (
    <>
      {WORKSPACE_NAV_GROUPS.map((group, index) => (
        <SidebarGroup key={group.label} className="px-1">
          {/* 透明侧栏不靠分割线区分分组，仅用组标签与间距 */}
          {index > 0 ? (
            <SidebarGroupLabel className="mt-2">{group.label}</SidebarGroupLabel>
          ) : (
            <SidebarGroupLabel className="sr-only">{group.label}</SidebarGroupLabel>
          )}
          <SidebarGroupContent>
            <SidebarMenu className="gap-1">
              {group.items.map((item) => {
                const Icon = item.icon
                const isActive = isNavItemActive(
                  pathname,
                  item.href,
                  allHrefs,
                  search
                )
                const badgeCount = item.badge
                  ? badgeCountFor(item.badge, counts)
                  : undefined

                return (
                  <SidebarMenuItem key={`${group.label}-${item.href}`}>
                    <SidebarMenuButton
                      isActive={isActive}
                      tooltip={item.label}
                      render={<Link href={item.href} />}
                    >
                      <Icon aria-hidden="true" />
                      <span>{item.label}</span>
                      {badgeCount && badgeCount > 0 ? (
                        <Badge
                          variant="secondary"
                          className="ml-auto border-0 bg-background/70 group-data-[collapsible=icon]:hidden"
                        >
                          {badgeCount}
                        </Badge>
                      ) : null}
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                )
              })}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      ))}
    </>
  )
}

export function WorkspaceShell({ children }: { children: React.ReactNode }) {
  const router = useRouter()
  const queryClient = useQueryClient()
  const [search, setSearch] = React.useState("")
  const [searchFocused, setSearchFocused] = React.useState(false)
  const customerSearchQuery = useCustomerDirectoryQuery({
    scope: "team",
    status: "all",
    query: search.trim(),
  })
  const todoCountQuery = useUnifiedTaskCountQuery()
  const todoCount = todoCountQuery.data?.mine
  const customerMatches = React.useMemo(
    () =>
      search.trim().length >= 2
        ? customerSearchQuery.data?.items.slice(0, 5) ?? []
        : [],
    [customerSearchQuery.data?.items, search]
  )

  React.useEffect(() => {
    const focusGlobalSearch = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault()
        document
          .querySelector<HTMLInputElement>('input[aria-label="全局搜索"]')
          ?.focus()
      }
    }
    window.addEventListener("keydown", focusGlobalSearch)
    return () => window.removeEventListener("keydown", focusGlobalSearch)
  }, [])

  const submitSearch = React.useCallback(() => {
    const query = search.trim()
    if (!query) return
    const exactCustomer = customerMatches.find((customer) =>
      [customer.customerNo, customer.legalName, customer.shortName]
        .filter(Boolean)
        .some((value) => value?.toLocaleLowerCase() === query.toLocaleLowerCase())
    )
    if (exactCustomer) {
      router.push(`/sales/customers/${exactCustomer.id}`)
      setSearchFocused(false)
      return
    }
    router.push(`/sales/orders?search=${encodeURIComponent(query)}`)
    setSearchFocused(false)
  }, [customerMatches, router, search])

  const openCustomer = React.useCallback(
    (customer: (typeof customerMatches)[number]) => {
      router.push(`/sales/customers/${customer.id}`)
      setSearch("")
      setSearchFocused(false)
    },
    [router]
  )

  return (
    <ErpAppShell
      className="min-h-svh"
      contentLabel="主工作区"
      sidebarCollapsible="none"
      showSidebarRail={false}
      sidebarHeader={
        <div className="flex items-center gap-2.5 px-2 py-3">
          <div className="flex size-9 items-center justify-center rounded-xl bg-primary text-primary-foreground shadow-sm">
            <Building2Icon className="size-5" aria-hidden="true" />
          </div>
          <div className="min-w-0">
            <div className="truncate text-base font-bold tracking-tight text-foreground">
              员工福利 ERP
            </div>
            <div className="truncate text-xs text-muted-foreground">
              内部工作台
            </div>
          </div>
        </div>
      }
      sidebarContent={<AppSidebarNav />}
      topbar={
        <div className="relative">
          <GlobalTopbar
            showSidebarTrigger
            search={{
              ariaLabel: "全局搜索",
              placeholder: "单号、客户、合同…",
              shortcut: "⌘K",
              value: search,
              onChange: (event) => setSearch(event.target.value),
              onFocus: () => setSearchFocused(true),
              onBlur: () => setSearchFocused(false),
              onKeyDown: (event) => {
                if (event.key === "Escape") {
                  setSearchFocused(false)
                  event.currentTarget.blur()
                } else if (event.key === "Enter") submitSearch()
              },
            }}
            actions={[
              {
                actionKey: "todos",
                label: "待办",
                icon: ListTodoIcon,
                badge:
                  todoCount && todoCount > 0
                    ? { label: String(todoCount), variant: "secondary" }
                    : undefined,
                onClick: () => router.push("/workspace/tasks"),
              },
            ]}
            trailing={
              <DropdownMenu>
                <DropdownMenuTrigger
                  render={
                    <button
                      type="button"
                      className="rounded-full outline-none ring-offset-background focus-visible:ring-2 focus-visible:ring-ring"
                      aria-label="账号菜单"
                    />
                  }
                >
                  <Avatar
                    size="default"
                    className="size-9 cursor-pointer shadow-sm ring-2 ring-card"
                  >
                    <AvatarFallback className="bg-primary/10 text-primary">
                      用
                    </AvatarFallback>
                  </Avatar>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="min-w-48">
                  <DropdownMenuLabel>
                    <div className="flex flex-col gap-0.5">
                      <span className="text-sm font-medium text-foreground">
                        已登录
                      </span>
                      <span className="text-xs font-normal text-muted-foreground">
                        后台账号
                      </span>
                    </div>
                  </DropdownMenuLabel>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem
                    variant="destructive"
                    onClick={() => logoutAndRedirect(router, queryClient)}
                  >
                    <LogOutIcon aria-hidden="true" />
                    退出登录
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            }
          />
          {searchFocused && search.trim().length >= 2 ? (
            <div
              role="listbox"
              aria-label="客户搜索结果"
              className="absolute right-4 top-[calc(100%-0.25rem)] z-50 w-[min(24rem,calc(100vw-2rem))] rounded-xl border bg-popover p-1 text-popover-foreground shadow-md md:right-6"
              onMouseDown={(event) => event.preventDefault()}
            >
              <p className="px-2 py-1 text-xs font-medium text-muted-foreground">
                客户
              </p>
              {customerSearchQuery.isFetching ? (
                <p className="px-2 py-2 text-sm text-muted-foreground">正在搜索…</p>
              ) : customerMatches.length > 0 ? (
                customerMatches.map((customer) => (
                  <button
                    key={customer.id}
                    type="button"
                    role="option"
                    aria-selected="false"
                    className="flex w-full items-center justify-between gap-3 rounded-lg px-2 py-2 text-left text-sm hover:bg-accent focus-visible:bg-accent focus-visible:outline-none"
                    onClick={() => openCustomer(customer)}
                  >
                    <span className="min-w-0 truncate font-medium">
                      {customer.shortName ?? customer.legalName}
                    </span>
                    <span className="num shrink-0 text-xs text-muted-foreground">
                      {customer.customerNo}
                    </span>
                  </button>
                ))
              ) : (
                <p className="px-2 py-2 text-sm text-muted-foreground">
                  无客户匹配；按 Enter 搜索销售单
                </p>
              )}
            </div>
          ) : null}
        </div>
      }
    >
      {children}
    </ErpAppShell>
  )
}
