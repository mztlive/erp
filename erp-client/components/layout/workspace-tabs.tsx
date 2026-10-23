"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import {
    AlertDialog,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { dispatchNavDelivery } from "@/lib/nav-delivery"
import {
    handleWorkspaceCloseShortcut,
    interceptWorkspaceLink,
    tabHref,
    workspaceTabKey,
    workspaceTabTitle,
    workspaceUrl,
    type WorkspaceFrameState,
} from "@/lib/workspace-tabs"
import { WorkspaceTabBar } from "./workspace-tab-bar"

export type WorkspaceTab = WorkspaceFrameState & {
    id: string
    key: string
    src: string
    ready: boolean
    instance: string
    openerId?: string
}

function newTab(url: URL): WorkspaceTab {
    const key = workspaceTabKey(url)
    return {
        id: toAutomationIdSegment(
            Array.from(key, (character) =>
                character.charCodeAt(0).toString(16),
            ).join("-"),
        ),
        key,
        src: tabHref(url),
        href: tabHref(url),
        title: workspaceTabTitle(url),
        dirty: false,
        busy: false,
        ready: false,
        // Fast Refresh retains tabs but resets module-level counters.
        instance: crypto.randomUUID(),
    }
}

export function WorkspaceTabs({
    children,
}: {
    children: (bar: React.ReactNode, panels: React.ReactNode) => React.ReactNode
}) {
    const [tabs, setTabs] = React.useState(() => [
        newTab(new URL(window.location.href)),
    ])
    const [active, setActive] = React.useState(tabs[0].id)
    const [pendingClose, setPendingClose] = React.useState<string[]>([])
    const [closeBusy, setCloseBusy] = React.useState(false)
    const frames = React.useRef(new Map<string, HTMLIFrameElement>())
    const changeRevision = React.useRef(0)
    const pendingRefresh = React.useRef(new Map<string, number>())
    const snapshot = React.useRef({ tabs, active, pendingClose })
    snapshot.current = { tabs, active, pendingClose }
    const closeRequest = React.useRef<(ids: string[]) => void>(() => {})
    const queryClient = useQueryClient()

    // 保留到目标页签确认刷新成功；未加载或暂不能刷新的页签可重复接收同一版本。
    const deliverRefresh = React.useCallback((id: string) => {
        const revision = pendingRefresh.current.get(id)
        if (revision !== undefined)
            frames.current
                .get(id)
                ?.contentWindow?.erpWorkspaceInvalidate?.(revision)
    }, [])

    const select = React.useCallback((tab: WorkspaceTab, replace = false) => {
        setActive(tab.id)
        snapshot.current.active = tab.id
        if (
            tab.href !==
            `${window.location.pathname}${window.location.search}${window.location.hash}`
        ) {
            window.history[replace ? "replaceState" : "pushState"](
                null,
                "",
                tab.href,
            )
        }
    }, [])

    React.useEffect(() => {
        const sourceTab = (source: Window) =>
            snapshot.current.tabs.find(
                (tab) => frames.current.get(tab.id)?.contentWindow === source,
            )
        const navigate = (
            source: Window | null,
            href: string,
            replace = false,
            fromHistory = false,
        ) => {
            const url = workspaceUrl(href, window.location.href)
            if (!url || (source && !sourceTab(source))) return
            const current = snapshot.current
            const origin = source ? sourceTab(source) : undefined
            const existing = current.tabs.find(
                (tab) => tab.key === workspaceTabKey(url),
            )
            const target = existing ?? {
                ...newTab(url),
                openerId: origin?.id ?? current.active,
            }
            let next = existing ? current.tabs : [...current.tabs, target]
            if (replace && source) {
                const previous = sourceTab(source)
                if (previous && previous.id !== target.id) {
                    next = next.filter((tab) => tab.id !== previous?.id)
                    pendingRefresh.current.delete(previous.id)
                }
            }
            snapshot.current.tabs = next
            setTabs(next)
            if (origin && origin.id !== current.active) return
            if (fromHistory && existing && existing.href !== tabHref(url)) {
                frames.current
                    .get(existing.id)
                    ?.contentWindow?.erpWorkspaceGo?.(tabHref(url))
                select({ ...existing, href: tabHref(url) }, true)
            } else if (
                source &&
                existing &&
                url.search &&
                existing.href !== tabHref(url)
            ) {
                frames.current
                    .get(existing.id)
                    ?.contentWindow?.erpWorkspaceGo?.(tabHref(url))
                select({ ...existing, href: tabHref(url) }, replace)
            } else select(target, replace || fromHistory)
        }
        window.erpWorkspaceHost = {
            closeCurrent: (source, fallbackHref) => {
                const current = snapshot.current
                const target = source ? sourceTab(source)?.id : current.active
                if (!target || current.pendingClose.length > 0) return
                const tab = current.tabs.find((item) => item.id === target)
                if (
                    source &&
                    fallbackHref &&
                    !current.tabs.some((item) => item.id === tab?.openerId)
                ) {
                    navigate(source, fallbackHref, true)
                    return
                }
                closeRequest.current([target])
            },
            navigate,
            deliver: (source, request) => {
                const tab = sourceTab(source)
                if (!tab) return
                const active = tab.id === snapshot.current.active
                if (!active && !request.task) return
                const rect = frames.current.get(tab.id)?.getBoundingClientRect()
                if (rect)
                    dispatchNavDelivery({
                        ...request,
                        animate: active && request.animate !== false,
                        origin: {
                            x: rect.left + request.origin.x,
                            y: rect.top + request.origin.y,
                        },
                    })
            },
            report: (source, state) => {
                const previous = sourceTab(source)
                if (!previous) return
                deliverRefresh(previous.id)
                const url = workspaceUrl(state.href, window.location.href)
                if (!url) return
                const redirectedKey = workspaceTabKey(url)
                if (
                    redirectedKey !== previous.key &&
                    snapshot.current.tabs.some(
                        (tab) => tab.key === redirectedKey,
                    )
                ) {
                    navigate(source, state.href, true)
                    return
                }
                const nextState = {
                    ...state,
                    title: state.title || previous.title,
                }
                if (
                    previous.ready &&
                    previous.href === nextState.href &&
                    previous.title === nextState.title &&
                    previous.dirty === state.dirty &&
                    previous.busy === state.busy
                )
                    return
                const next = snapshot.current.tabs.map((tab) =>
                    tab.id === previous.id
                        ? {
                              ...tab,
                              ...nextState,
                              key: redirectedKey,
                              ready: true,
                          }
                        : tab,
                )
                snapshot.current.tabs = next
                setTabs(next)
                if (
                    snapshot.current.active === previous.id &&
                    previous.href !== state.href
                ) {
                    window.history.replaceState(null, "", state.href)
                }
            },
            changed: (source) => {
                const origin = sourceTab(source)
                if (!origin) return
                const revision = ++changeRevision.current
                // 先持久保留在宿主内，再尝试通知；不依赖来源页签后续仍存活。
                const targets = snapshot.current.tabs.filter(
                    (tab) => tab.id !== origin.id,
                )
                for (const tab of targets)
                    pendingRefresh.current.set(tab.id, revision)
                void queryClient.invalidateQueries()
                for (const tab of targets) deliverRefresh(tab.id)
            },
            refreshed: (source, revision) => {
                const tab = sourceTab(source)
                if (tab && pendingRefresh.current.get(tab.id) === revision)
                    pendingRefresh.current.delete(tab.id)
            },
        }
        const pop = () => navigate(null, window.location.href, false, true)
        const sessionChanged = (event: StorageEvent) => {
            if (event.key === "erp.token" || event.key === null)
                window.location.replace("/login")
        }
        const unload = (event: BeforeUnloadEvent) => {
            if (snapshot.current.tabs.some((tab) => tab.dirty || tab.busy)) {
                event.preventDefault()
                event.returnValue = ""
            }
        }
        document.addEventListener("click", interceptWorkspaceLink, true)
        window.addEventListener("keydown", handleWorkspaceCloseShortcut, true)
        window.addEventListener("popstate", pop)
        window.addEventListener("storage", sessionChanged)
        window.addEventListener("beforeunload", unload)
        return () => {
            delete window.erpWorkspaceHost
            document.removeEventListener("click", interceptWorkspaceLink, true)
            window.removeEventListener(
                "keydown",
                handleWorkspaceCloseShortcut,
                true,
            )
            window.removeEventListener("popstate", pop)
            window.removeEventListener("storage", sessionChanged)
            window.removeEventListener("beforeunload", unload)
        }
    }, [queryClient, select, deliverRefresh])

    React.useEffect(() => {
        for (const [id, iframe] of frames.current) {
            deliverRefresh(id)
            iframe.contentWindow?.erpWorkspaceActivate?.(id === active)
        }
    }, [active, deliverRefresh])

    const activeTitle = tabs.find((tab) => tab.id === active)?.title
    React.useEffect(() => {
        if (activeTitle) document.title = `${activeTitle} · 福尚云 ERP`
    }, [activeTitle])

    const close = (ids: string[]) => {
        const current = { ...snapshot.current }
        let next = current.tabs.filter((tab) => !ids.includes(tab.id))
        if (next.length === 0)
            next = [newTab(new URL("/workspace", window.location.origin))]
        // A closed last tab must not reuse its iframe and retain the closed document.
        for (const id of ids) {
            frames.current.delete(id)
            pendingRefresh.current.delete(id)
        }
        snapshot.current.tabs = next
        setTabs(next)
        if (ids.includes(current.active)) {
            const openerId = current.tabs.find(
                (tab) => tab.id === current.active,
            )?.openerId
            select(
                next.find((tab) => tab.id === openerId) ??
                    next[
                        Math.min(
                            current.tabs.findIndex(
                                (tab) => tab.id === current.active,
                            ),
                            next.length - 1,
                        )
                    ],
                true,
            )
            requestAnimationFrame(() => {
                document
                    .getElementById(`workspace-tab-${snapshot.current.active}`)
                    ?.focus({ preventScroll: true })
            })
        }
        setPendingClose([])
    }

    const requestClose = (ids: string[]) => {
        if (!ids.length) return
        const selected = snapshot.current.tabs.filter((tab) =>
            ids.includes(tab.id),
        )
        const busy = selected.some((tab) => tab.busy)
        const changed = selected.some((tab) => {
            const child = frames.current.get(tab.id)?.contentWindow
            // Consult existing page-specific dirty guards as well as shared form state.
            return (
                tab.dirty ||
                (child &&
                    !child.dispatchEvent(
                        new Event("beforeunload", { cancelable: true }),
                    ))
            )
        })
        if (busy || changed) {
            setCloseBusy(busy)
            setPendingClose(ids)
        } else close(ids)
    }
    closeRequest.current = requestClose

    return (
        <>
            {children(
                <WorkspaceTabBar
                    tabs={tabs}
                    active={active}
                    onSelect={select}
                    onClose={requestClose}
                />,
                <div className="relative flex min-h-0 flex-1 flex-col">
                    {tabs.map((tab) => (
                        <section
                            key={tab.instance}
                            id={`workspace-panel-${tab.id}`}
                            role="tabpanel"
                            aria-labelledby={`workspace-tab-${tab.id}`}
                            hidden={tab.id !== active}
                            className="relative min-h-0 flex-1"
                            style={{
                                display: tab.id === active ? "flex" : "none",
                            }}
                        >
                            {!tab.ready ? (
                                <div
                                    role="status"
                                    className="absolute inset-0 flex items-center justify-center bg-background text-sm text-muted-foreground"
                                >
                                    正在打开工作页…
                                </div>
                            ) : null}
                            <iframe
                                ref={(node) => {
                                    if (node) frames.current.set(tab.id, node)
                                    else frames.current.delete(tab.id)
                                }}
                                id={`workspace-frame-${tab.id}`}
                                data-workspace-frame=""
                                data-workspace-active={tab.id === active}
                                title={tab.title}
                                src={tab.src}
                                onLoad={() => {
                                    setTabs((current) =>
                                        current.map((item) =>
                                            item.instance === tab.instance
                                                ? { ...item, ready: true }
                                                : item,
                                        ),
                                    )
                                    deliverRefresh(tab.id)
                                    frames.current
                                        .get(tab.id)
                                        ?.contentWindow?.erpWorkspaceActivate?.(
                                            tab.id === snapshot.current.active,
                                        )
                                }}
                                className="h-full min-h-0 w-full flex-1 border-0"
                            />
                        </section>
                    ))}
                </div>,
            )}
            <AlertDialog
                open={pendingClose.length > 0}
                onOpenChange={(open) => {
                    if (!open) setPendingClose([])
                }}
            >
                <AlertDialogContent>
                    <AlertDialogHeader>
                        <AlertDialogTitle>
                            {closeBusy ? "页面正在提交" : "关闭前请确认已保存"}
                        </AlertDialogTitle>
                        <AlertDialogDescription>
                            {closeBusy
                                ? "请等待提交完成后再关闭，避免遗漏处理结果。"
                                : "所选页签中有修改过的表单。关闭会释放页面，尚未保存的内容将丢失。"}
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    <AlertDialogFooter>
                        <Button
                            id="workspace-tabs-close-cancel"
                            variant="outline"
                            onClick={() => setPendingClose([])}
                        >
                            {closeBusy ? "知道了" : "继续工作"}
                        </Button>
                        {!closeBusy ? (
                            <Button
                                id="workspace-tabs-close-confirm"
                                variant="destructive"
                                onClick={() => {
                                    if (
                                        snapshot.current.tabs.some(
                                            (tab) =>
                                                pendingClose.includes(tab.id) &&
                                                tab.busy,
                                        )
                                    )
                                        setCloseBusy(true)
                                    else close(pendingClose)
                                }}
                            >
                                确认关闭
                            </Button>
                        ) : null}
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>
        </>
    )
}
