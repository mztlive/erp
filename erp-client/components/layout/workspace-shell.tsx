"use client"

import * as React from "react"
import { Building2Icon } from "lucide-react"

import { ErpAppShell } from "@/components/business"
import {
    WorkspaceSidebarAccount,
    WorkspaceSidebarNav,
} from "@/components/layout/workspace-sidebar-nav"
import { NavDeliveryOverlay } from "@/components/layout/nav-delivery-overlay"
import { isWorkspaceFrame } from "@/lib/workspace-tabs"
import { WorkspaceFrame } from "./workspace-frame"
import { WorkspaceTabs } from "./workspace-tabs"

function WorkspaceSidebarHeader() {
    return (
        <div className="flex items-center gap-3 px-3 py-3">
            <div className="flex size-8 items-center justify-center rounded-lg bg-sidebar-primary text-sidebar-primary-foreground shadow-xs">
                <Building2Icon className="size-4.5" aria-hidden="true" />
            </div>
            <div className="min-w-0">
                <div className="truncate text-sm font-bold tracking-tight text-foreground">
                    福尚云 ERP
                </div>
                <div className="truncate text-[11px] font-medium text-muted-foreground">
                    经营协同工作台
                </div>
            </div>
        </div>
    )
}

export function WorkspaceShell({ children }: { children: React.ReactNode }) {
    const [embedded, setEmbedded] = React.useState<boolean | null>(null)
    React.useEffect(() => {
        setEmbedded(isWorkspaceFrame())
    }, [])
    if (embedded === null) return <div className="min-h-svh bg-background" />
    if (embedded) return <WorkspaceFrame>{children}</WorkspaceFrame>
    return (
        <WorkspaceTabs>
            {(tabbar, panels) => (
                <ErpAppShell
                    className="h-svh overflow-hidden"
                    contentLabel="主工作区"
                    sidebarCollapsible="none"
                    showSidebarRail={false}
                    sidebarHeader={<WorkspaceSidebarHeader />}
                    sidebarContent={<WorkspaceSidebarNav />}
                    sidebarFooter={<WorkspaceSidebarAccount />}
                    topbar={tabbar}
                >
                    {panels}
                    <NavDeliveryOverlay />
                </ErpAppShell>
            )}
        </WorkspaceTabs>
    )
}
