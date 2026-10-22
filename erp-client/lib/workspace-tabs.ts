import { WORKSPACE_NAV_GROUPS } from "@/lib/workspace-registry"
import type { NavDeliveryRequest } from "@/lib/nav-delivery"

export type WorkspaceFrameState = {
    href: string
    title: string
    dirty: boolean
    busy: boolean
}

export type WorkspaceHost = {
    closeCurrent: (source: Window | null) => void
    navigate: (source: Window | null, href: string, replace?: boolean) => void
    report: (source: Window, state: WorkspaceFrameState) => void
    changed: (source: Window) => void
    deliver: (source: Window, request: NavDeliveryRequest) => void
}

export const WORKSPACE_CLOSE_SHORTCUT = "Alt+Shift+W"

/** Reserved Cmd/Ctrl+W is best-effort only: browsers may never dispatch it. */
export function handleWorkspaceCloseShortcut(event: KeyboardEvent): void {
    if (
        event.defaultPrevented ||
        !event.cancelable ||
        event.isComposing ||
        event.keyCode === 229 ||
        event.getModifierState("AltGraph")
    )
        return
    const isW =
        event.key.toLowerCase() === "w" ||
        (event.altKey && event.code === "KeyW")
    if (!isW) return
    const applicationShortcut =
        event.altKey && event.shiftKey && !event.metaKey && !event.ctrlKey
    const nativeModifier = /Mac|iPhone|iPad|iPod/.test(navigator.platform)
        ? event.metaKey && !event.ctrlKey
        : event.ctrlKey && !event.metaKey
    const nativeShortcut = !event.altKey && !event.shiftKey && nativeModifier
    if (!applicationShortcut && !nativeShortcut) return
    const host = workspaceHost()
    const frame = isWorkspaceFrame()
    if (
        !host ||
        (frame &&
            window.frameElement?.getAttribute("data-workspace-active") !==
                "true")
    )
        return
    event.preventDefault()
    event.stopImmediatePropagation()
    // Holding the chord must never close a succession of documents.
    if (!event.repeat) host.closeCurrent(frame ? window : null)
}

declare global {
    interface Window {
        erpWorkspaceHost?: WorkspaceHost
        erpWorkspaceActivate?: (active: boolean) => void
        erpWorkspaceInvalidate?: () => void
        erpWorkspaceGo?: (href: string) => void
    }
}

/** Only our same-origin workspace frames participate in the navigation bridge. */
export function workspaceHost(): WorkspaceHost | undefined {
    if (typeof window === "undefined") return undefined
    try {
        if (window.frameElement?.hasAttribute("data-workspace-frame")) {
            return window.parent.erpWorkspaceHost
        }
        return window.erpWorkspaceHost
    } catch {
        return undefined
    }
}

export function isWorkspaceFrame(): boolean {
    if (typeof window === "undefined") return false
    try {
        return (
            window.frameElement?.hasAttribute("data-workspace-frame") === true
        )
    } catch {
        return false
    }
}

export function workspaceUrl(href: string, base: string): URL | null {
    const url = new URL(href, base)
    if (url.origin !== new URL(base).origin) return null
    url.pathname = url.pathname.replace(/\/+$/, "") || "/"
    const root = url.pathname.split("/")[1]
    return [
        "workspace",
        "sales",
        "procurement",
        "master-data",
        "inventory",
        "fulfillment",
        "finance",
        "system",
        "analytics",
        "supplier-api",
        "governance",
    ].includes(root)
        ? url
        : null
}

export function tabHref(url: URL): string {
    return `${url.pathname}${url.search}${url.hash}`
}

/** Filters belong to a tab; separately named fulfillment lanes belong to different tabs. */
export function workspaceTabKey(url: URL): string {
    const create =
        url.pathname.endsWith("/new") ||
        url.searchParams.get("mode") === "create" ||
        url.searchParams.get("action") === "create" ||
        url.searchParams.get("create") === "1"
    if (create) {
        const identity = new URLSearchParams({ mode: "create" })
        for (const name of [
            "customerId",
            "contractId",
            "contractRevisionId",
            "selectionProposalId",
            "inquiryId",
            "round",
            "salesOrderId",
            "workItemId",
            "purchaseOrderId",
            "copyFrom",
            "nature",
            "businessType",
        ]) {
            const value = url.searchParams.get(name)
            if (value) identity.set(name, value)
        }
        identity.sort()
        return `${url.pathname}?${identity}`
    }
    const lane =
        url.pathname === "/fulfillment" ? url.searchParams.get("lane") : null
    return `${url.pathname}${lane ? `?lane=${encodeURIComponent(lane)}` : ""}`
}

export function workspaceTabTitle(url: URL): string {
    const items = WORKSPACE_NAV_GROUPS.flatMap((group) => group.items)
    const match = items
        .filter((item) => {
            const candidate = new URL(item.href, url.origin)
            return (
                (url.pathname === candidate.pathname ||
                    url.pathname.startsWith(`${candidate.pathname}/`)) &&
                [...candidate.searchParams].every(
                    ([key, value]) => url.searchParams.get(key) === value,
                )
            )
        })
        .sort((a, b) => b.href.length - a.href.length)[0]
    if (url.pathname.endsWith("/new"))
        return `新建${match?.label.replace(/列表$/, "") ?? "单据"}`
    return match?.label ?? "业务页面"
}

/** Same-page URL updates stay inside the frame, preserving Next's normal filter semantics. */
export function navigateWorkspace(href: string, replace = false): boolean {
    const host = workspaceHost()
    if (!host) return false
    const target = workspaceUrl(href, window.location.href)
    if (!target) return false
    const frame = isWorkspaceFrame()
    if (
        frame &&
        workspaceTabKey(target) ===
            workspaceTabKey(new URL(window.location.href))
    )
        return false
    host.navigate(frame ? window : null, tabHref(target), replace)
    return true
}

/** Preserve normal new-window, download, external-link and in-page anchor behavior. */
export function interceptWorkspaceLink(event: MouseEvent): void {
    if (
        event.defaultPrevented ||
        event.button !== 0 ||
        event.ctrlKey ||
        event.metaKey ||
        event.shiftKey ||
        event.altKey
    )
        return
    const link =
        event.target instanceof Element ? event.target.closest("a[href]") : null
    if (
        !(link instanceof HTMLAnchorElement) ||
        link.hasAttribute("download") ||
        (link.target && link.target !== "_self")
    )
        return
    const destination = new URL(link.href)
    if (
        destination.pathname === window.location.pathname &&
        destination.search === window.location.search &&
        destination.hash
    )
        return
    if (!navigateWorkspace(link.href)) {
        const url = workspaceUrl(link.href, window.location.href)
        if (!isWorkspaceFrame() || !url || !window.erpWorkspaceGo) return
        window.erpWorkspaceGo(tabHref(url))
    }
    event.preventDefault()
    event.stopPropagation()
}
