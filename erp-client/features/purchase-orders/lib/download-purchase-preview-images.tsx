import { flushSync } from "react-dom"
import { createRoot } from "react-dom/client"
import { getFontEmbedCSS, toBlob } from "html-to-image"
import JSZip from "jszip"

import { PurchaseOrderPreviewPaper } from "../components/purchase-order-create-preview-paper"
import type {
    PurchaseOrderPreview,
    SourcingSalesOrder,
} from "./purchase-order-create-model"

function fileNamePart(value: string): string {
    return (
        Array.from(value, (character) =>
            character.charCodeAt(0) < 32 ? "_" : character,
        )
            .join("")
            .replace(/[\\/:*?"<>|]/g, "_")
            .trim()
            .slice(0, 80) || "采购单"
    )
}

/** 按预览顺序逐张渲染完整纸张，全部成功后一次下载，避免产生不完整文件包。 */
export async function downloadPurchasePreviewImages(
    previews: readonly PurchaseOrderPreview[],
    sourceOrder: SourcingSalesOrder | undefined,
    onProgress: (completed: number) => void,
): Promise<void> {
    if (previews.length === 0) return

    const host = document.createElement("div")
    host.setAttribute("aria-hidden", "true")
    host.inert = true
    // 脱离缩放视口，使用纸张原始宽度；不使用 display:none，以保留真实排版。
    Object.assign(host.style, {
        position: "fixed",
        left: "-10000px",
        top: "0",
        width: "768px",
        pointerEvents: "none",
    })
    document.body.appendChild(host)
    const root = createRoot(host)
    const zip = new JSZip()

    try {
        let fontEmbedCSS: string | undefined
        for (const [index, preview] of previews.entries()) {
            flushSync(() => {
                root.render(
                    <PurchaseOrderPreviewPaper
                        preview={preview}
                        index={index}
                        sourceOrder={sourceOrder}
                    />,
                )
            })
            await document.fonts.ready
            const paper = host.querySelector<HTMLElement>(
                "[data-slot='paper-document']",
            )
            if (!paper) throw new Error("采购单图片未生成")
            fontEmbedCSS ??= await getFontEmbedCSS(paper)
            const blob = await toBlob(paper, {
                width: Math.max(paper.offsetWidth, paper.scrollWidth),
                height: Math.max(paper.offsetHeight, paper.scrollHeight),
                pixelRatio: 2,
                backgroundColor: getComputedStyle(paper).backgroundColor,
                fontEmbedCSS,
            })
            if (!blob || blob.size === 0) throw new Error("采购单图片未生成")
            const sequence = String(index + 1).padStart(2, "0")
            zip.file(
                `${sequence}-${fileNamePart(preview.supplierName)}-采购单预览.png`,
                blob,
            )
            onProgress(index + 1)
        }

        const archive = await zip.generateAsync({ type: "blob" })
        const url = URL.createObjectURL(archive)
        const anchor = document.createElement("a")
        anchor.href = url
        anchor.download = `${fileNamePart(sourceOrder?.salesOrderNo ?? "供给分配")}-采购单预览图片.zip`
        document.body.appendChild(anchor)
        try {
            anchor.click()
        } finally {
            anchor.remove()
            // 给浏览器留出读取文件的时间，再释放对象 URL。
            window.setTimeout(() => URL.revokeObjectURL(url), 60_000)
        }
    } finally {
        root.unmount()
        host.remove()
    }
}
