"use client"

import * as React from "react"

import { ErpAppShell } from "@/components/business"
import { NavDeliveryOverlay } from "@/components/layout/nav-delivery-overlay"
import { WorkspaceSidebarProvider } from "@/components/layout/workspace-sidebar-context"
import { WorkspaceSidebarNav } from "@/components/layout/workspace-sidebar-nav"
import { WorkspaceTopNav } from "@/components/layout/workspace-top-nav"
import { isWorkspaceFrame } from "@/lib/workspace-tabs"
import { WorkspaceFrame } from "./workspace-frame"
import { WorkspaceTabs } from "./workspace-tabs"

export function WorkspaceShell({
    children,
    hostProvider: HostProvider,
}: {
    children: React.ReactNode
    /** 仅在顶层宿主工作区挂载的业务数据适配 Provider（子 iframe 帧不挂载，避免重复轮询）。 */
    hostProvider?: React.ComponentType<{ children: React.ReactNode }>
}) {
    const [embedded, setEmbedded] = React.useState<boolean | null>(null)
    React.useEffect(() => {
        setEmbedded(isWorkspaceFrame())
    }, [])
    if (embedded === null) return <div className="min-h-svh bg-background" />
    if (embedded) return <WorkspaceFrame>{children}</WorkspaceFrame>
    const content = (
        <WorkspaceTabs>
            {(tabbar, panels) => (
                <WorkspaceSidebarProvider>
                    <ErpAppShell
                        className="h-svh overflow-hidden"
                        contentLabel="主工作区"
                        sidebarCollapsible="icon"
                        showSidebarRail={true}
                        sidebarContentClassName="gap-0 overflow-hidden p-0"
                        sidebarContent={<WorkspaceSidebarNav />}
                        topbar={<WorkspaceTopNav tabs={tabbar} />}
                    >
                        {panels}
                        <NavDeliveryOverlay />
                    </ErpAppShell>
                </WorkspaceSidebarProvider>
            )}
        </WorkspaceTabs>
    )
    return HostProvider ? <HostProvider>{content}</HostProvider> : content
}
