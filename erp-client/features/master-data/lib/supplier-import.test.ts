import { describe, expect, it } from "vitest"
import { Workbook } from "exceljs"
import {
    SUPPLIER_IMPORT_HEADERS,
    supplierCellText,
    supplierFailuresWorkbook,
    supplierWorkbookRows,
} from "./supplier-import"

describe("供应商模板读取", () => {
    it.each([
        ["公式地址", { formula: '"原始地址"', result: "原始地址" }, "formula"],
        ["错误内容", { error: "#VALUE!" }, "#VALUE!"],
    ] as const)(
        "失败导出保留%s，未经修正不得重导",
        async (_, value, original) => {
            const source = new Workbook()
            const sheet = source.addWorksheet("供应商")
            sheet.addRow([...SUPPLIER_IMPORT_HEADERS])
            sheet.getCell("B2").value = "完整供应商"
            sheet.getCell("H2").value = "公司甲"
            sheet.getCell("J2").value = "月结"
            sheet.getCell("S2").value = "普票"
            sheet.getCell("G2").value = value
            const rows = supplierWorkbookRows(source)
            expect(rows[0].parse_errors).not.toHaveLength(0)
            const failure = await supplierFailuresWorkbook(rows, [
                {
                    row_number: 2,
                    name: "完整供应商",
                    status: "failed",
                    message: "读取失败",
                    supplier_id: null,
                    supplier_no: null,
                },
            ])
            const read = new Workbook()
            await read.xlsx.load(await failure.xlsx.writeBuffer())
            const cell = read.worksheets[0].getCell("G2")
            expect(typeof cell.value).toBe("string")
            expect(cell.value).toContain(original)
            expect(supplierWorkbookRows(read)[0].parse_errors).not.toHaveLength(
                0,
            )
            const originalText = cell.value
            cell.value = "已核实地址"
            expect(supplierWorkbookRows(read)[0].parse_errors).not.toHaveLength(
                0,
            )
            cell.value = originalText
            // 删除错误汇总不能绕过原单元格的未修正标记。
            read.worksheets[0].getCell("Z2").value = ""
            expect(supplierWorkbookRows(read)[0].parse_errors).not.toHaveLength(
                0,
            )
            cell.value = "已核实地址"
            const repaired = supplierWorkbookRows(read)[0]
            expect(repaired.parse_errors).toEqual([])
            expect(repaired.cells[6]).toBe("已核实地址")
        },
    )
    it("保留原行号、文本银行账号及多个税率", () => {
        const workbook = new Workbook()
        const sheet = workbook.addWorksheet("供应商")
        sheet.addRow([...SUPPLIER_IMPORT_HEADERS])
        sheet.getCell("B3").value = "测试供应商"
        sheet.getCell("E3").value = "001234567890123456789"
        sheet.getCell("T3").value = "9%，13%"
        const [row] = supplierWorkbookRows(workbook)
        expect(row.row_number).toBe(3)
        expect(row.cells[4]).toBe("001234567890123456789")
        expect(row.cells[19]).toBe("9%，13%")
        expect(row.supplier_no).toMatch(/^SUP-[A-Z0-9]+$/)
        expect(row.parse_errors).toEqual([])
    })
    it("原生百分比按百分数读取，普通数字不会误放大", () => {
        expect(supplierCellText(0.13, 19, "0%")).toBe("13%")
        expect(supplierCellText(9, 19, "General")).toBe("9")
    })
    it("长数字账号和公式只标记所属失败行", () => {
        const workbook = new Workbook()
        const sheet = workbook.addWorksheet("供应商")
        sheet.addRow([...SUPPLIER_IMPORT_HEADERS])
        sheet.getCell("B2").value = "数字账号"
        sheet.getCell("E2").value = Number("123456789012345670")
        sheet.getCell("B3").value = "公式"
        sheet.getCell("E3").value = { formula: "1+1", result: 2 }
        const rows = supplierWorkbookRows(workbook)
        expect(rows[0].parse_errors.join()).toContain("数字精度")
        expect(rows[1].parse_errors.join()).toContain("公式")
    })
    it("拒绝表头漂移和无数据工作簿", () => {
        expect(() => supplierWorkbookRows(new Workbook())).toThrow("表头")
        const workbook = new Workbook()
        workbook.addWorksheet("供应商").addRow([...SUPPLIER_IMPORT_HEADERS])
        expect(() => supplierWorkbookRows(workbook)).toThrow("1–500")
    })
    it("失败工作簿只含失败行且保留文本账号、原行号和错误", async () => {
        const workbook = new Workbook()
        const sheet = workbook.addWorksheet("供应商")
        sheet.addRow([...SUPPLIER_IMPORT_HEADERS])
        sheet.getCell("B3").value = "失败供应商"
        sheet.getCell("E3").value = "001234567890123456789"
        sheet.getCell("B4").value = "成功供应商"
        const rows = supplierWorkbookRows(workbook)
        const failed = await supplierFailuresWorkbook(rows, [
            {
                row_number: 3,
                name: "失败供应商",
                status: "failed",
                message: "缺少结算方式",
                supplier_id: null,
                supplier_no: null,
            },
            {
                row_number: 4,
                name: "成功供应商",
                status: "succeeded",
                message: "供应商已导入",
                supplier_id: "s1",
                supplier_no: "SUP-test",
            },
        ])
        const read = new Workbook()
        await read.xlsx.load(await failed.xlsx.writeBuffer())
        const result = read.worksheets[0]
        expect(result.rowCount).toBe(2)
        expect(result.getCell("E2").value).toBe("001234567890123456789")
        expect(result.getCell("X2").value).toBe("3")
        expect(result.getCell("Y2").value).toBe("缺少结算方式")
        expect(supplierWorkbookRows(read)).toHaveLength(1)
    })
})
