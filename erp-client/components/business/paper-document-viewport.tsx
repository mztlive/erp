"use client"

import * as React from "react"

import { cn } from "@/lib/utils"

export type PaperDocumentViewportProps = {
    children: React.ReactNode
    fitKey?: React.Key
    className?: string
}

/**
 * 把纸质单据缩放到预览区以内，避免 Dialog 本体出现滚动条。
 */
export function PaperDocumentViewport({
    children,
    fitKey,
    className,
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
            const availableWidth = frame.clientWidth
            const availableHeight = frame.clientHeight
            const comfortWidth = Math.min(availableWidth, 48 * 16)
            const scale = Math.min(
                1,
                comfortWidth / Math.max(width, 1),
                availableHeight / Math.max(height, 1),
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
    }, [fitKey, children])

    return (
        <div
            ref={frameRef}
            className={cn(
                "flex min-h-0 flex-1 items-start justify-center overflow-hidden bg-surface-sunken p-3 sm:p-4",
                className,
            )}
        >
            <div
                className="overflow-hidden"
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
