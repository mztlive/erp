import { describe, expect, it } from "vitest"
import ExcelJS from "exceljs"

describe("exceljs workbook", () => {
    it("embeds a png so cells can carry a product image", async () => {
        const workbook = new ExcelJS.Workbook()
        const sheet = workbook.addWorksheet("选品")
        const imageId = workbook.addImage({
            base64: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
            extension: "png",
        })
        sheet.addImage(imageId, {
            tl: { col: 0, row: 2 },
            ext: { width: 76, height: 76 },
            editAs: "oneCell",
        })
        sheet.getCell(3, 2).value = "礼盒"
        const buffer = await workbook.xlsx.writeBuffer()
        expect(buffer.byteLength).toBeGreaterThan(100)
    })
})
