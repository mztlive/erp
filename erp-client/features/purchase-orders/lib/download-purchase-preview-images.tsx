import { flushSync } from "react-dom"
import { createRoot } from "react-dom/client"
import { getFontEmbedCSS } from "html-to-image"
import JSZip from "jszip"

import { PurchaseOrderPreviewPaper } from "../components/purchase-order-create-preview-paper"
import type {
    PurchaseOrderPreview,
    SourcingSalesOrder,
} from "./purchase-order-create-model"
import {
    purchaseDeliveryWorkbookFileName,
    shouldBundleDeliveryWorkbook,
} from "./purchase-delivery-plan"
import { createPurchaseDeliveryWorkbookBlob } from "./purchase-delivery-workbook"
import {
    downloadPurchasePaperFile,
    purchasePaperFileNamePart,
    renderPurchasePaperImage,
} from "./purchase-paper-image"

export type PurchasePreviewDownloadResult = Readonly<{
    imageCount: number
    workbookCount: number
    archived: boolean
}>

/**
 * 按预览顺序逐张渲染供应商版纸张（隐藏来源销售客户资料）。
 * 地址较多时自动切换为主单清晰 PNG + 配套发货地址 Excel 表；仅单个文件时直下，不套 ZIP。
 */
export async function downloadPurchasePreviewImages(
    previews: readonly PurchaseOrderPreview[],
    sourceOrder: SourcingSalesOrder | undefined,
    onProgress: (completed: number) => void,
): Promise<PurchasePreviewDownloadResult> {
    if (previews.length === 0) {
        return { imageCount: 0, workbookCount: 0, archived: false }
    }

    const host = document.createElement("div")
    host.setAttribute("aria-hidden", "true")
    host.inert = true
    // 脱离缩放视口，使用纸张原始宽度；不使用 display:none，以保留真实排版。
    Object.assign(host.style, {
        position: "fixed",
        left: "-10000px",
        top: "0",
        width: "840px",
        pointerEvents: "none",
    })
    document.body.appendChild(host)
    const root = createRoot(host)

    const files: Array<{ filename: string; blob: Blob }> = []
    let workbookCount = 0

    try {
        let fontEmbedCSS: string | undefined
        const multipleOrders = previews.length > 1

        for (const [index, preview] of previews.entries()) {
            const warehouse = preview.fulfillmentResponsibility === "WAREHOUSE"
            const needsWorkbook = shouldBundleDeliveryWorkbook(
                preview.lines,
                warehouse,
            )
            const sequence = multipleOrders
                ? String(index + 1).padStart(2, "0")
                : undefined
            const supplierPart = purchasePaperFileNamePart(preview.supplierName)
            const imageFilename = sequence
                ? `${sequence}-${supplierPart}-采购单预览.png`
                : `${supplierPart}-采购单预览.png`
            const workbookFilename = needsWorkbook
                ? purchaseDeliveryWorkbookFileName(
                      preview.supplierName,
                      sequence,
                  )
                : undefined

            flushSync(() => {
                root.render(
                    <PurchaseOrderPreviewPaper
                        preview={preview}
                        index={index}
                        sourceOrder={sourceOrder}
                        audience="supplier"
                        deliveryMode={
                            needsWorkbook ? "export-summary" : "export-full"
                        }
                        deliveryAttachmentName={workbookFilename}
                    />,
                )
            })
            await document.fonts.ready
            const paper = host.querySelector<HTMLElement>(
                "[data-slot='paper-document']",
            )
            if (!paper) throw new Error("采购单图片未生成")
            fontEmbedCSS ??= await getFontEmbedCSS(paper)
            const imageBlob = await renderPurchasePaperImage(
                paper,
                fontEmbedCSS,
            )
            files.push({ filename: imageFilename, blob: imageBlob })

            if (needsWorkbook && workbookFilename) {
                const workbookBlob = await createPurchaseDeliveryWorkbookBlob({
                    documentLabel: `待确认 ${index + 1}`,
                    supplierName: preview.supplierName,
                    lines: preview.lines,
                })
                files.push({ filename: workbookFilename, blob: workbookBlob })
                workbookCount += 1
            }

            onProgress(index + 1)
        }

        if (files.length === 1) {
            downloadPurchasePaperFile(files[0].blob, files[0].filename)
            return {
                imageCount: previews.length,
                workbookCount,
                archived: false,
            }
        }

        const zip = new JSZip()
        for (const file of files) {
            zip.file(file.filename, file.blob)
        }
        const archive = await zip.generateAsync({ type: "blob" })
        const archiveBase = purchasePaperFileNamePart(
            previews.length === 1
                ? previews[0].supplierName
                : (sourceOrder?.salesOrderNo ?? "供给分配"),
        )
        const archiveSuffix =
            workbookCount > 0 ? "采购单及发货地址" : "采购单预览图片"
        downloadPurchasePaperFile(
            archive,
            `${archiveBase}-${archiveSuffix}.zip`,
        )
        return {
            imageCount: previews.length,
            workbookCount,
            archived: true,
        }
    } finally {
        root.unmount()
        host.remove()
    }
}
