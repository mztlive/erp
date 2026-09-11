/**
 * 投递动画：从触发按钮沿二次贝塞尔飞到侧栏入口。
 * 减弱动效时只在落点闪一下。粒子是装饰，成功语义由 Toast 承担。
 */

"use client"

import * as React from "react"
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
}

/**
 * 挂在工作区壳上，接收提交成功后的投递。
 */
export function NavDeliveryOverlay() {
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
        const unsubscribe = subscribeNavDelivery(({ kind, origin }) => {
            const targetEl = document.querySelector<HTMLElement>(
                `[data-workspace-nav="${NAV_DELIVERY_TARGETS[kind]}"]`,
            )
            // 入口可能在侧栏的可视区之外：先让它露出，落点才是用户看得到的那个菜单项。
            targetEl?.scrollIntoView({ block: "nearest" })
            const target = targetEl
                ? rectCenter(targetEl.getBoundingClientRect())
                : origin
            const reduced =
                typeof window.matchMedia === "function" &&
                window.matchMedia("(prefers-reduced-motion: reduce)").matches
            if (reduced || !targetEl) {
                pulseAt(target)
                return
            }
            const id = nextId.current
            nextId.current += 1
            setParticles((current) => [
                ...current,
                { id, kind, origin, target },
            ])
        })
        return () => {
            unsubscribe()
            if (pulseTimer.current != null)
                window.clearTimeout(pulseTimer.current)
        }
    }, [pulseAt])

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
