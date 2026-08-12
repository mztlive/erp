"use client"

import { useRouter } from "next/navigation"
import { useQueryClient } from "@tanstack/react-query"
import { ChevronDownIcon, LogOutIcon } from "lucide-react"

import { Avatar, AvatarFallback } from "@/components/ui/avatar"
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

    const displayName =
        profileQuery.data?.name || profileQuery.data?.account || "已登录"
    const accountLabel = profileQuery.data?.account || "后台账号"

    return (
        <DropdownMenu>
            <DropdownMenuTrigger
                render={
                    <button
                        type="button"
                        className="flex h-9 items-center gap-1.5 rounded-2xl border border-border/40 bg-card/80 py-0 pr-2 pl-1 shadow-xs outline-none transition-colors hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring"
                        aria-label="账号菜单"
                    />
                }
            >
                <Avatar size="default" className="size-7 cursor-pointer">
                    <AvatarFallback className="bg-primary/10 text-primary">
                        {displayInitial(
                            profileQuery.data?.name,
                            profileQuery.data?.account,
                        )}
                    </AvatarFallback>
                </Avatar>
                <ChevronDownIcon
                    className="size-3.5 text-muted-foreground"
                    aria-hidden="true"
                />
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="min-w-48">
                <DropdownMenuGroup>
                    <DropdownMenuLabel>
                        <div className="flex flex-col gap-0.5">
                            <span className="text-sm font-medium text-foreground">
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
                    variant="destructive"
                    onClick={() => logoutAndRedirect(router, queryClient)}
                >
                    <LogOutIcon aria-hidden="true" />
                    退出登录
                </DropdownMenuItem>
            </DropdownMenuContent>
        </DropdownMenu>
    )
}
