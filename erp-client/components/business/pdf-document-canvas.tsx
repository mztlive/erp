"use client"

import { useEffect, useRef, useState } from "react"
import type { PDFDocumentProxy, RenderTask } from "pdfjs-dist"
import { ChevronLeftIcon, ChevronRightIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { cn } from "@/lib/utils"

export type PdfRotation = 0 | 90 | 180 | 270

export type PdfDocumentCanvasProps = Readonly<{
    blob: Blob | null
    /** 缩放倍率（默认 1）。fitWidth=true 时在适应容器宽度的基础上叠加该倍率。 */
    scale?: number
    /** 是否按容器宽度自适应（默认 true）。 */
    fitWidth?: boolean
    /** 顺时针旋转角度（默认 0）。 */
    rotation?: PdfRotation
    /** 受控页码（1-based）；不传时组件内部维护页码。 */
    page?: number
    /** 页码变化回调。 */
    onPageChange?: (page: number) => void
    /** 页数加载完成后通知调用方。 */
    onPageCountChange?: (pageCount: number) => void
    /** 是否显示内置翻页栏（默认 true）。 */
    showPagination?: boolean
    /** 适应视口内完整单页显示（兼容合同预览弹窗默认行为）。 */
    containPage?: boolean
    /** 自动化 ID 前缀。 */
    idPrefix?: string
    containerAriaLabel?: string
    canvasAriaLabel?: (page: number) => string
    loadingLabel?: string
    errorMessage?: string
    emptyMessage?: string
    className?: string
    canvasContainerClassName?: string
}>

/**
 * 通用 PDF 分页画布：消费认证拉取后的 Blob，不执行 PDF 脚本，
 * CMap、标准字体与 WASM 资源均从同源 `/pdfjs/${version}/` 加载，
 * 在参数变化或卸载时主动取消在途渲染任务。
 */
export function PdfDocumentCanvas({
    blob,
    scale = 1,
    fitWidth = true,
    rotation = 0,
    page: controlledPage,
    onPageChange,
    onPageCountChange,
    showPagination = true,
    containPage = false,
    idPrefix = "pdf-document-canvas",
    containerAriaLabel = "PDF 文档预览，可滚动查看",
    canvasAriaLabel = (pageNumber) => `PDF 第 ${pageNumber} 页`,
    loadingLabel = "正在渲染页面…",
    errorMessage = "此 PDF 文件暂时无法分页预览，请下载原文件查看。",
    emptyMessage = "暂无可预览的 PDF 文件。",
    className,
    canvasContainerClassName,
}: PdfDocumentCanvasProps) {
    const [pdfDocument, setPdfDocument] = useState<PDFDocumentProxy | null>(
        null,
    )
    const [internalPage, setInternalPage] = useState(1)
    const [containerWidth, setContainerWidth] = useState(0)
    const [error, setError] = useState(false)
    const [rendering, setRendering] = useState(false)

    const canvasRef = useRef<HTMLCanvasElement>(null)
    const containerRef = useRef<HTMLDivElement>(null)
    const onPageCountChangeRef = useRef(onPageCountChange)
    onPageCountChangeRef.current = onPageCountChange

    const pageCount = pdfDocument?.numPages ?? 0
    const rawPage = controlledPage ?? internalPage
    const activePage =
        pageCount > 0 ? Math.min(Math.max(1, rawPage), pageCount) : 1

    const updatePage = (nextPage: number) => {
        const clamped =
            pageCount > 0 ? Math.min(Math.max(1, nextPage), pageCount) : 1
        if (controlledPage === undefined) {
            setInternalPage(clamped)
        }
        onPageChange?.(clamped)
        containerRef.current?.scrollTo({ top: 0 })
    }

    useEffect(() => {
        if (!blob) {
            setPdfDocument(null)
            setInternalPage(1)
            setError(false)
            setRendering(false)
            onPageCountChangeRef.current?.(0)
            return
        }

        let active = true
        let loadingTask:
            | ReturnType<typeof import("pdfjs-dist").getDocument>
            | undefined

        setPdfDocument(null)
        setInternalPage(1)
        setError(false)
        setRendering(true)

        void (async () => {
            const pdf = await import("pdfjs-dist")
            pdf.GlobalWorkerOptions.workerSrc = new URL(
                "pdfjs-dist/build/pdf.worker.min.mjs",
                import.meta.url,
            ).toString()
            const data = await blob.arrayBuffer()
            if (!active) return
            const assets = `/pdfjs/${pdf.version}/`
            loadingTask = pdf.getDocument({
                data,
                cMapUrl: `${assets}cmaps/`,
                standardFontDataUrl: `${assets}standard_fonts/`,
                wasmUrl: `${assets}wasm/`,
                iccUrl: `${assets}iccs/`,
            })
            const loaded = await loadingTask.promise
            if (!active) {
                void loadingTask.destroy()
                return
            }
            setPdfDocument(loaded)
            onPageCountChangeRef.current?.(loaded.numPages)
        })().catch(() => {
            if (!active) return
            setError(true)
            setRendering(false)
            onPageCountChangeRef.current?.(0)
        })

        return () => {
            active = false
            void loadingTask?.destroy()
        }
    }, [blob])

    const hasBlob = Boolean(blob)

    useEffect(() => {
        if (!hasBlob) {
            setContainerWidth(0)
            return
        }
        const element = containerRef.current
        if (!element) return
        const updateWidth = () => {
            const nextWidth = Math.floor(element.clientWidth)
            setContainerWidth((current) =>
                current === nextWidth ? current : nextWidth,
            )
        }
        updateWidth()
        if (typeof ResizeObserver === "undefined") return
        const observer = new ResizeObserver(updateWidth)
        observer.observe(element)
        return () => observer.disconnect()
    }, [hasBlob])

    useEffect(() => {
        if (!pdfDocument || !canvasRef.current) return
        if (fitWidth && containerWidth <= 0) return

        const element = canvasRef.current
        let active = true
        let renderTask: RenderTask | undefined
        setRendering(true)
        setError(false)

        void pdfDocument
            .getPage(activePage)
            .then(async (pdfPage) => {
                if (!active) return
                const effectiveRotation = (pdfPage.rotate + rotation) % 360
                const baseViewport = pdfPage.getViewport({
                    scale: 1,
                    rotation: effectiveRotation,
                })
                const safeScale = Math.max(0.25, Math.min(scale, 4))
                const density = Math.min(window.devicePixelRatio || 1, 2)

                if (containPage) {
                    const cssScale =
                        (Math.max(containerWidth, 160) / baseViewport.width) *
                        safeScale
                    const renderViewport = pdfPage.getViewport({
                        scale: cssScale * density,
                        rotation: effectiveRotation,
                    })
                    element.width = Math.ceil(renderViewport.width)
                    element.height = Math.ceil(renderViewport.height)
                    element.style.width = ""
                    element.style.height = ""
                    renderTask = pdfPage.render({
                        canvas: element,
                        viewport: renderViewport,
                    })
                } else {
                    const availableWidth = Math.max(containerWidth - 24, 160)
                    const cssScale = fitWidth
                        ? (availableWidth / baseViewport.width) * safeScale
                        : safeScale
                    const renderViewport = pdfPage.getViewport({
                        scale: cssScale * density,
                        rotation: effectiveRotation,
                    })
                    element.width = Math.ceil(renderViewport.width)
                    element.height = Math.ceil(renderViewport.height)
                    element.style.width = `${Math.round(baseViewport.width * cssScale)}px`
                    element.style.height = `${Math.round(baseViewport.height * cssScale)}px`
                    renderTask = pdfPage.render({
                        canvas: element,
                        viewport: renderViewport,
                    })
                }

                await renderTask.promise
                if (active) setRendering(false)
            })
            .catch((cause: unknown) => {
                if (!active) return
                if (
                    cause instanceof Error &&
                    cause.name === "RenderingCancelledException"
                ) {
                    return
                }
                setError(true)
                setRendering(false)
            })

        return () => {
            active = false
            renderTask?.cancel()
        }
    }, [
        pdfDocument,
        activePage,
        containerWidth,
        scale,
        fitWidth,
        rotation,
        containPage,
    ])

    if (!blob) {
        return (
            <div
                className={cn(
                    "flex min-h-48 flex-1 items-center justify-center rounded-md border border-dashed border-border bg-muted/20 p-6 text-xs text-muted-foreground",
                    className,
                )}
            >
                {emptyMessage}
            </div>
        )
    }

    return (
        <div
            data-slot="pdf-document-canvas"
            className={cn("flex min-h-0 flex-1 flex-col gap-3", className)}
        >
            <div
                ref={containerRef}
                id={`${idPrefix}-pages`}
                aria-label={containerAriaLabel}
                className={cn(
                    "relative flex min-h-64 flex-1 items-start justify-center overflow-auto rounded-md border border-border bg-background lg:min-h-0",
                    !containPage && "bg-muted/25 p-3",
                    canvasContainerClassName,
                )}
            >
                {error ? (
                    <p
                        role="alert"
                        className="p-5 text-xs text-muted-foreground"
                    >
                        {errorMessage}
                    </p>
                ) : null}
                {!error && (!pdfDocument || rendering) ? (
                    <p
                        role="status"
                        className="absolute inset-0 flex items-center justify-center gap-2 bg-background/85 text-xs text-muted-foreground"
                    >
                        <Spinner />
                        {loadingLabel}
                    </p>
                ) : null}
                <canvas
                    ref={canvasRef}
                    role="img"
                    aria-label={canvasAriaLabel(activePage)}
                    className={cn(
                        "block",
                        containPage
                            ? "max-h-full max-w-full object-contain"
                            : "shrink-0 bg-card shadow-xs",
                    )}
                    hidden={error}
                />
            </div>
            {showPagination ? (
                <div className="flex shrink-0 items-center justify-center gap-4">
                    <Button
                        id={`${idPrefix}-previous`}
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        aria-label="上一页"
                        disabled={!pdfDocument || activePage <= 1}
                        onClick={() => updatePage(activePage - 1)}
                    >
                        <ChevronLeftIcon aria-hidden="true" />
                    </Button>
                    <span
                        className="num text-body-compact text-muted-foreground"
                        aria-live="polite"
                    >
                        {pdfDocument
                            ? `${activePage} / ${pageCount}`
                            : "加载中"}
                    </span>
                    <Button
                        id={`${idPrefix}-next`}
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        aria-label="下一页"
                        disabled={!pdfDocument || activePage >= pageCount}
                        onClick={() => updatePage(activePage + 1)}
                    >
                        <ChevronRightIcon aria-hidden="true" />
                    </Button>
                </div>
            ) : null}
        </div>
    )
}
