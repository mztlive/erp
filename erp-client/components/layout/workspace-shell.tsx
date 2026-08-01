"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter } from "next/navigation"
import {
  Building2Icon,
  ClipboardCheckIcon,
  LayoutDashboardIcon,
  ShoppingCartIcon,
  type LucideIcon,
} from "lucide-react"

import {
  ErpAppShell,
  GlobalTopbar,
  TaskTabs,
} from "@/components/business"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Badge } from "@/components/ui/badge"
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarSeparator,
} from "@/components/ui/sidebar"

type NavItem = {
  href: string
  label: string
  icon: LucideIcon
  badge?: string
}

type NavGroup = {
  label: string
  items: readonly NavItem[]
}

const NAV_GROUPS: readonly NavGroup[] = [
  {
    label: "工作",
    items: [
      { href: "/workspace", label: "今日工作台", icon: LayoutDashboardIcon },
      {
        href: "/procurement/confirm",
        label: "采购待办",
        icon: ClipboardCheckIcon,
        badge: "3",
      },
    ],
  },
  {
    label: "销售",
    items: [
      {
        href: "/sales/orders",
        label: "销售单",
        icon: ShoppingCartIcon,
      },
    ],
  },
  {
    label: "采购与履约",
    items: [
      {
        href: "/procurement/confirm",
        label: "二次确认",
        icon: ClipboardCheckIcon,
      },
    ],
  },
]

function AppSidebarNav() {
  const pathname = usePathname()

  return (
    <>
      {NAV_GROUPS.map((group, index) => (
        <SidebarGroup key={group.label}>
          {index > 0 ? <SidebarSeparator className="mx-0 mb-2" /> : null}
          <SidebarGroupLabel>{group.label}</SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu>
              {group.items.map((item) => {
                const Icon = item.icon
                const isActive =
                  (pathname === item.href ||
                    pathname.startsWith(`${item.href}/`))

                return (
                  <SidebarMenuItem key={item.href}>
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

export function WorkspaceShell({ children }: { children: React.ReactNode }) {
  const pathname = usePathname()
  const router = useRouter()
  const [search, setSearch] = React.useState("")
  const [sidebarOpen, setSidebarOpen] = React.useState(false)
  const activeTab = pathname.startsWith("/sales/orders")
    ? "sales-orders"
    : pathname.startsWith("/procurement/confirm")
      ? "procurement-confirm"
      : "workspace"

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
    router.push(`/sales/orders?search=${encodeURIComponent(query)}`)
  }, [router, search])

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
        <GlobalTopbar
          search={{
            ariaLabel: "全局搜索",
            placeholder: "单号、客户、合同…",
            shortcut: "⌘K",
            value: search,
            onChange: (event) => setSearch(event.target.value),
            onKeyDown: (event) => {
              if (event.key === "Enter") submitSearch()
            },
          }}
          actions={[
            {
              actionKey: "todos",
              label: "待办",
              badge: { label: "3", variant: "secondary" },
              onClick: () => router.push("/procurement/confirm"),
            },
          ]}
          trailing={
            <Avatar size="sm">
              <AvatarFallback>王</AvatarFallback>
            </Avatar>
          }
        />
      }
      taskTabs={
        <TaskTabs
          value={activeTab}
          onValueChange={(value) => {
            if (value === "sales-orders") router.push("/sales/orders")
            else if (value === "procurement-confirm") {
              router.push("/procurement/confirm")
            } else router.push("/workspace")
          }}
          items={[
            {
              value: "workspace",
              label: "今日工作台",
              icon: LayoutDashboardIcon,
            },
            {
              value: "sales-orders",
              label: "销售单",
              icon: ShoppingCartIcon,
              badge: activeTab === "sales-orders"
                ? { label: "当前", variant: "secondary" }
                : undefined,
            },
            {
              value: "procurement-confirm",
              label: "二次确认",
              icon: ClipboardCheckIcon,
              badge: activeTab === "procurement-confirm"
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
