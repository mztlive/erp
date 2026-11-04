import {
    downloadPurchasePaperFile,
    purchasePaperFileNamePart,
    renderPurchasePaperImage,
} from "./purchase-paper-image"

/** 使用独立文档保留纸质采购单样式，将完整内容导出为供应商命名的 PNG。 */
export async function downloadPurchasePaperImage(
    html: string,
    supplierName: string,
): Promise<void> {
    const frame = document.createElement("iframe")
    frame.title = "采购单图片生成"
    frame.setAttribute("aria-hidden", "true")
    frame.setAttribute("sandbox", "allow-same-origin")
    frame.inert = true
    // 保留实际布局并隔离页面样式；不允许单据执行脚本。
    Object.assign(frame.style, {
        position: "fixed",
        left: "-10000px",
        top: "0",
        width: "1048px",
        height: "1px",
        border: "0",
        pointerEvents: "none",
    })

    try {
        await new Promise<void>((resolve, reject) => {
            const timeout = window.setTimeout(
                () => reject(new Error("采购单加载超时，请重试")),
                15_000,
            )
            frame.onload = () => {
                window.clearTimeout(timeout)
                resolve()
            }
            frame.onerror = () => {
                window.clearTimeout(timeout)
                reject(new Error("采购单加载失败，请重试"))
            }
            frame.srcdoc = html
            document.body.appendChild(frame)
        })
        const paperDocument = frame.contentDocument
        if (!paperDocument?.body) throw new Error("采购单图片未生成")
        await paperDocument.fonts.ready
        const paper = paperDocument.body
        // 去掉页面外边距，保留纸张内部留白，按完整内容高度导出长图。
        paper.style.margin = "0"
        paper.style.boxSizing = "border-box"
        paper.style.maxWidth = "none"
        // 独立打印文档使用白纸背景，不受应用深浅主题影响。
        paper.style.backgroundColor = "white"
        const blob = await renderPurchasePaperImage(paper)
        downloadPurchasePaperFile(
            blob,
            `${purchasePaperFileNamePart(supplierName)}.png`,
        )
    } finally {
        frame.remove()
    }
}
