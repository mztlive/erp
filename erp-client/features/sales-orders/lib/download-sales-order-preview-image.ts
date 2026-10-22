import { getFontEmbedCSS, toBlob } from "html-to-image"

/** 下载完整纸张，尺寸不受预览容器滚动位置或祖先缩放影响。 */
export async function downloadSalesOrderPreviewImage(
    paper: HTMLElement,
    customerName: string,
): Promise<void> {
    await document.fonts.ready
    const width = Math.max(paper.offsetWidth, paper.scrollWidth)
    const height = Math.max(paper.offsetHeight, paper.scrollHeight)
    // 限制长单据的画布边长及面积，避免浏览器生成空白 PNG。
    const pixelRatio = Math.min(
        2,
        16_000 / Math.max(width, height),
        Math.sqrt(32_000_000 / (width * height)),
    )
    if (!width || !height || pixelRatio < 1)
        throw new Error("销售单过长，无法生成清晰的完整图片，请下载表格")
    const blob = await toBlob(paper, {
        width,
        height,
        pixelRatio,
        backgroundColor: getComputedStyle(paper).backgroundColor,
        fontEmbedCSS: await getFontEmbedCSS(paper),
        style: { margin: "0", boxShadow: "none", transform: "none" },
    })
    if (!blob?.size) throw new Error("销售单图片未生成，请重试")
    const name = customerName.replace(/[^\p{L}\p{N}_-]/gu, "_").slice(0, 60)
    const url = URL.createObjectURL(blob)
    const anchor = document.createElement("a")
    anchor.id = "sales-orders-submit-preview-image-file"
    anchor.href = url
    anchor.download = `销售单-${name || "未填写客户"}-提交预览.png`
    document.body.appendChild(anchor)
    try {
        anchor.click()
    } finally {
        anchor.remove()
        window.setTimeout(() => URL.revokeObjectURL(url), 60_000)
    }
}
