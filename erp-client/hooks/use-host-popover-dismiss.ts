"use client"

import { useEffect, useRef } from "react"

/** Collects accessible same-origin iframe documents, including nested frames. */
export function collectIframeDocuments(
    root: Document,
    result: Set<Document>,
): void {
    for (const frame of root.querySelectorAll("iframe")) {
        try {
            const doc = frame.contentDocument
            if (doc && !result.has(doc)) {
                result.add(doc)
                collectIframeDocuments(doc, result)
            }
        } catch {
            // Cross-origin frames are handled via window blur + activeElement fallback.
        }
    }
}

/**
 * Dismisses a host-document popover when the user interacts with any child
 * workspace iframe (same-origin pointerdown/click or cross-origin window blur).
 * Skips listener registration inside leaf iframe pages without nested frames.
 */
export function useHostPopoverDismiss(
    open: boolean,
    onDismiss: () => void,
): void {
    const onDismissRef = useRef(onDismiss)
    onDismissRef.current = onDismiss

    useEffect(() => {
        if (!open || typeof document === "undefined") return

        const isHostWindow = window === window.top
        if (!isHostWindow && document.querySelector("iframe") === null) return

        // Base UI dismiss only observes the current document; workspace pages live in child iframes.
        const docs = new Set<Document>()
        const cleanups: Array<() => void> = []
        let blurTimer: number | undefined

        const closeFromOutsideFrame = () => {
            window.clearTimeout(blurTimer)
            onDismissRef.current()
        }

        const bindIframeDocuments = () => {
            const nextDocs = new Set<Document>()
            collectIframeDocuments(document, nextDocs)
            for (const doc of nextDocs) {
                if (docs.has(doc)) continue
                docs.add(doc)
                doc.addEventListener("pointerdown", closeFromOutsideFrame, true)
                doc.addEventListener("click", closeFromOutsideFrame, true)
                cleanups.push(() => {
                    doc.removeEventListener(
                        "pointerdown",
                        closeFromOutsideFrame,
                        true,
                    )
                    doc.removeEventListener(
                        "click",
                        closeFromOutsideFrame,
                        true,
                    )
                })
            }
        }

        const onFrameLoad = (event: Event) => {
            if (event.target instanceof HTMLIFrameElement) {
                bindIframeDocuments()
            }
        }

        const onWindowBlur = () => {
            window.clearTimeout(blurTimer)
            blurTimer = window.setTimeout(() => {
                if (document.activeElement instanceof HTMLIFrameElement) {
                    closeFromOutsideFrame()
                }
            }, 0)
        }

        bindIframeDocuments()
        document.addEventListener("load", onFrameLoad, true)
        window.addEventListener("blur", onWindowBlur)

        return () => {
            window.clearTimeout(blurTimer)
            document.removeEventListener("load", onFrameLoad, true)
            window.removeEventListener("blur", onWindowBlur)
            for (const cleanup of cleanups) {
                cleanup()
            }
        }
    }, [open])
}
