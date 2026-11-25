"use client"

import type { ReactNode } from "react"
import Link from "next/link"
import { usePathname } from "next/navigation"
import {
    Building2Icon,
    ChevronsUpDownIcon,
    ClipboardListIcon,
    FilePlus2Icon,
    HandshakeIcon,
    ListChecksIcon,
    LogOutIcon,
    PackageIcon,
    SettingsIcon,
    TagsIcon,
} from "lucide-react"

import { ErpAppShell } from "@/components/business/shell"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
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
    SidebarTrigger,
    useSidebar,
} from "@/components/ui/sidebar"
import type { PortalProfile } from "../types"

const navigationGroups = [
    {
        label: "供应协同",
        items: [
            { path: "offerings", label: "供货商品目录", icon: PackageIcon },
            { path: "quotes", label: "已有商品报价", icon: TagsIcon },
            { path: "new-products", label: "新品提报", icon: FilePlus2Icon },
            {
                path: "applications",
                label: "我的申请",
                icon: ClipboardListIcon,
            },
        ],
    },
    {
        label: "合作与账号",
        items: [
            { path: "cooperation", label: "合作资料", icon: HandshakeIcon },
            { path: "tasks", label: "导入导出记录", icon: ListChecksIcon },
            { path: "account", label: "账号设置", icon: SettingsIcon },
        ],
    },
] as const

function isActivePath(pathname: string, path: string) {
    const href = `/supplier-portal/${path}`
    return pathname === href || pathname.startsWith(`${href}/`)
}

function PortalSidebarNav({ canMaintain }: { canMaintain: boolean }) {
    const pathname = usePathname()
    const { isMobile, setOpenMobile } = useSidebar()

    return (
        <nav aria-label="供应商门户导航">
            {navigationGroups.map((group) => (
                <SidebarGroup key={group.label} className="gap-1 px-1 py-1">
                    <SidebarGroupLabel>{group.label}</SidebarGroupLabel>
                    <SidebarGroupContent>
                        <SidebarMenu className="gap-0.5">
                            {group.items
                                .filter(
                                    ({ path }) =>
                                        canMaintain || path !== "new-products",
                                )
                                .map(({ path, label, icon: Icon }) => {
                                    const active = isActivePath(pathname, path)
                                    return (
                                        <SidebarMenuItem key={path}>
                                            <SidebarMenuButton
                                                id={`supplier-portal-nav-${path}`}
                                                data-workspace-nav={
                                                    path === "tasks"
                                                        ? "supplier-portal-tasks"
                                                        : undefined
                                                }
                                                isActive={active}
                                                tooltip={label}
                                                render={
                                                    <Link
                                                        href={`/supplier-portal/${path}`}
                                                        aria-current={
                                                            active
                                                                ? "page"
                                                                : undefined
                                                        }
                                                    />
                                                }
                                                onClick={() => {
                                                    if (isMobile)
                                                        setOpenMobile(false)
                                                }}
                                            >
                                                <Icon aria-hidden="true" />
                                                <span>{label}</span>
                                            </SidebarMenuButton>
                                        </SidebarMenuItem>
                                    )
                                })}
                        </SidebarMenu>
                    </SidebarGroupContent>
                </SidebarGroup>
            ))}
        </nav>
    )
}

function PortalSidebarAccount({
    profile,
    onLogout,
}: {
    profile: PortalProfile
    onLogout: () => void
}) {
    const displayName = profile.name || profile.account
    return (
        <SidebarMenu>
            <SidebarMenuItem>
                <DropdownMenu>
                    <DropdownMenuTrigger
                        id="supplier-portal-account-menu"
                        render={
                            <SidebarMenuButton
                                size="lg"
                                aria-label="账号菜单"
                            />
                        }
                    >
                        <Avatar size="sm" className="size-7 rounded-md">
                            <AvatarFallback className="rounded-md bg-sidebar-primary text-xs font-semibold text-sidebar-primary-foreground">
                                {displayName.trim().slice(0, 1).toUpperCase() ||
                                    "供"}
                            </AvatarFallback>
                        </Avatar>
                        <span className="grid min-w-0 flex-1 text-left text-body-compact">
                            <span className="truncate font-medium">
                                {displayName}
                            </span>
                            <span className="truncate text-xs text-sidebar-foreground/70">
                                {profile.role === "read_only"
                                    ? "只读查看账号"
                                    : "业务经办人"}
                            </span>
                        </span>
                        <ChevronsUpDownIcon
                            className="ml-auto"
                            aria-hidden="true"
                        />
                    </DropdownMenuTrigger>
                    <DropdownMenuContent
                        side="top"
                        align="start"
                        className="min-w-56"
                    >
                        <DropdownMenuItem
                            id="supplier-portal-account-settings"
                            render={<Link href="/supplier-portal/account" />}
                        >
                            <SettingsIcon aria-hidden="true" />
                            账号设置
                        </DropdownMenuItem>
                        <DropdownMenuSeparator />
                        <DropdownMenuItem
                            id="supplier-portal-logout"
                            variant="destructive"
                            onClick={onLogout}
                        >
                            <LogOutIcon aria-hidden="true" />
                            退出登录
                        </DropdownMenuItem>
                    </DropdownMenuContent>
                </DropdownMenu>
            </SidebarMenuItem>
        </SidebarMenu>
    )
}

export function PortalShell({
    profile,
    onLogout,
    children,
}: {
    profile: PortalProfile
    onLogout: () => void
    children: ReactNode
}) {
    const pathname = usePathname()
    const currentItem = navigationGroups
        .flatMap((group) => [...group.items])
        .find((item) => isActivePath(pathname, item.path))
    const Icon = currentItem?.icon ?? PackageIcon

    return (
        <ErpAppShell
            className="h-svh overflow-hidden"
            contentLabel="供应商工作区"
            sidebarCollapsible="none"
            showSidebarRail={false}
            sidebarHeader={
                <div className="flex items-center gap-2.5 px-1 py-0.5">
                    <div className="flex size-7.5 items-center justify-center rounded-md bg-primary text-primary-foreground shadow-2xs">
                        <Building2Icon className="size-4" aria-hidden="true" />
                    </div>
                    <div className="min-w-0">
                        <div className="truncate text-body-compact font-semibold tracking-tight text-foreground">
                            福尚云 ERP
                        </div>
                        <div className="truncate text-xs font-medium text-muted-foreground">
                            供应商门户
                        </div>
                    </div>
                </div>
            }
            sidebarContent={
                <PortalSidebarNav canMaintain={profile.role === "maintainer"} />
            }
            sidebarFooter={
                <PortalSidebarAccount profile={profile} onLogout={onLogout} />
            }
            topbar={
                <header className="flex h-topbar shrink-0 items-center gap-3 border-b border-border/90 bg-card px-workspace-inline md:px-page-inline min-[75rem]:px-page-inline-lg">
                    <SidebarTrigger
                        id="supplier-portal-sidebar-trigger"
                        className="md:hidden"
                    />
                    <div className="flex h-control-sm min-w-0 items-center gap-1.5 rounded-md bg-sidebar-accent px-2.5 text-body-compact font-medium text-sidebar-accent-foreground">
                        <Icon
                            className="size-3.5 shrink-0 text-sidebar-accent-foreground"
                            aria-hidden="true"
                        />
                        <span className="truncate">
                            {currentItem?.label ?? "供应商门户"}
                        </span>
                    </div>
                    <span
                        className="ml-auto min-w-0 truncate text-xs text-muted-foreground"
                        title={profile.supplier_name}
                    >
                        {profile.supplier_name ?? "供应合作"}
                    </span>
                </header>
            }
            maintenanceBanner={
                profile.role === "read_only" ? (
                    <p
                        role="status"
                        className="shrink-0 border-b border-border bg-muted/40 px-page-inline py-2 text-xs text-muted-foreground md:px-page-inline-lg"
                    >
                        当前账号可查看资料；提交申请和更新可供请联系本供应商的业务经办人。
                    </p>
                ) : undefined
            }
        >
            {children}
        </ErpAppShell>
    )
}
