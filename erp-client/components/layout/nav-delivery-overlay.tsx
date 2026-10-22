/**
 * 投递动画：从触发按钮沿二次贝塞尔飞到侧栏入口。
 * 减弱动效时只在落点闪一下。粒子是装饰，成功语义由 Toast 承担。
 */

"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import { toast } from "@/components/ui/toast"
import { useRouter } from "@/lib/navigation"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { backgroundJobKeys } from "@/features/background-jobs/queries"
import { createPortal } from "react-dom"
import { ListChecksIcon, PackageSearchIcon } from "lucide-react"

import {
    launchControlPoint,
    NAV_DELIVERY_TARGETS,
    quadraticBezier,
    rectCenter,
    subscribeNavDelivery,
    type NavDeliveryKind,
    type NavDeliveryPoint,
} from "@/lib/nav-delivery"

const LAUNCH_DURATION_MS = 720
const PULSE_DURATION_MS = 700

type LaunchParticle = {
    id: number
    kind: NavDeliveryKind
    origin: NavDeliveryPoint
    target: NavDeliveryPoint
}

const PARTICLE_ICON: Record<
    NavDeliveryKind,
    React.ComponentType<{ className?: string }>
> = {
    "selection-booklet": PackageSearchIcon,
    "background-task": ListChecksIcon,
    "file-task": ListChecksIcon,
    "portal-task": ListChecksIcon,
}

/**
 * 挂在工作区壳上，接收提交成功后的投递。
 */
export function NavDeliveryOverlay() {
    const router = useRouter()
    const queryClient = useQueryClient()
    const notifiedTasks = React.useRef(new Set<string>())
    const [mounted, setMounted] = React.useState(false)
    const [particles, setParticles] = React.useState<LaunchParticle[]>([])
    const [pulse, setPulse] = React.useState<NavDeliveryPoint | null>(null)
    const nextId = React.useRef(0)
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
                // 入口可能在侧栏的可视区之外：先让它露出，落点才是用户看得到的那个菜单项。
                targetEl?.scrollIntoView({ block: "nearest" })
                const targetRect = targetEl?.getBoundingClientRect()
                const hasTarget = Boolean(
                    targetRect?.width && targetRect.height,
                )
                const target =
                    hasTarget && targetRect ? rectCenter(targetRect) : origin
                const reduced =
                    typeof window.matchMedia === "function" &&
                    window.matchMedia("(prefers-reduced-motion: reduce)")
                        .matches
                if (reduced || !hasTarget) {
                    pulseAt(target)
                    return
                }
                const id = nextId.current
                nextId.current += 1
                setParticles((current) => [
                    ...current,
                    { id, kind, origin, target },
                ])
            },
        )
        return () => {
            unsubscribe()
            if (pulseTimer.current != null)
                window.clearTimeout(pulseTimer.current)
        }
    }, [pulseAt, queryClient, router])

    const removeParticle = React.useCallback(
        (id: number, target: NavDeliveryPoint) => {
            setParticles((current) =>
                current.filter((particle) => particle.id !== id),
            )
            pulseAt(target)
        },
        [pulseAt],
    )

    if (!mounted) return null

    return createPortal(
        <div
            className="pointer-events-none fixed inset-0 z-[100]"
            aria-hidden="true"
        >
            {particles.map((particle) => (
                <FlyingTask
                    key={particle.id}
                    kind={particle.kind}
                    origin={particle.origin}
                    target={particle.target}
                    onDone={() => removeParticle(particle.id, particle.target)}
                />
            ))}
            {pulse ? (
                <span
                    data-slot="nav-delivery-pulse"
                    className="absolute size-10 -translate-x-1/2 -translate-y-1/2 rounded-full bg-primary/25 motion-safe:animate-ping"
                    style={{ left: pulse.x, top: pulse.y }}
                />
            ) : null}
        </div>,
        document.body,
    )
}

function FlyingTask({
    kind,
    origin,
    target,
    onDone,
}: {
    kind: NavDeliveryKind
    origin: NavDeliveryPoint
    target: NavDeliveryPoint
    onDone: () => void
}) {
    const nodeRef = React.useRef<HTMLDivElement>(null)
    const onDoneRef = React.useRef(onDone)
    onDoneRef.current = onDone
    const Icon = PARTICLE_ICON[kind]

    React.useEffect(() => {
        const node = nodeRef.current
        if (!node) return undefined
        const control = launchControlPoint(origin, target)
        const started = performance.now()
        let frame = 0
        const tick = (now: number) => {
            const t = Math.min(1, (now - started) / LAUNCH_DURATION_MS)
            const eased = 1 - (1 - t) ** 3
            const x = quadraticBezier(origin.x, control.x, target.x, eased)
            const y = quadraticBezier(origin.y, control.y, target.y, eased)
            const scale = 1 - eased * 0.55
            const opacity = t > 0.82 ? Math.max(0, 1 - (t - 0.82) / 0.18) : 1
            node.style.transform = `translate(${x}px, ${y}px) translate(-50%, -50%) scale(${scale})`
            node.style.opacity = String(opacity)
            if (t < 1) {
                frame = window.requestAnimationFrame(tick)
                return
            }
            onDoneRef.current()
        }
        frame = window.requestAnimationFrame(tick)
        return () => window.cancelAnimationFrame(frame)
    }, [origin, target])

    return (
        <div
            ref={nodeRef}
            data-slot="nav-delivery-particle"
            className="absolute top-0 left-0 flex size-8 items-center justify-center rounded-full bg-primary text-primary-foreground shadow-lg will-change-transform"
            style={{
                transform: `translate(${origin.x}px, ${origin.y}px) translate(-50%, -50%) scale(1)`,
            }}
        >
            <Icon className="size-4" />
        </div>
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
