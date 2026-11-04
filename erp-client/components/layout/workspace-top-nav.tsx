"use client"

import { SidebarTrigger } from "@/components/ui/sidebar"
import { WorkspaceAccountMenu } from "./workspace-account-menu"

export function WorkspaceTopNav() {
    return (
        <header
            data-slot="workspace-top-nav"
            className="flex h-15 shrink-0 items-center gap-3 border-b border-border bg-card px-workspace-inline md:px-page-inline min-[75rem]:px-page-inline-lg"
        >
            <SidebarTrigger
                id="workspace-topbar-navigation-trigger"
                className="md:hidden"
            />
            <span className="truncate text-sm font-semibold md:hidden">
                福尚云 ERP
            </span>
            <div className="min-w-0 flex-1" />
            <nav aria-label="全局功能" className="min-w-0 shrink-0">
                <WorkspaceAccountMenu />
            </nav>
        </header>
    )
}
