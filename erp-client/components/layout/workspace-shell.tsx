"use client"

import * as React from "react"
import { Building2Icon } from "lucide-react"

import { ErpAppShell } from "@/components/business"
import {
    WorkspaceSidebarAccount,
    WorkspaceSidebarNav,
} from "@/components/layout/workspace-sidebar-nav"

function WorkspaceSidebarHeader() {
    return (
        <div className="flex items-center gap-2.5 px-2 py-3">
            <div className="flex size-9 items-center justify-center rounded-xl bg-sidebar-primary text-sidebar-primary-foreground shadow-sm">
                <Building2Icon className="size-5" aria-hidden="true" />
            </div>
            <div className="min-w-0">
                <div className="truncate text-base font-bold tracking-tight text-sidebar-accent-foreground">
                    福尚云 ERP
                </div>
                <div className="truncate text-xs text-sidebar-foreground/70">
                    内部工作台
                </div>
            </div>
        </div>
    )
}

export function WorkspaceShell({ children }: { children: React.ReactNode }) {
    return (
        <ErpAppShell
            className="h-svh overflow-hidden"
            contentLabel="主工作区"
            sidebarCollapsible="none"
            showSidebarRail={false}
            sidebarHeader={<WorkspaceSidebarHeader />}
            sidebarContent={<WorkspaceSidebarNav />}
            sidebarFooter={<WorkspaceSidebarAccount />}
        >
            {children}
        </ErpAppShell>
    )
}
