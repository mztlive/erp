import { toBlob } from "html-to-image"
import { downloadBlob } from "@/lib/download-blob"

export function purchasePaperFileNamePart(value: string): string {
    return (
        Array.from(value, (character) =>
            character.charCodeAt(0) < 32 ? "_" : character,
        )
            .join("")
            .replace(/[\\/:*?"<>|]/g, "_")
            .trim()
            .slice(0, 80)
            .replace(/[. ]+$/, "") || "采购单"
    )
}

/** 按完整纸张尺寸生成两倍分辨率 PNG，支持批量导出时复用字体。 */
export async function renderPurchasePaperImage(
    paper: HTMLElement,
    fontEmbedCSS?: string,
): Promise<Blob> {
    await paper.ownerDocument.fonts.ready
    const blob = await toBlob(paper, {
        width: Math.max(paper.offsetWidth, paper.scrollWidth),
        height: Math.max(paper.offsetHeight, paper.scrollHeight),
        pixelRatio: 2,
        backgroundColor: getComputedStyle(paper).backgroundColor,
        fontEmbedCSS,
    })
    if (!blob || blob.size === 0) throw new Error("采购单图片未生成")
    return blob
}

export function downloadPurchasePaperFile(blob: Blob, filename: string): void {
    downloadBlob(blob, filename, {
        linkId: "purchase-order-paper-file",
        revokeDelayMs: 60_000,
    })
}
