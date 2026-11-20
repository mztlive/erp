"use client"

import * as React from "react"
import type { Table } from "@tanstack/react-table"

/** 根据浏览器实际列宽同步固定列偏移，负责 ResizeObserver/MutationObserver 生命周期。 */
export function usePinnedColumnOffsets<TData>(
    table: Table<TData>,
    surfaceRef: React.RefObject<HTMLDivElement | null>,
) {
    const visibleColumnSignature = table
        .getVisibleLeafColumns()
        .map((column) => column.id)
        .join("|")
    const pinnedColumnSignature = [
        table
            .getLeftLeafColumns()
            .filter((column) => column.getIsVisible())
            .map((column) => column.id)
            .join("|"),
        table
            .getRightLeafColumns()
            .filter((column) => column.getIsVisible())
            .map((column) => column.id)
            .join("|"),
    ].join("::")

    React.useLayoutEffect(() => {
        const surface = surfaceRef.current
        if (!surface) return

        const observedHeaders = new Set<Element>()
        const resizeObserver = new ResizeObserver(() => {
            syncPinnedColumnOffsets()
            syncScrollEdgeFlags()
        })

        function observeLeafHeaders() {
            surface
                ?.querySelectorAll<HTMLElement>(
                    '[data-column-leaf="true"][data-column-id]',
                )
                .forEach((header) => {
                    if (observedHeaders.has(header)) return
                    observedHeaders.add(header)
                    resizeObserver.observe(header)
                })
        }

        function syncPinnedColumnOffsets() {
            const widths = new Map<string, number>()
            surface
                ?.querySelectorAll<HTMLElement>(
                    '[data-column-leaf="true"][data-column-id]',
                )
                .forEach((header) => {
                    const columnId = header.dataset.columnId
                    if (columnId) {
                        widths.set(
                            columnId,
                            header.getBoundingClientRect().width,
                        )
                    }
                })

            const positions = new Map<
                string,
                { side: "left" | "right"; offset: number }
            >()
            let leftOffset = 0
            table
                .getLeftLeafColumns()
                .filter((column) => column.getIsVisible())
                .forEach((column) => {
                    positions.set(column.id, {
                        side: "left",
                        offset: leftOffset,
                    })
                    leftOffset += widths.get(column.id) ?? 0
                })

            let rightOffset = 0
            table
                .getRightLeafColumns()
                .filter((column) => column.getIsVisible())
                .reverse()
                .forEach((column) => {
                    positions.set(column.id, {
                        side: "right",
                        offset: rightOffset,
                    })
                    rightOffset += widths.get(column.id) ?? 0
                })

            surface
                ?.querySelectorAll<HTMLElement>("[data-column-id]")
                .forEach((element) => {
                    const columnId = element.dataset.columnId
                    const position = columnId
                        ? positions.get(columnId)
                        : undefined
                    element.style.left =
                        position?.side === "left" ? `${position.offset}px` : ""
                    element.style.right =
                        position?.side === "right" ? `${position.offset}px` : ""
                })
        }

        const scrollContainer = surface.querySelector<HTMLElement>(
            '[data-slot="table-container"]',
        )

        function syncScrollEdgeFlags() {
            if (!surface || !scrollContainer) return
            const { scrollTop, scrollLeft, scrollWidth, clientWidth } =
                scrollContainer
            const hasHorizontalOverflow = scrollWidth - clientWidth > 1
            if (scrollTop > 1) surface.dataset.scrolledTop = "true"
            else delete surface.dataset.scrolledTop

            if (hasHorizontalOverflow && scrollLeft > 1) {
                surface.dataset.scrolledLeft = "true"
            } else {
                delete surface.dataset.scrolledLeft
            }

            if (
                hasHorizontalOverflow &&
                scrollLeft + clientWidth < scrollWidth - 1
            ) {
                surface.dataset.scrolledRight = "true"
            } else {
                delete surface.dataset.scrolledRight
            }
        }

        const mutationObserver = new MutationObserver(() => {
            observeLeafHeaders()
            syncPinnedColumnOffsets()
            syncScrollEdgeFlags()
        })

        observeLeafHeaders()
        resizeObserver.observe(surface)
        if (scrollContainer) {
            resizeObserver.observe(scrollContainer)
            scrollContainer.addEventListener("scroll", syncScrollEdgeFlags, {
                passive: true,
            })
        }
        mutationObserver.observe(surface, { childList: true, subtree: true })
        syncPinnedColumnOffsets()
        syncScrollEdgeFlags()

        return () => {
            resizeObserver.disconnect()
            mutationObserver.disconnect()
            scrollContainer?.removeEventListener("scroll", syncScrollEdgeFlags)
        }
    }, [pinnedColumnSignature, surfaceRef, table, visibleColumnSignature])
}
