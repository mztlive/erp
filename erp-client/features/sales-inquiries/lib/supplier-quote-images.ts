import { fetchFileAssetPreviewBlob } from "@/features/file-assets/api"
import { mapWithConcurrency } from "@/features/master-data/lib/sellable-excel-rows"
import type { SupplierQuoteGroup } from "./supplier-quote-export"

export type SupplierQuoteImage = {
    base64: string
    width: number
    height: number
}

/** 每个主图只读取一次；有图但读取失败时阻止下载，不能伪装成没有图片。 */
export async function loadSupplierQuoteImages(
    groups: readonly SupplierQuoteGroup[],
) {
    const ids = [
        ...new Set(
            groups.flatMap((group) =>
                group.lines.flatMap((line) =>
                    line.imageAssetId ? [line.imageAssetId] : [],
                ),
            ),
        ),
    ]
    const images = await mapWithConcurrency(ids, 4, async (id) => {
        const blob = await fetchFileAssetPreviewBlob(id)
        const bitmap = await createImageBitmap(blob)
        try {
            const scale = Math.min(
                1,
                152 / Math.max(bitmap.width, bitmap.height),
            )
            const canvas = document.createElement("canvas")
            canvas.width = Math.max(1, Math.round(bitmap.width * scale))
            canvas.height = Math.max(1, Math.round(bitmap.height * scale))
            const context = canvas.getContext("2d")
            if (!context) throw new Error("商品图片转换失败，请重试生成报价表")
            context.fillStyle = "#ffffff"
            context.fillRect(0, 0, canvas.width, canvas.height)
            context.drawImage(bitmap, 0, 0, canvas.width, canvas.height)
            const displayScale = Math.min(
                1,
                72 / Math.max(canvas.width, canvas.height),
            )
            return [
                id,
                {
                    base64: canvas.toDataURL("image/jpeg", 0.86),
                    width: canvas.width * displayScale,
                    height: canvas.height * displayScale,
                },
            ] as const
        } finally {
            bitmap.close()
        }
    })
    return new Map(images)
}
