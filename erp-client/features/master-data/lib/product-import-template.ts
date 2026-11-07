import { downloadWorkbook } from "@/lib/xlsx"

/** 列序与后端 PRODUCT_IMPORT_HEADERS 一致，供应商成本与公司售价分别填写。 */
export const PRODUCT_IMPORT_HEADERS = [
    "产品编码",
    "产品条码",
    "产品主图",
    "产品副图",
    "产品副图",
    "品牌",
    "产品类别",
    "产品名称",
    "产品规格",
    "供应商代发成本价（含税，元）",
    "供应商集采成本价（含税，元）",
    "供应商集采起订量",
    "市场参考价（元）",
    "供应商编号",
    "供应商订货编码",
    "进项税率（%）",
    "代发价（含税）",
    "集采价（含税）",
    "集采起订量",
] as const

export async function createProductImportTemplate() {
    const { Workbook } = await import("exceljs")
    const workbook = new Workbook()
    const sheet = workbook.addWorksheet("对内")
    sheet.addRow([...PRODUCT_IMPORT_HEADERS])
    sheet.columns.forEach((column) => {
        column.width = 28
        column.numFmt = "@"
    })
    sheet.getRow(1).font = { bold: true }
    sheet.getRow(1).alignment = { vertical: "middle", wrapText: true }
    sheet.getRow(1).height = 40
    sheet.views = [{ state: "frozen", ySplit: 1 }]
    const guide = workbook.addWorksheet("填写说明")
    for (const text of [
        "在「对内」工作表填写，每行一个 SKU，最多 1000 行。保留第一行表头和列序。",
        "产品名称、产品类别、供应商编号、供应商订货编码、两项供应商成本价和进项税率必填；供应商须已录入且具备供给资格。",
        "产品编码相同、规格不同的行归入同一商品；已有 SKU 保留原价格，新增 SKU 使用本行价格。",
        "Q 列代发价（含税）、R 列集采价（含税）、S 列集采起订量对应商品编辑中的同名字段。价格以元填写，最多两位小数；起订量必须大于 0，最多六位小数。",
        "Q–S 列选填，留空表示未维护；填写价格不得为负数。集采价或集采起订量未维护时，不启用集采自动取价。",
        "J–L 列为供应商成本及供应商起订量，与 Q–S 列分别填写，不相互代填。供应商起订量留空按 1 处理。",
        "进项税率填写百分数，例如 13 表示 13%。新供给默认可供数量为 999，重复导入保留已有供给成本和数量。",
        "每行商品、SKU 和供给同时保存，失败时该行不写入；导入不创建库存。进度与逐行结果在「后台任务」查看。",
    ])
        guide.addRow([text])
    guide.getColumn(1).width = 110
    guide.getColumn(1).alignment = { wrapText: true, vertical: "middle" }
    guide.eachRow((row) => {
        row.height = 44
    })
    return workbook
}

export async function downloadProductImportTemplate() {
    const workbook = await createProductImportTemplate()
    await downloadWorkbook(workbook, "商品与供给导入模板.xlsx", {
        revokeDelayMs: 1000,
    })
}
