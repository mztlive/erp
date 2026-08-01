"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter } from "next/navigation"
import {
  Building2Icon,
  ClipboardCheckIcon,
  LayoutDashboardIcon,
  Link2Icon,
  ListTodoIcon,
  ShoppingCartIcon,
  UsersIcon,
  XIcon,
} from "lucide-react"

import {
  ErpAppShell,
  GlobalTopbar,
  TaskTabs,
} from "@/components/business"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarSeparator,
} from "@/components/ui/sidebar"
import { isNavItemActive } from "@/lib/nav-active"
import { WORKSPACE_NAV_GROUPS } from "@/lib/workspace-registry"
import { OwnershipMigrationGlobalFreezeBanner } from "@/features/ownership-migration/global-freeze-banner"
import { useCustomerDirectoryQuery } from "@/features/customers/queries"

function AppSidebarNav() {
  const pathname = usePathname()
  const allHrefs = WORKSPACE_NAV_GROUPS.flatMap((group) =>
    group.items.map((item) => item.href)
  )

  return (
    <>
      {WORKSPACE_NAV_GROUPS.map((group, index) => (
        <SidebarGroup key={group.label}>
          {index > 0 ? <SidebarSeparator className="mx-0 mb-2" /> : null}
          <SidebarGroupLabel>{group.label}</SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu>
              {group.items.map((item) => {
                const Icon = item.icon
                const isActive = isNavItemActive(pathname, item.href, allHrefs)

                return (
                  <SidebarMenuItem key={`${group.label}-${item.href}`}>
                    <SidebarMenuButton
                      isActive={isActive}
                      tooltip={item.label}
                      render={<Link href={item.href} />}
                    >
                      <Icon aria-hidden="true" />
                      <span>{item.label}</span>
                      {item.badge ? (
                        <Badge
                          variant="secondary"
                          className="ml-auto group-data-[collapsible=icon]:hidden"
                        >
                          {item.badge}
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

function resolveSalesOrderObjectId(pathname: string): string | null {
  const match = pathname.match(/^\/sales\/orders\/([^/]+)$/)
  return match?.[1] ?? null
}

function resolveSupplierConnectionId(pathname: string): string | null {
  const match = pathname.match(/^\/supplier-api\/connections\/([^/]+)$/)
  return match?.[1] ?? null
}

function resolveCustomerObjectId(pathname: string): string | null {
  const match = pathname.match(/^\/sales\/customers\/([^/]+)$/)
  return match?.[1] ?? null
}

type OpenObjectTab = {
  value: string
  label: string
  href: string
  returnHref: string
  kind:
    | "customer"
    | "sales-order"
    | "supplier-connection"
    | "analysis-drilldown"
}

const OBJECT_TABS_STORAGE_KEY = "erp.open-object-tabs.v1"

function fallbackObjectTab(
  pathname: string,
  search: string,
  returnHref: string
): OpenObjectTab | null {
  const params = new URLSearchParams(search)
  const explicitReturnHref = params.get("returnTo")
  const safeReturnHref =
    explicitReturnHref?.startsWith("/") && !explicitReturnHref.startsWith("//")
      ? explicitReturnHref
      : returnHref
  const customerName = params.get("customerName")
  const customerContextId = params.get("customerId") ?? params.get("search")
  const fromW15 = params.get("from") === "W15" || params.get("source") === "W15"

  if (fromW15 && pathname === "/sales/orders" && customerContextId) {
    return {
      value: `analysis-drilldown:w05:${customerContextId}`,
      label: `销售构成 · ${customerName ?? customerContextId}`,
      href: `${pathname}${search ? `?${search}` : ""}`,
      returnHref: safeReturnHref,
      kind: "analysis-drilldown",
    }
  }

  if (fromW15 && pathname === "/finance/customer-accounts" && customerContextId) {
    return {
      value: `analysis-drilldown:w11:${customerContextId}`,
      label: `逾期应收 · ${customerName ?? customerContextId}`,
      href: `${pathname}${search ? `?${search}` : ""}`,
      returnHref: safeReturnHref,
      kind: "analysis-drilldown",
    }
  }

  if (fromW15 && pathname === "/analytics/profit-loss" && customerContextId) {
    return {
      value: `analysis-drilldown:w16:${customerContextId}`,
      label: `实际盈亏 · ${customerName ?? customerContextId}`,
      href: `${pathname}${search ? `?${search}` : ""}`,
      returnHref: safeReturnHref,
      kind: "analysis-drilldown",
    }
  }

  const customerId = resolveCustomerObjectId(pathname)
  if (customerId) {
    return {
      value: `customer:${customerId}`,
      label: `客户 · ${customerName ?? customerId}`,
      href: `/sales/customers/${customerId}${search ? `?${search}` : ""}`,
      returnHref: safeReturnHref,
      kind: "customer",
    }
  }

  const salesOrderId = resolveSalesOrderObjectId(pathname)
  if (salesOrderId) {
    return {
      value: `sales-order:${salesOrderId}`,
      label: salesOrderId,
      href: `/sales/orders/${salesOrderId}${search ? `?${search}` : ""}`,
      returnHref: safeReturnHref,
      kind: "sales-order",
    }
  }

  const connectionId =
    resolveSupplierConnectionId(pathname) ??
    (pathname.startsWith("/supplier-api/connections")
      ? new URLSearchParams(search).get("connectionId") ??
        new URLSearchParams(search).get("id")
      : null)
  if (connectionId) {
    return {
      value: `supplier-connection:${connectionId}`,
      label: connectionId.replace(/^conn_/, "").toUpperCase(),
      href: resolveSupplierConnectionId(pathname)
        ? `/supplier-api/connections/${connectionId}${search ? `?${search}` : ""}`
        : `/supplier-api/connections?connectionId=${encodeURIComponent(connectionId)}`,
      returnHref: safeReturnHref,
      kind: "supplier-connection",
    }
  }

  return null
}

function resolveTaskTab(pathname: string, search: string): string {
  const params = new URLSearchParams(search)
  const fromW15 = params.get("from") === "W15" || params.get("source") === "W15"
  const customerContextId = params.get("customerId") ?? params.get("search")
  if (fromW15 && customerContextId) {
    if (pathname === "/sales/orders") {
      return `analysis-drilldown:w05:${customerContextId}`
    }
    if (pathname === "/finance/customer-accounts") {
      return `analysis-drilldown:w11:${customerContextId}`
    }
    if (pathname === "/analytics/profit-loss") {
      return `analysis-drilldown:w16:${customerContextId}`
    }
  }
  const customerId = resolveCustomerObjectId(pathname)
  if (customerId) return `customer:${customerId}`
  const salesOrderId = resolveSalesOrderObjectId(pathname)
  if (salesOrderId) return `sales-order:${salesOrderId}`
  const connFromPath = resolveSupplierConnectionId(pathname)
  if (connFromPath) return `supplier-connection:${connFromPath}`
  if (pathname.startsWith("/supplier-api/connections")) {
    const params = new URLSearchParams(search.startsWith("?") ? search.slice(1) : search)
    const connId = params.get("connectionId") ?? params.get("id")
    if (connId) return `supplier-connection:${connId}`
    return "supplier-connections"
  }
  if (pathname.startsWith("/sales/orders")) return "sales-orders"
  if (pathname.startsWith("/procurement/confirm")) return "procurement-confirm"
  if (pathname.startsWith("/workspace/tasks")) return "tasks"
  if (pathname.startsWith("/workspace")) return "workspace"
  if (pathname.startsWith("/fulfillment")) return "fulfillment"
  return "workspace"
}

export function WorkspaceShell({ children }: { children: React.ReactNode }) {
  const pathname = usePathname()
  const router = useRouter()
  const [search, setSearch] = React.useState("")
  const [searchFocused, setSearchFocused] = React.useState(false)
  const [sidebarOpen, setSidebarOpen] = React.useState(false)
  const [openObjectTabs, setOpenObjectTabs] = React.useState<OpenObjectTab[]>([])
  const openObjectTabsRef = React.useRef<OpenObjectTab[]>([])
  const closingTabRef = React.useRef<string | null>(null)
  const previousLocationRef = React.useRef("/workspace")
  const [locationSearch, setLocationSearch] = React.useState("")
  const activeTab = resolveTaskTab(pathname, locationSearch)
  const customerSearchQuery = useCustomerDirectoryQuery({
    scope: "team",
    status: "all",
    query: search.trim(),
  })
  const customerMatches = React.useMemo(
    () =>
      search.trim().length >= 2
        ? customerSearchQuery.data?.items.slice(0, 5) ?? []
        : [],
    [customerSearchQuery.data?.items, search]
  )

  React.useEffect(() => {
    const nextSearch = window.location.search.replace(/^\?/, "")
    setLocationSearch((current) =>
      current === nextSearch ? current : nextSearch
    )
  }, [children, pathname])

  const persistObjectTabs = React.useCallback((tabs: OpenObjectTab[]) => {
    window.sessionStorage.setItem(OBJECT_TABS_STORAGE_KEY, JSON.stringify(tabs))
  }, [])

  const rememberObjectTab = React.useCallback(
    (tab: OpenObjectTab) => {
      const current = openObjectTabsRef.current
      const existing = current.find((item) => item.value === tab.value)
      const next = existing
        ? current.map((item) =>
            item.value === tab.value
              ? {
                  ...item,
                  href: tab.href,
                  returnHref: tab.returnHref,
                  label:
                    tab.label.includes("cust_") && !item.label.includes("cust_")
                      ? item.label
                      : tab.label,
                }
              : item
          )
        : [...current, tab].slice(-8)
      openObjectTabsRef.current = next
      persistObjectTabs(next)
      setOpenObjectTabs(next)
    },
    [persistObjectTabs]
  )

  React.useEffect(() => {
    try {
      const raw = window.sessionStorage.getItem(OBJECT_TABS_STORAGE_KEY)
      if (!raw) return
      const parsed = JSON.parse(raw) as unknown
      if (!Array.isArray(parsed)) return
      const valid = parsed.filter(
        (item): item is OpenObjectTab =>
          typeof item === "object" &&
          item !== null &&
          "value" in item &&
          typeof item.value === "string" &&
          "label" in item &&
          typeof item.label === "string" &&
          "href" in item &&
          typeof item.href === "string" &&
          "returnHref" in item &&
          typeof item.returnHref === "string" &&
          "kind" in item &&
          (item.kind === "customer" ||
            item.kind === "sales-order" ||
            item.kind === "supplier-connection" ||
            item.kind === "analysis-drilldown")
      )
      const restored = valid.slice(-8)
      openObjectTabsRef.current = restored
      setOpenObjectTabs(restored)
    } catch {
      window.sessionStorage.removeItem(OBJECT_TABS_STORAGE_KEY)
    }
  }, [])

  React.useEffect(() => {
    const currentHref = `${pathname}${locationSearch ? `?${locationSearch}` : ""}`
    const fallbackReturnHref = pathname.startsWith("/sales/customers/")
      ? "/sales/customers"
      : pathname.startsWith("/sales/orders/")
        ? "/sales/orders"
        : "/supplier-api/connections"
    const returnHref = previousLocationRef.current.startsWith(pathname)
      ? fallbackReturnHref
      : previousLocationRef.current
    const currentObject = fallbackObjectTab(pathname, locationSearch, returnHref)
    if (currentObject && closingTabRef.current !== currentObject.value) {
      rememberObjectTab(currentObject)
    }
    if (!currentObject || closingTabRef.current !== currentObject.value) {
      closingTabRef.current = null
    }
    previousLocationRef.current = currentHref
  }, [locationSearch, pathname, rememberObjectTab])

  React.useEffect(() => {
    const media = window.matchMedia("(min-width: 1280px)")
    const syncSidebar = () => setSidebarOpen(media.matches)
    syncSidebar()
    media.addEventListener("change", syncSidebar)
    return () => media.removeEventListener("change", syncSidebar)
  }, [])

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
      rememberObjectTab({
        value: `customer:${exactCustomer.id}`,
        label: `客户 · ${exactCustomer.shortName ?? exactCustomer.legalName}`,
        href: `/sales/customers/${exactCustomer.id}`,
        returnHref: `${pathname}${locationSearch ? `?${locationSearch}` : ""}`,
        kind: "customer",
      })
      router.push(`/sales/customers/${exactCustomer.id}`)
      setSearchFocused(false)
      return
    }
    router.push(`/sales/orders?search=${encodeURIComponent(query)}`)
    setSearchFocused(false)
  }, [customerMatches, locationSearch, pathname, rememberObjectTab, router, search])

  const openCustomer = React.useCallback(
    (customer: (typeof customerMatches)[number]) => {
      rememberObjectTab({
        value: `customer:${customer.id}`,
        label: `客户 · ${customer.shortName ?? customer.legalName}`,
        href: `/sales/customers/${customer.id}`,
        returnHref: `${pathname}${locationSearch ? `?${locationSearch}` : ""}`,
        kind: "customer",
      })
      router.push(`/sales/customers/${customer.id}`)
      setSearch("")
      setSearchFocused(false)
    },
    [locationSearch, pathname, rememberObjectTab, router]
  )

  const activeObjectTab = openObjectTabs.find((tab) => tab.value === activeTab)

  const closeActiveObjectTab = React.useCallback(() => {
    if (!activeObjectTab) return
    closingTabRef.current = activeObjectTab.value
    const next = openObjectTabsRef.current.filter(
      (tab) => tab.value !== activeObjectTab.value
    )
    openObjectTabsRef.current = next
    persistObjectTabs(next)
    setOpenObjectTabs(next)
    router.push(activeObjectTab.returnHref)
  }, [activeObjectTab, persistObjectTabs, router])

  return (
    <ErpAppShell
      className="min-h-svh"
      contentLabel="主工作区"
      sidebarOpen={sidebarOpen}
      onSidebarOpenChange={setSidebarOpen}
      sidebarHeader={
        <div className="flex items-center gap-2 px-2 py-1.5">
          <div className="flex size-8 items-center justify-center rounded-md bg-sidebar-primary text-sidebar-primary-foreground">
            <Building2Icon className="size-4" aria-hidden="true" />
          </div>
          <div className="min-w-0 group-data-[collapsible=icon]:hidden">
            <div className="truncate text-sm font-semibold text-sidebar-accent-foreground">
              员工福利 ERP
            </div>
            <div className="truncate text-xs text-sidebar-foreground/70">
              演示环境
            </div>
          </div>
        </div>
      }
      sidebarContent={<AppSidebarNav />}
      sidebarFooter={
        <div className="flex items-center gap-2 px-2 py-1.5 group-data-[collapsible=icon]:justify-center">
          <Avatar size="sm">
            <AvatarFallback>王</AvatarFallback>
          </Avatar>
          <div className="min-w-0 group-data-[collapsible=icon]:hidden">
            <div className="truncate text-sm text-sidebar-accent-foreground">
              王敏
            </div>
            <div className="truncate text-xs text-sidebar-foreground/70">
              销售
            </div>
          </div>
        </div>
      }
      topbar={
        <div className="relative">
          <GlobalTopbar
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
                badge: { label: "18", variant: "secondary" },
                onClick: () => router.push("/workspace/tasks"),
              },
            ]}
            trailing={
              <Avatar size="sm">
                <AvatarFallback>王</AvatarFallback>
              </Avatar>
            }
          />
          {searchFocused && search.trim().length >= 2 ? (
            <div
              role="listbox"
              aria-label="客户搜索结果"
              className="absolute left-14 top-[calc(100%-0.25rem)] z-50 w-[min(24rem,calc(100vw-5rem))] rounded-md border bg-popover p-1 text-popover-foreground shadow-md"
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
                    className="flex w-full items-center justify-between gap-3 rounded-sm px-2 py-2 text-left text-sm hover:bg-accent focus-visible:bg-accent focus-visible:outline-none"
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
      maintenanceBanner={<OwnershipMigrationGlobalFreezeBanner />}
      taskTabs={
        <TaskTabs
          value={activeTab}
          onValueChange={(value) => {
            const objectTab = openObjectTabs.find((tab) => tab.value === value)
            if (objectTab) {
              router.push(objectTab.href)
            } else if (value.startsWith("sales-order:")) {
              router.push(`/sales/orders/${value.slice("sales-order:".length)}`)
            } else if (value.startsWith("supplier-connection:")) {
              router.push(
                `/supplier-api/connections/${value.slice("supplier-connection:".length)}`
              )
            } else if (value === "supplier-connections") {
              router.push("/supplier-api/connections")
            } else if (value === "sales-orders") router.push("/sales/orders")
            else if (value === "procurement-confirm") {
              router.push("/procurement/confirm")
            } else if (value === "tasks") {
              router.push("/workspace/tasks")
            } else if (value === "fulfillment") {
              router.push("/fulfillment")
            } else router.push("/workspace")
          }}
          trailing={
            activeObjectTab ? (
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                aria-label={`关闭页签 ${activeObjectTab.label}`}
                onClick={closeActiveObjectTab}
              >
                <XIcon aria-hidden="true" />
              </Button>
            ) : null
          }
          items={[
            {
              value: "workspace",
              label: "今日工作台",
              icon: LayoutDashboardIcon,
            },
            {
              value: "tasks",
              label: "待办队列",
              icon: ListTodoIcon,
              badge:
                activeTab === "tasks"
                  ? { label: "当前", variant: "secondary" }
                  : { label: "18", variant: "secondary" },
            },
            {
              value: "sales-orders",
              label: "销售单列表",
              icon: ShoppingCartIcon,
              badge:
                activeTab === "sales-orders"
                  ? { label: "当前", variant: "secondary" }
                  : undefined,
            },
            ...openObjectTabs.map((tab) => ({
              value: tab.value,
              label: tab.label,
              icon:
                tab.kind === "customer"
                  ? UsersIcon
                  : tab.kind === "sales-order"
                    ? ShoppingCartIcon
                    : tab.kind === "analysis-drilldown"
                      ? LayoutDashboardIcon
                      : Link2Icon,
              badge: {
                label: tab.kind === "analysis-drilldown" ? "下钻" : "对象",
                variant: "secondary" as const,
              },
            })),
            {
              value: "supplier-connections",
              label: "API 连接",
              icon: Link2Icon,
              badge:
                activeTab === "supplier-connections"
                  ? { label: "当前", variant: "secondary" as const }
                  : undefined,
            },
            {
              value: "procurement-confirm",
              label: "二次确认",
              icon: ClipboardCheckIcon,
              badge:
                activeTab === "procurement-confirm"
                  ? { label: "3", variant: "secondary" }
                  : undefined,
            },
          ]}
        />
      }
    >
      {children}
    </ErpAppShell>
  )
}
