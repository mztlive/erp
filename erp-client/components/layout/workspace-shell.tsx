"use client"

import * as React from "react"
import { Building2Icon } from "lucide-react"

import { ErpAppShell } from "@/components/business"
import { WorkspaceSidebarNav } from "@/components/layout/workspace-sidebar-nav"
import { WorkspaceTopNav } from "@/components/layout/workspace-top-nav"
import { NavDeliveryOverlay } from "@/components/layout/nav-delivery-overlay"
import { isWorkspaceFrame } from "@/lib/workspace-tabs"
import { WorkspaceQuickSearch } from "./workspace-quick-search"
import { WorkspaceFrame } from "./workspace-frame"
import { WorkspaceTabs } from "./workspace-tabs"

function WorkspaceSidebarHeader() {
    return (
        <div className="flex flex-col gap-2">
            <div className="flex items-center gap-2.5 px-1 py-0.5 group-data-[collapsible=icon]:justify-center group-data-[collapsible=icon]:px-0">
                <div className="flex size-7.5 shrink-0 items-center justify-center rounded-md bg-primary text-primary-foreground shadow-2xs">
                    <Building2Icon className="size-4" aria-hidden="true" />
                </div>
                <div className="min-w-0 group-data-[collapsible=icon]:hidden">
                    <div className="truncate text-sm font-semibold tracking-tight text-foreground">
                        福尚云 ERP
                    </div>
                    <div className="truncate text-2xs font-medium text-muted-foreground">
                        经营协同工作台
                    </div>
                </div>
            </div>
            <WorkspaceQuickSearch />
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
                    sidebarCollapsible="icon"
                    showSidebarRail={true}
                    sidebarHeader={<WorkspaceSidebarHeader />}
                    sidebarContent={<WorkspaceSidebarNav />}
                    topbar={<WorkspaceTopNav tabs={tabbar} />}
                >
                    {panels}
                    <NavDeliveryOverlay />
                </ErpAppShell>
            )}
        </WorkspaceTabs>
    )
}
