"use client"

import Link from "next/link"
import { usePathname } from "next/navigation"
import {
  BarChart3Icon,
  BoxesIcon,
  Building2Icon,
  ClipboardCheckIcon,
  FileTextIcon,
  LayoutDashboardIcon,
  PackageIcon,
  ReceiptIcon,
  Settings2Icon,
  ShoppingCartIcon,
  UsersIcon,
  WalletIcon,
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
  enabled?: boolean
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
        href: "/workspace/todos",
        label: "待办队列",
        icon: ClipboardCheckIcon,
        badge: "6",
        enabled: false,
      },
    ],
  },
  {
    label: "销售",
    items: [
      {
        href: "/sales/customers",
        label: "客户中心",
        icon: UsersIcon,
        enabled: false,
      },
      {
        href: "/sales/contracts",
        label: "合同",
        icon: FileTextIcon,
        enabled: false,
      },
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
        enabled: false,
      },
      {
        href: "/procurement/orders",
        label: "采购单",
        icon: PackageIcon,
        enabled: false,
      },
      {
        href: "/fulfillment",
        label: "履约作业",
        icon: BoxesIcon,
        enabled: false,
      },
    ],
  },
  {
    label: "票款",
    items: [
      {
        href: "/finance/ar",
        label: "客户往来",
        icon: WalletIcon,
        enabled: false,
      },
      {
        href: "/finance/ap",
        label: "供应商往来",
        icon: ReceiptIcon,
        enabled: false,
      },
    ],
  },
  {
    label: "经营",
    items: [
      {
        href: "/analytics/customers",
        label: "客户经营质量",
        icon: BarChart3Icon,
        enabled: false,
      },
    ],
  },
  {
    label: "系统",
    items: [
      {
        href: "/system/settings",
        label: "权限与审计",
        icon: Settings2Icon,
        enabled: false,
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
                  item.enabled !== false &&
                  (pathname === item.href ||
                    pathname.startsWith(`${item.href}/`))
                const disabled = item.enabled === false

                return (
                  <SidebarMenuItem key={item.href}>
                    <SidebarMenuButton
                      isActive={isActive}
                      tooltip={item.label}
                      disabled={disabled}
                      render={
                        disabled ? undefined : (
                          <Link href={item.href} />
                        )
                      }
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
  const isSalesOrders = pathname.startsWith("/sales/orders")

  return (
    <ErpAppShell
      className="min-h-svh"
      contentLabel="主工作区"
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
            defaultValue: "",
          }}
          actions={[
            {
              actionKey: "todos",
              label: "待办",
              badge: { label: "6", variant: "secondary" },
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
          defaultValue={isSalesOrders ? "sales-orders" : "workspace"}
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
              badge: isSalesOrders
                ? { label: "当前", variant: "secondary" }
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
