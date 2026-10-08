"use client"

import { useEffect, useRef, useState } from "react"
import type { PDFDocumentProxy, RenderTask } from "pdfjs-dist"
import { ChevronLeftIcon, ChevronRightIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"

/** 只渲染当前页；不执行 PDF 脚本，字体与解码资源从同源加载。 */
export function ContractPdfPages({ blob }: { blob: Blob }) {
    const [document, setDocument] = useState<PDFDocumentProxy>()
    const [page, setPage] = useState(1)
    const [width, setWidth] = useState(0)
    const [error, setError] = useState(false)
    const [rendering, setRendering] = useState(true)
    const canvas = useRef<HTMLCanvasElement>(null)
    const container = useRef<HTMLDivElement>(null)

    useEffect(() => {
        let active = true
        let loading:
            | ReturnType<typeof import("pdfjs-dist").getDocument>
            | undefined
        setDocument(undefined)
        setPage(1)
        setError(false)
        void (async () => {
            const pdf = await import("pdfjs-dist")
            pdf.GlobalWorkerOptions.workerSrc = new URL(
                "pdfjs-dist/build/pdf.worker.min.mjs",
                import.meta.url,
            ).toString()
            const data = await blob.arrayBuffer()
            if (!active) return
            const assets = `/pdfjs/${pdf.version}/`
            loading = pdf.getDocument({
                data,
                cMapUrl: `${assets}cmaps/`,
                standardFontDataUrl: `${assets}standard_fonts/`,
                wasmUrl: `${assets}wasm/`,
                iccUrl: `${assets}iccs/`,
            })
            const document = await loading.promise
            if (active) setDocument(document)
        })().catch(() => {
            if (active) setError(true)
        })
        return () => {
            active = false
            void loading?.destroy()
        }
    }, [blob])

    useEffect(() => {
        const element = container.current
        if (!element) return
        const observer = new ResizeObserver(([entry]) => {
            if (entry) setWidth(Math.floor(entry.contentRect.width))
        })
        observer.observe(element)
        return () => observer.disconnect()
    }, [])

    useEffect(() => {
        if (!document || !width || !canvas.current) return
        const element = canvas.current
        let active = true
        let render: RenderTask | undefined
        setRendering(true)
        setError(false)
        void document
            .getPage(page)
            .then(async (pdfPage) => {
                if (!active) return
                const viewport = pdfPage.getViewport({ scale: 1 })
                const density = Math.min(window.devicePixelRatio || 1, 2)
                const scaled = pdfPage.getViewport({
                    scale: (width / viewport.width) * density,
                })
                element.width = scaled.width
                element.height = scaled.height
                render = pdfPage.render({ canvas: element, viewport: scaled })
                await render.promise
                if (active) setRendering(false)
            })
            .catch(() => {
                if (active) setError(true)
            })
        return () => {
            active = false
            render?.cancel()
        }
    }, [document, page, width])

    return (
        <>
            <div
                ref={container}
                id="contract-import-pdf-pages"
                aria-label="合同原文页，可滚动查看"
                className="relative flex min-h-64 flex-1 items-start justify-center overflow-auto rounded-md border border-border bg-background lg:min-h-0"
            >
                {error ? (
                    <p
                        role="alert"
                        className="p-5 text-sm text-muted-foreground"
                    >
                        此文件暂时无法分页预览，请点击“查看完整原文”。
                    </p>
                ) : null}
                {!error && (!document || rendering) ? (
                    <p
                        role="status"
                        className="absolute inset-0 flex items-center justify-center gap-2 bg-background text-sm text-muted-foreground"
                    >
                        <Spinner />
                        正在渲染原文…
                    </p>
                ) : null}
                <canvas
                    ref={canvas}
                    role="img"
                    aria-label={`合同原文第 ${page} 页，请通过“查看完整原文”阅读可选择的文本`}
                    className="block max-h-full max-w-full object-contain"
                    hidden={error}
                />
            </div>
            <div className="flex shrink-0 items-center justify-center gap-4">
                <Button
                    id="contract-import-pdf-previous"
                    type="button"
                    variant="outline"
                    size="icon-sm"
                    aria-label="原文上一页"
                    disabled={!document || page <= 1}
                    onClick={() => {
                        setPage(page - 1)
                        container.current?.scrollTo({ top: 0 })
                    }}
                >
                    <ChevronLeftIcon />
                </Button>
                <span
                    className="num text-sm text-muted-foreground"
                    aria-live="polite"
                >
                    {document ? `${page} / ${document.numPages}` : "加载中"}
                </span>
                <Button
                    id="contract-import-pdf-next"
                    type="button"
                    variant="outline"
                    size="icon-sm"
                    aria-label="原文下一页"
                    disabled={!document || page >= document.numPages}
                    onClick={() => {
                        setPage(page + 1)
                        container.current?.scrollTo({ top: 0 })
                    }}
                >
                    <ChevronRightIcon />
                </Button>
            </div>
        </>
    )
}
