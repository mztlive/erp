/**
 * 投递提示：提交后台任务后发出 Toast，并在对应侧栏入口轻脉冲提示位置。
 * 保持克制商务感，不使用跨屏飞行粒子动画。
 */

"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import { toast } from "@/components/ui/toast"
import { useRouter } from "@/lib/navigation"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { backgroundJobKeys } from "@/features/background-jobs/queries"
import { createPortal } from "react-dom"

import {
    NAV_DELIVERY_TARGETS,
    rectCenter,
    subscribeNavDelivery,
    type NavDeliveryKind,
    type NavDeliveryPoint,
} from "@/lib/nav-delivery"

const PULSE_DURATION_MS = 650

/**
 * 挂在工作区壳上，接收提交成功后的投递。
 */
export function NavDeliveryOverlay() {
    const router = useRouter()
    const queryClient = useQueryClient()
    const notifiedTasks = React.useRef(new Set<string>())
    const [mounted, setMounted] = React.useState(false)
    const [pulse, setPulse] = React.useState<NavDeliveryPoint | null>(null)
    const pulseTimer = React.useRef<number | null>(null)

    React.useEffect(() => {
        setMounted(true)
    }, [])

    const pulseAt = React.useCallback((point: NavDeliveryPoint) => {
        if (pulseTimer.current != null) window.clearTimeout(pulseTimer.current)
        setPulse(point)
        pulseTimer.current = window.setTimeout(() => {
            setPulse(null)
            pulseTimer.current = null
        }, PULSE_DURATION_MS)
    }, [])

    React.useEffect(() => {
        const unsubscribe = subscribeNavDelivery(
            ({ kind, origin, task, animate }) => {
                const destination = deliveryDestination(kind)
                const targetEl = document.querySelector<HTMLElement>(
                    `[data-workspace-nav="${destination.nav}"]`,
                )
                if (task) {
                    if (notifiedTasks.current.has(task.id)) return
                    notifiedTasks.current.add(task.id)
                    const oldest = notifiedTasks.current.values().next().value
                    if (notifiedTasks.current.size > 200 && oldest)
                        notifiedTasks.current.delete(oldest)
                    void queryClient.invalidateQueries({
                        queryKey: backgroundJobKeys.all,
                    })
                    toast.add({
                        title: "已提交后台处理",
                        description: `任务 ${task.jobNo}，可离开当前页面，稍后到“${destination.label}”查看结果。`,
                        type: "info",
                        timeout: 6000,
                        actionProps: {
                            id: `data-transfer-notice-${toAutomationIdSegment(task.id)}-open`,
                            children: "查看任务",
                            onClick: () => router.push(destination.href),
                        },
                    })
                }
                if (animate === false) return
                targetEl?.scrollIntoView({ block: "nearest" })
                const targetRect = targetEl?.getBoundingClientRect()
                const hasTarget = Boolean(
                    targetRect?.width && targetRect.height,
                )
                const target =
                    hasTarget && targetRect ? rectCenter(targetRect) : origin
                pulseAt(target)
            },
        )
        return () => {
            unsubscribe()
            if (pulseTimer.current != null)
                window.clearTimeout(pulseTimer.current)
        }
    }, [pulseAt, queryClient, router])

    if (!mounted) return null

    return createPortal(
        <div
            className="pointer-events-none fixed inset-0 z-[100]"
            aria-hidden="true"
        >
            {pulse ? (
                <span
                    data-slot="nav-delivery-pulse"
                    className="absolute size-8 -translate-x-1/2 -translate-y-1/2 rounded-full bg-primary/20 motion-safe:animate-ping"
                    style={{ left: pulse.x, top: pulse.y }}
                />
            ) : null}
        </div>,
        document.body,
    )
}

/** 文件任务优先投递到后台任务菜单，未授权账号使用常驻的个人任务入口。 */
function deliveryDestination(kind: NavDeliveryKind) {
    if (kind === "portal-task")
        return {
            nav: NAV_DELIVERY_TARGETS[kind],
            label: "我的文件任务",
            href: "/supplier-portal/tasks",
        }
    if (
        kind === "file-task" &&
        !document.querySelector(
            '[data-workspace-nav="governance-background-jobs"]',
        )
    )
        return {
            nav: NAV_DELIVERY_TARGETS[kind],
            label: "我的文件任务",
            href: "/account/data-transfers",
        }
    if (kind === "selection-booklet")
        return {
            nav: NAV_DELIVERY_TARGETS[kind],
            label: "选品册",
            href: "/sales/selection",
        }
    return {
        nav: NAV_DELIVERY_TARGETS["background-task"],
        label: "后台任务",
        href: "/governance/background-jobs",
    }
}
