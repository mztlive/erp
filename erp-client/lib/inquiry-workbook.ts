import type { Workbook, Worksheet } from "exceljs"
import { appendPoolDeliverySheet } from "@/lib/inquiry-pool-delivery"

export const INQUIRY_TEMPLATE_VERSION = "sales-inquiry-v1"
export const INQUIRY_POOL_TEMPLATE_VERSION = "sales-inquiry-pool-v4"
export const INQUIRY_IMPORT_MAX_BYTES = 5 * 1024 * 1024
export const INQUIRY_IMPORT_MAX_DELIVERY_ROWS = 2000
export const INQUIRY_IMPORT_MAX_REQUIREMENT_ROWS = 200
export const INQUIRY_IMPORT_MAX_ADDRESSES = 100
export const INQUIRY_WORKBOOK_MIME =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"

export const INQUIRY_PRODUCT_HEADERS = [
    "SKU编码",
    "商品名称",
    "规格",
    "单位",
    "需求数量",
    "销售价",
    "需求备注",
] as const

export const INQUIRY_DELIVERY_HEADERS = [
    "SKU编码",
    "收件人",
    "联系电话",
    "省",
    "市",
    "区县",
    "详细地址",
    "配送数量",
    "要求交付日期",
    "交付条件",
] as const

/** 只接收客户模板接口返回的商品快照，不接收商品池内部导出行。 */
export type InquiryCustomerWorkbookProduct = Readonly<{
    sku_code: string
    product_name: string
    specification: string
    unit: string
    delivery_key?: string
    inquiry_supported?: boolean
    quantity?: string | null
    sales_reference_price?: string | null
    remark?: string | null
}>

function configureSheet(
    sheet: Worksheet,
    headers: readonly string[],
    widths: readonly number[],
) {
    sheet.addRow([...headers])
    headers.forEach((_, index) => {
        const column = sheet.getColumn(index + 1)
        column.width = widths[index] ?? 22
        column.numFmt = "@"
    })
    sheet.getRow(1).font = { bold: true }
    sheet.getRow(1).height = 24
    sheet.autoFilter = {
        from: { row: 1, column: 1 },
        to: { row: 1, column: headers.length },
    }
}

/**
 * ExcelJS 的字符串单元格写为 shared string，不构造公式对象；即使文本以
 * =、+、-、@ 开头，Excel 也按文本保存，并完整保留 SKU 与电话前导零。
 */
function addTextRow(sheet: Worksheet, values: readonly string[]) {
    const row = sheet.addRow([...values])
    row.alignment = { vertical: "top", wrapText: true }
    values.forEach((value, index) => {
        const cell = row.getCell(index + 1)
        cell.numFmt = "@"
        cell.value = String(value)
    })
}

/** 只对白名单客户字段生成工作簿；调用方须先重新获取当前授权的模板数据。 */
export async function buildInquiryCustomerWorkbook(
    products: readonly InquiryCustomerWorkbookProduct[],
): Promise<Blob> {
    if (!products.length) throw new Error("请选择至少一个商品后下载客户模板")
    if (products.length > INQUIRY_IMPORT_MAX_REQUIREMENT_ROWS)
        throw new Error("模板最多包含 200 个商品，请减少后重新下载")
    if (new Set(products.map((item) => item.sku_code)).size !== products.length)
        throw new Error("商品清单含重复 SKU，请刷新商品选择后重新下载")

    const { Workbook } = await import("exceljs")
    const workbook = new Workbook()
    workbook.creator = "客户选品"
    workbook.subject = INQUIRY_TEMPLATE_VERSION
    workbook.calcProperties.fullCalcOnLoad = false

    const productSheet = workbook.addWorksheet("商品清单", {
        views: [{ state: "frozen", ySplit: 1 }],
    })
    configureSheet(
        productSheet,
        INQUIRY_PRODUCT_HEADERS,
        [24, 32, 30, 12, 16, 16, 38],
    )
    for (const product of products)
        addTextRow(productSheet, [
            product.sku_code,
            product.product_name,
            product.specification,
            product.unit,
            product.quantity ?? "",
            product.sales_reference_price ?? "",
            product.remark ?? "",
        ])

    appendInquiryDeliverySheets(workbook, products)

    const buffer = await workbook.xlsx.writeBuffer()
    const blob = new Blob([new Uint8Array(buffer as ArrayBuffer)], {
        type: INQUIRY_WORKBOOK_MIME,
    })
    if (blob.size > INQUIRY_IMPORT_MAX_BYTES)
        throw new Error("生成的模板超过 5 MiB，请减少商品数量后重新下载")
    return blob
}

export async function downloadInquiryCustomerWorkbook(
    products: readonly InquiryCustomerWorkbookProduct[],
    inquiry_no?: string,
): Promise<void> {
    const blob = await buildInquiryCustomerWorkbook(products)
    const url = URL.createObjectURL(blob)
    const link = document.createElement("a")
    link.id = "sales-inquiry-customer-workbook-download"
    link.href = url
    link.download = `客户选品-${inquiry_no?.replace(/[\\/:*?"<>|]/g, "-") || "询价"}.xlsx`
    link.click()
    window.setTimeout(() => URL.revokeObjectURL(url), 1000)
}

/** 为原商品池导出和询价模板追加同一套配送填写页。 */
export function appendInquiryDeliverySheets(
    workbook: Workbook,
    products: readonly InquiryCustomerWorkbookProduct[],
    pool = false,
) {
    if (pool) appendPoolDeliverySheet(workbook, products)
    else appendStaticDeliverySheet(workbook, products)

    const notes = workbook.addWorksheet("填写说明")
    notes.getColumn(1).width = 22
    notes.getColumn(2).width = 112
    const instructions: readonly (readonly [string, string])[] = [
        [
            "模板版本",
            pool ? INQUIRY_POOL_TEMPLATE_VERSION : INQUIRY_TEMPLATE_VERSION,
        ],
        ["文件限制", "仅支持 XLSX 文件，大小不超过 5 MiB（5,242,880 字节）。"],
        [
            "行数限制",
            "配送需求最多 2,000 行；按 SKU、交期及交付条件分组后最多 200 条需求，每条需求最多 100 个地址。超限须拆分后重新询价。",
        ],
        [
            pool ? "选品" : "商品清单",
            pool
                ? "在选品页填写需求数量即表示选中；未填数量的商品不导入询价。需求数量、销售价和需求备注由客户或销售填写，原有三种展示价格不会自动作为询价销售价。"
                : "每个 SKU 仅填写一行。保留 SKU 编码、商品名称、规格和单位；不得新增列或填写供应商、内部报价、采购责任等资料。",
        ],
        [
            "配送需求",
            pool
                ? "每个商品默认一个配送行。选品页填写需求数量后，自动带出商品信息，配送数量默认等于需求数量。一个地址填一行；省、市、区县分别填写完整名称。"
                : "每行填写一个 SKU、一个收件地址和一组交付要求。省、市、区县分别填写完整名称，不要合并单元格。",
        ],
        [
            "编号与电话",
            "SKU 编码和联系电话必须按文本填写，保留前导零。不要改为数字或科学计数法。",
        ],
        [
            "非实物商品",
            "虚拟商品或服务只填写 SKU 编码、配送数量、要求交付日期和交付条件；不需要收件地址时，收件人、电话、省市区县和详细地址全部留空。实物商品必须完整填写收件地址。",
        ],
        [
            "数量与销售价",
            "数量和销售价使用十进制文本，不加千分位或货币符号。每个 SKU 的需求数量须等于该 SKU 全部配送数量之和。销售价用于询价需求，成交价在销售开单时确认。",
        ],
        [
            "交期与条件",
            "要求交付日期使用 YYYY-MM-DD。同一 SKU 不同日期或交付条件分别保留；不要只按 SKU 合并。",
        ],
        [
            "重复行",
            "收件信息与交付要求完全相同的配送行会在导入预览中提示合并，确认后才写入询价草稿。",
        ],
        [
            "公式处理",
            pool
                ? "系统自动带出的商品资料和配送数量可直接导入，无须转值。请勿修改自动公式；自行添加的其他公式须先选择性粘贴为值。"
                : "不接受公式单元格。请先复制公式计算结果，再选择性粘贴为值；不得只保留公式缓存值。",
        ],
        [
            "导入确认",
            "销售上传后先查看逐行校验和合并结果。存在错误时修正文件并重新预览，不会部分写入。",
        ],
    ]
    for (const instruction of instructions) addTextRow(notes, instruction)
    if (pool) {
        addTextRow(notes, [
            "多个收货地址",
            "点击左侧行号复制该商品整行到配送页末尾，保留隐藏核对列，分别填写地址，并将原行和新增行的数量都改为实际分配值。例如需求 100，原行填 60、新增行填 40。各行合计须等于需求数量；修改选品数量后须重新核对分配。",
        ])
        addTextRow(notes, [
            "数量核对与颜色",
            "配送数量和右侧核对区：超量标红并显示超出数量，不足标黄并显示尚差数量，数量一致标绿。需求数量、已分配数量、剩余数量按 SKU 汇总，包括复制追加的地址行。绿色仅表示数量一致，不表示地址完整或通过导入校验；格式错误须修正。核对列无需修改。",
        ])
        addTextRow(notes, [
            "筛选与取消选品",
            "可在配送页筛选 SKU 编号非空的行。取消选品后，已填地址仍保留，须核对并清空该行配送内容；系统不会静默删除已填资料。",
        ])
        addTextRow(notes, [
            "发送客户前",
            "本文件保留公司商品池原有全部列，包括供应商编号及供应保障；销售须自行删除不对客的信息后再发送。可删除供应商等展示列，但须保留商品名称、SKU 编号、SPU 编号、需求数量、销售价、需求备注及隐藏的单位编码、配送关联列。",
        ])
        addTextRow(notes, [
            "卡券",
            "卡券保留在选品清单中供查看，但不适用销售询价；请勿填写其需求数量或配送需求。",
        ])
    }
    notes.getRow(1).font = { bold: true }
}

function appendStaticDeliverySheet(
    workbook: Workbook,
    products: readonly InquiryCustomerWorkbookProduct[],
) {
    const deliverySheet = workbook.addWorksheet("配送需求", {
        views: [{ state: "frozen", ySplit: 1 }],
    })
    configureSheet(
        deliverySheet,
        INQUIRY_DELIVERY_HEADERS,
        [24, 18, 22, 18, 18, 18, 42, 16, 20, 38],
    )
    for (const product of products)
        addTextRow(deliverySheet, [
            product.sku_code,
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
        ])
}
