"use client"

import Link from "next/link"
import { useQueryClient } from "@tanstack/react-query"
import { ChevronDownIcon, ListChecksIcon, LogOutIcon } from "lucide-react"

import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuGroup,
    DropdownMenuItem,
    DropdownMenuLabel,
    DropdownMenuSeparator,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { logoutAndRedirect } from "@/components/providers/auth-session-provider"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useRouter } from "@/lib/navigation"
import { hasPermission } from "@/lib/permissions"

function displayInitial(
    name: string | undefined,
    account: string | undefined,
): string {
    const source = (name || account || "用").trim()
    return source.slice(0, 1).toUpperCase() || "用"
}

export function WorkspaceAccountMenu() {
    const router = useRouter()
    const queryClient = useQueryClient()
    const profileQuery = useAccountProfileQuery()

    const canViewBackgroundJobs = hasPermission(
        profileQuery.data?.permissions,
        "background_job:list",
    )

    const displayName =
        profileQuery.data?.name || profileQuery.data?.account || "已登录"
    const accountLabel = profileQuery.data?.account || "后台账号"

    return (
        <div className="flex min-w-0 items-center gap-1">
            {profileQuery.isSuccess && !canViewBackgroundJobs && (
                <div className="min-w-0">
                    <Button
                        id="workspace-topbar-file-tasks"
                        data-workspace-nav="account-data-transfers"
                        render={<Link href="/account/data-transfers" />}
                        variant="ghost"
                        size="icon-sm"
                        className="text-muted-foreground hover:text-foreground"
                        aria-label="我的文件任务"
                        title="我的文件任务"
                    >
                        <ListChecksIcon className="size-4" aria-hidden="true" />
                    </Button>
                </div>
            )}
            <span
                aria-hidden="true"
                className="mx-1 hidden h-4 w-px shrink-0 bg-border sm:block"
            />
            <div className="min-w-0">
                <DropdownMenu>
                    <DropdownMenuTrigger
                        id="workspace-topbar-account-trigger"
                        render={
                            <Button
                                variant="ghost"
                                className="h-control-sm max-w-48 gap-2 pr-2 pl-1.5"
                                aria-label="账号菜单"
                            />
                        }
                    >
                        <Avatar size="sm">
                            <AvatarFallback className="bg-sidebar-accent text-xs font-medium text-sidebar-accent-foreground">
                                {displayInitial(
                                    profileQuery.data?.name,
                                    profileQuery.data?.account,
                                )}
                            </AvatarFallback>
                        </Avatar>
                        <span className="hidden min-w-0 text-left text-body-compact leading-tight sm:block">
                            <span className="block truncate font-medium">
                                {displayName}
                            </span>
                        </span>
                        <ChevronDownIcon
                            className="size-3.5 text-muted-foreground"
                            aria-hidden="true"
                        />
                    </DropdownMenuTrigger>
                    <DropdownMenuContent
                        side="bottom"
                        align="end"
                        className="min-w-56"
                    >
                        <DropdownMenuGroup>
                            <DropdownMenuLabel>
                                <div className="flex flex-col gap-0.5">
                                    <span className="text-body-compact font-medium text-foreground">
                                        {displayName}
                                    </span>
                                    <span className="text-xs font-normal text-muted-foreground">
                                        {accountLabel}
                                    </span>
                                </div>
                            </DropdownMenuLabel>
                        </DropdownMenuGroup>
                        <DropdownMenuSeparator />
                        <DropdownMenuItem
                            id="workspace-topbar-home-settings"
                            onClick={() => router.push("/account/workbench")}
                        >
                            个人首页设置
                        </DropdownMenuItem>
                        <DropdownMenuItem
                            id="workspace-topbar-my-transfers"
                            onClick={() =>
                                router.push("/account/data-transfers")
                            }
                        >
                            我的文件任务
                        </DropdownMenuItem>
                        <DropdownMenuItem
                            id="workspace-topbar-account-logout"
                            variant="destructive"
                            onClick={() =>
                                logoutAndRedirect(router, queryClient)
                            }
                        >
                            <LogOutIcon aria-hidden="true" />
                            退出登录
                        </DropdownMenuItem>
                    </DropdownMenuContent>
                </DropdownMenu>
            </div>
        </div>
    )
}
