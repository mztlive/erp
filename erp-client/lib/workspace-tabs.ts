import { WORKSPACE_NAV_GROUPS } from "@/lib/workspace-registry"
import { normalizeWorkspaceNavPathname } from "@/lib/nav-active"
import type { NavDeliveryRequest } from "@/lib/nav-delivery"

export type WorkspaceViewportDensity = "comfortable" | "compact" | "dense"

export type WorkspaceFrameState = {
    href: string
    title: string
    dirty: boolean
    busy: boolean
}

export type WorkspaceOpenTabSnapshot = Readonly<
    WorkspaceFrameState & {
        id: string
        key: string
        ready: boolean
        active: boolean
    }
>

export const WORKSPACE_OPEN_TABS_EVENT = "erp-workspace-open-tabs-change"

export function resolveWorkspaceViewportDensity(
    height: number,
    width?: number,
): WorkspaceViewportDensity {
    if (width !== undefined && width < 768) return "comfortable"
    if (height <= 720) return "dense"
    if (height <= 960) return "compact"
    return "comfortable"
}

export function applyWorkspaceViewportDensity(
    doc: Document | null | undefined,
    density: WorkspaceViewportDensity,
): void {
    const root = doc?.documentElement
    if (!root) return
    if (root.getAttribute("data-viewport-density") !== density) {
        root.setAttribute("data-viewport-density", density)
    }
}

export type WorkspaceHost = {
    closeCurrent: (source: Window | null, fallbackHref?: string) => void
    navigate: (source: Window | null, href: string, replace?: boolean) => void
    report: (source: Window, state: WorkspaceFrameState) => void
    changed: (source: Window) => void
    refreshed: (source: Window, revision: number) => void
    deliver: (source: Window, request: NavDeliveryRequest) => void
    openQuickSearch?: () => void
    getOpenTabs?: () => readonly WorkspaceOpenTabSnapshot[]
    getViewportDensity?: () => WorkspaceViewportDensity
}

export function subscribeWorkspaceOpenTabs(listener: () => void): () => void {
    if (typeof window === "undefined") return () => {}
    let target: Window = window
    try {
        if (window.frameElement?.hasAttribute("data-workspace-frame")) {
            target = window.parent
        }
    } catch {
        target = window
    }
    target.addEventListener(WORKSPACE_OPEN_TABS_EVENT, listener)
    return () => target.removeEventListener(WORKSPACE_OPEN_TABS_EVENT, listener)
}

export const WORKSPACE_CLOSE_SHORTCUT = "Alt+Shift+W"

export function isWorkspaceQuickSearchShortcut(event: KeyboardEvent): boolean {
    return (
        typeof event.key === "string" &&
        !event.defaultPrevented &&
        !event.isComposing &&
        !event.altKey &&
        !event.shiftKey &&
        (event.metaKey === true || event.ctrlKey === true) &&
        event.key.toLowerCase() === "k"
    )
}

/** Reserved Cmd/Ctrl+W is best-effort only: browsers may never dispatch it. */
export function handleWorkspaceCloseShortcut(event: KeyboardEvent): void {
    if (
        typeof event.key !== "string" ||
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
        erpWorkspaceInvalidate?: (revision: number) => void
        erpWorkspaceGo?: (href: string) => void
        erpWorkspaceSetDensity?: (density: WorkspaceViewportDensity) => void
        erpWorkspaceFocus?: () => void
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
    url.searchParams.delete("_workspace_frame")
    if (url.origin !== new URL(base).origin) return null
    url.pathname = url.pathname.replace(/\/+$/, "") || "/"
    const root = url.pathname.split("/")[1]
    return [
        "workspace",
        "account",
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
    const businessUrl = new URL(url)
    businessUrl.searchParams.delete("_workspace_frame")
    return `${businessUrl.pathname}${businessUrl.search}${businessUrl.hash}`
}

/** Filters belong to a tab; separately named fulfillment lanes belong to different tabs. */
export function workspaceTabKey(url: URL): string {
    // 归一化遗留双路由与查询挂靠详情，防止同一业务对象重复开出两个工作页签。
    const legacySelection = url.pathname.match(/^\/sales\/selections\/([^/]+)$/)
    if (legacySelection) {
        return `/sales/selection/${legacySelection[1]}`
    }
    if (url.pathname === "/sales/selections") {
        return "/sales/selection"
    }
    const legacyProposal = url.pathname.match(/^\/sales\/proposals\/([^/]+)$/)
    if (legacyProposal) {
        return `/sales/selection/proposals/${legacyProposal[1]}`
    }
    if (url.pathname === "/supplier-api/settlements") {
        const statementId = url.searchParams.get("statementId")?.trim()
        if (statementId) {
            return `/supplier-api/settlements/${encodeURIComponent(statementId)}`
        }
    }
    if (url.pathname === "/governance/imports") {
        const batchId = url.searchParams.get("batchId")?.trim()
        if (batchId) {
            return `/governance/imports/${encodeURIComponent(batchId)}`
        }
    }
    if (url.pathname === "/supplier-api/catalog") {
        const connectionId = url.searchParams.get("connectionId")?.trim()
        if (connectionId) {
            return `${url.pathname}?connectionId=${encodeURIComponent(connectionId)}`
        }
    }
    if (url.pathname === "/finance/customer-accounts/receipts/new")
        return `${url.pathname}?${new URLSearchParams({ sessionId: url.searchParams.get("sessionId") ?? "" })}`
    if (url.pathname === "/finance/supplier-accounts/invoices/new") {
        const identity = new URLSearchParams()
        for (const name of [
            "draftSessionId",
            "supplierId",
            "invoiceId",
            "payableAccountId",
            "purchaseOrderId",
        ]) {
            const value = url.searchParams.get(name)
            if (value) identity.set(name, value)
        }
        return `${url.pathname}?${identity}`
    }
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
    if (url.pathname === "/account/workbench") return "个人首页设置"
    if (url.pathname === "/account/data-transfers") return "我的文件任务"
    if (/^\/master-data\/products\/(?!new$)[^/]+$/.test(url.pathname))
        return "商品详情"
    if (
        /^\/sales\/selection\/proposals\/[^/]+$/.test(url.pathname) ||
        /^\/sales\/proposals\/[^/]+$/.test(url.pathname)
    )
        return "销售方案"
    if (/^\/sales\/selections\/[^/]+$/.test(url.pathname)) return "选品册"
    if (
        /^\/procurement\/supplier-portal\/applications\/[^/]+$/.test(
            url.pathname,
        )
    )
        return "供应商申请审核"
    if (/^\/governance\/imports\/[^/]+$/.test(url.pathname))
        return "导入批次详情"
    if (
        url.pathname === "/governance/imports" &&
        Boolean(url.searchParams.get("batchId")?.trim())
    )
        return "导入批次详情"
    if (/^\/governance\/integration-errors\/errors\/[^/]+$/.test(url.pathname))
        return "外部同步异常"
    if (
        /^\/governance\/integration-errors\/differences\/[^/]+$/.test(
            url.pathname,
        )
    )
        return "对账差异"
    if (/^\/system\/roles\/[^/]+(?:\/edit)?$/.test(url.pathname))
        return "角色权限"
    if (/^\/supplier-api\/catalog\/adoptions\/[^/]+$/.test(url.pathname))
        return "批量选品入库"
    if (url.pathname === "/finance/customer-accounts/receipts/new")
        return "登记回款"
    if (url.pathname === "/finance/customer-accounts/scope")
        return "客户往来汇总"
    if (url.pathname === "/finance/customer-accounts/invoice-requests/scope")
        return "开票申请汇总"
    if (
        url.pathname === "/finance/customer-accounts/invoice-requests" &&
        url.searchParams.get("create") === "1"
    )
        return "新建开票申请"
    if (
        /^\/finance\/customer-accounts\/invoice-requests\/[^/]+$/.test(
            url.pathname,
        )
    )
        return "开票申请详情"
    if (url.pathname === "/finance/supplier-accounts/scope")
        return "供应商往来汇总"
    if (url.pathname === "/finance/supplier-accounts/invoices/new")
        return url.searchParams.has("invoiceId")
            ? "继续核销进项发票"
            : "登记进项发票"
    const normalizedPath = normalizeWorkspaceNavPathname(url.pathname)
    const items = WORKSPACE_NAV_GROUPS.flatMap((group) => group.items)
    const match = items
        .filter((item) => {
            const candidate = new URL(item.href, url.origin)
            return (
                (normalizedPath === candidate.pathname ||
                    normalizedPath.startsWith(`${candidate.pathname}/`)) &&
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
