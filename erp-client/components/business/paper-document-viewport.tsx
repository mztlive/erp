"use client"

import * as React from "react"

import { cn } from "@/lib/utils"

export type PaperDocumentViewportProps = React.ComponentProps<"div"> & {
    fitKey?: React.Key
    fitMode?: "page" | "width"
}

/**
 * page 模式展示整张单据；width 模式只按宽度缩放，长单据在正文区滚动。
 */
export function PaperDocumentViewport({
    children,
    fitKey,
    fitMode = "page",
    className,
    ...props
}: PaperDocumentViewportProps) {
    const frameRef = React.useRef<HTMLDivElement>(null)
    const sheetRef = React.useRef<HTMLDivElement>(null)
    const [fit, setFit] = React.useState({ scale: 1, width: 0, height: 0 })

    React.useLayoutEffect(() => {
        const frame = frameRef.current
        const sheet = sheetRef.current
        if (!frame || !sheet) return

        const update = () => {
            const paper =
                sheet.querySelector<HTMLElement>(
                    "[data-slot='paper-document']",
                ) ?? sheet
            const width = Math.max(paper.scrollWidth, paper.offsetWidth)
            const height = Math.max(paper.scrollHeight, paper.offsetHeight)
            const style = getComputedStyle(frame)
            const availableWidth =
                frame.clientWidth -
                parseFloat(style.paddingLeft) -
                parseFloat(style.paddingRight)
            const availableHeight =
                frame.clientHeight -
                parseFloat(style.paddingTop) -
                parseFloat(style.paddingBottom)
            const comfortWidth = Math.min(availableWidth, 48 * 16)
            const scale = Math.min(
                1,
                comfortWidth / Math.max(width, 1),
                fitMode === "page" ? availableHeight / Math.max(height, 1) : 1,
            )
            setFit({
                scale: Number.isFinite(scale) && scale > 0 ? scale : 1,
                width,
                height,
            })
        }

        update()
        const observer = new ResizeObserver(update)
        observer.observe(frame)
        observer.observe(sheet)
        return () => observer.disconnect()
    }, [fitKey, fitMode, children])

    return (
        <div
            ref={frameRef}
            className={cn(
                "flex min-h-0 flex-1 items-start justify-center bg-surface-sunken p-3 sm:p-4",
                fitMode === "width" ? "overflow-auto" : "overflow-hidden",
                className,
            )}
            {...props}
        >
            <div
                className="shrink-0 overflow-hidden"
                style={{
                    width: fit.width * fit.scale,
                    height: fit.height * fit.scale,
                }}
            >
                <div
                    ref={sheetRef}
                    className="inline-block"
                    style={{
                        transform: `scale(${fit.scale})`,
                        transformOrigin: "top left",
                    }}
                >
                    {children}
                </div>
            </div>
        </div>
    )
}
