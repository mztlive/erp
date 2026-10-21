import type { Worksheet } from "exceljs"

export const INQUIRY_DELIVERY_CHECK_HEADERS = [
    "需求数量",
    "已分配数量",
    "剩余数量",
    "分配状态",
] as const
export const INQUIRY_DELIVERY_HELPER_HEADERS = ["核对数量", "核对商品"] as const

// 与导入物理行上限一致，追加或复制的地址行也参与核对。
const LAST_ROW = 10000

export function inquiryDeliveryCheckFormulas(row: number): readonly string[] {
    const sku = `$B${row}`
    const quantities = `$Q$2:$Q$${LAST_ROW}`
    const keys = `$R$2:$R$${LAST_ROW}`
    // 前缀防止 00123 与 123 被条件求和当成同一数字；转义通配符按精确 SKU 匹配。
    const criterion = `SUBSTITUTE(SUBSTITUTE(SUBSTITUTE("sku:"&${sku},"~","~~"),"*","~*"),"?","~?")`
    return [
        `IF(${sku}="","",INDEX(InquirySelection[需求数量],MATCH(${sku},InquirySelection[SKU 编号],0)))`,
        `IF(${sku}="","",IF(COUNTIFS(${keys},${criterion},${quantities},"数量格式错误")>0,"数量格式错误",ROUND(SUMIF(${keys},${criterion},${quantities}),6)))`,
        `IF(${sku}="","",IFERROR(ROUND(M${row}-N${row},6),"数量格式错误"))`,
        `IF(${sku}="","",IFERROR(IF(1*M${row}<=0,"需求数量错误",IF(NOT(ISNUMBER(O${row})),"数量格式错误",IF(O${row}<0,"超出 "&TEXT(-O${row},"General"),IF(O${row}>0,"尚差 "&TEXT(O${row},"General"),"数量一致")))),"数量格式错误"))`,
        `IF(${sku}="","",IF(LEN(J${row}&"")=0,"",IFERROR(IF(AND(1*J${row}>0,ROUND(1*J${row},6)=1*J${row}),1*J${row},"数量格式错误"),"数量格式错误")))`,
        `IF(${sku}="","","sku:"&${sku})`,
    ]
}

export function addDeliveryChecks(sheet: Worksheet, row: number) {
    inquiryDeliveryCheckFormulas(row).forEach((formula, index) => {
        const cell = sheet.getCell(row, index + 13)
        cell.value = { formula, result: "" }
        cell.numFmt = "General"
        cell.fill = {
            type: "pattern",
            pattern: "solid",
            fgColor: { argb: "FFF2F2F2" },
        }
    })
}

export function colorDeliveryChecks(sheet: Worksheet) {
    // 分配数量与状态同时着色；不依赖颜色辨认，状态列保留文字和差额。
    for (const ref of [`J2:J${LAST_ROW}`, `M2:P${LAST_ROW}`]) {
        sheet.addConditionalFormatting({
            ref,
            rules: [
                [
                    'OR(LEFT($P2,2)="超出",ISNUMBER(SEARCH("错误",$P2)))',
                    "FFFDE9E7",
                    "FFB42318",
                ],
                ['LEFT($P2,2)="尚差"', "FFFFF3CD", "FF854D0E"],
                ['$P2="数量一致"', "FFDCFCE7", "FF166534"],
            ].map(([formula, background, foreground], index) => ({
                type: "expression" as const,
                priority: index + 1,
                formulae: [formula],
                style: {
                    fill: {
                        type: "pattern" as const,
                        pattern: "solid" as const,
                        fgColor: { argb: background },
                    },
                    font: { color: { argb: foreground } },
                },
            })),
        })
    }
}
