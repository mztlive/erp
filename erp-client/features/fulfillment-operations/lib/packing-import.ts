import type { Cell } from "exceljs"
import { workbookBlob } from "@/lib/xlsx"
import { compactFixed, compareDecimal } from "@/lib/fixed-decimal"
import { CARRIER_OPTIONS } from "@/lib/business-options"
import {
    packingIssues,
    unpackedItems,
    type DeliveryAddressSnapshot,
    type DeliveryPackage,
    type DeliveryPacking,
} from "./delivery-packing"

const sheetName = "发货包裹"
const metadataName = "模板信息"
const templateVersion = "fulfillment-packages-v1"
const headers = [
    "收货人",
    "联系电话",
    "收货地址",
    "商品",
    "本次发货数量",
    "包裹数量",
    "物流公司",
    "物流单号",
    "地址标识",
    "商品标识",
] as const

export type PackingImportContext = {
    operationId: string
    source: DeliveryAddressSnapshot
    packing: DeliveryPacking
    itemName: (id: string) => string
}

export type PackingImportPreview = {
    packing: DeliveryPacking
    addresses: Array<{ key: string; before: number; after: number }>
    rows: number
}

function checkSource({ source, packing }: PackingImportContext) {
    if (source.source_version !== packing.source_version)
        throw new Error("收货安排已变化，请刷新后重新下载模板")
}

/** 模板保留真实地址及商品身份；名称和收件信息只供核对，不作模糊匹配。 */
export async function createPackingWorkbook(context: PackingImportContext) {
    checkSource(context)
    const { Workbook } = await import("exceljs")
    const workbook = new Workbook()
    const sheet = workbook.addWorksheet(sheetName)
    sheet.addRow([...headers])
    const widths = [16, 20, 48, 32, 18, 16, 20, 28, 20, 20]
    sheet.columns.forEach((column, index) => {
        column.width = widths[index]
        column.numFmt = "@"
        column.hidden = index >= 8
    })
    sheet.views = [
        { state: "frozen", xSplit: 3, ySplit: 1, showGridLines: false },
    ]
    sheet.getRow(1).font = { name: "Microsoft YaHei", bold: true, size: 11 }
    for (const destination of context.source.destinations) {
        if (destination.pending_address) continue
        const parcels = context.packing.packages.filter(
            (parcel) => parcel.destination_key === destination.key,
        )
        const rows = [
            ...parcels.flatMap((parcel) =>
                parcel.items.map((item) => ({
                    ...item,
                    carrier: parcel.carrier,
                    tracking: parcel.tracking_no,
                })),
            ),
            ...unpackedItems(context.packing, destination.key).map((item) => ({
                ...item,
                carrier: "",
                tracking: "",
            })),
        ]
        for (const item of rows) {
            const dispatch = context.packing.dispatch
                .find((row) => row.destination_key === destination.key)
                ?.items.find(
                    (row) => row.delivery_line_id === item.delivery_line_id,
                )
            sheet.addRow([
                destination.recipient,
                destination.phone,
                destination.address,
                context.itemName(item.delivery_line_id),
                dispatch?.quantity ?? "0",
                item.quantity,
                item.carrier,
                item.tracking,
                destination.key,
                item.delivery_line_id,
            ])
        }
    }
    if (sheet.rowCount === 1)
        throw new Error("没有可导出的发货商品，请先补齐地址并填写本次发货数量")
    sheet.eachRow((row, index) => {
        if (index > 1) row.font = { name: "Microsoft YaHei", size: 11 }
        row.alignment = { vertical: "middle", wrapText: true }
    })
    const notes = workbook.addWorksheet("填写说明")
    for (const note of [
        "仅替换文件中出现的地址的全部包裹，其他地址保留。请整行删除不需要覆盖的地址，不能仅清空物流。",
        "每行是一件包裹中的一种商品。同地址、同物流公司、同单号自动合为一个包裹；拆包时复制整行（包含隐藏列），分别填写单号和数量。",
        "只修改包裹数量、物流公司和物流单号。收货人、电话、地址、商品及隐藏列不得修改；不支持新增地址。",
        "每个导入地址必须包含本次发货的全部商品，各商品的包裹数量合计必须等于页面上的本次发货数量。部分发货请先在页面修改本次数量，再下载模板。",
        "物流公司和单号必填。电话、物流单号必须保留文本格式，不能使用公式。数量必须大于0，最多6位小数。",
        `常用物流公司：${CARRIER_OPTIONS.map((option) => option.label).join("、")}；其他物流公司可直接填写名称。`,
        "仅支持当前任务导出的 .xlsx，文件最多5 MB，每次最多100行包裹商品；覆盖后整个任务的包裹商品关联仍不得超过100项。",
        "上传后核对覆盖预览，确认后更新页面录入；须再保存草稿或确认发货。任何一行校验失败，整次导入不生效。",
    ])
        notes.addRow([note])
    notes.getColumn(1).width = 110
    notes.eachRow((row) => {
        row.alignment = { wrapText: true, vertical: "middle" }
        row.height = 44
    })
    const metadata = workbook.addWorksheet(metadataName, {
        state: "veryHidden",
    })
    metadata.addRow([
        templateVersion,
        context.operationId,
        context.source.source_version,
    ])
    return workbookBlob(workbook)
}

/** 拒绝公式、日期、错误和数值型身份字段，避免物流号或电话精度丢失后静默导入。 */
function cellText(cell: Cell, numeric = false): string {
    const value = cell.value
    if (value == null) return ""
    if (typeof value === "string") return value.trim()
    if (numeric && typeof value === "number" && Number.isFinite(value))
        return String(value)
    throw new Error(
        `第${cell.row}行「${headers[Number(cell.col) - 1] ?? "模板信息"}」须使用${numeric ? "数字或文本" : "文本"}，公式请先粘贴为值`,
    )
}

/** 完整校验后生成候选草稿；未出现的地址连同未完成录入原样保留。 */
export async function readPackingWorkbook(
    file: File,
    context: PackingImportContext,
): Promise<PackingImportPreview> {
    checkSource(context)
    if (!/\.xlsx$/i.test(file.name))
        throw new Error("请选择从当前任务下载的 .xlsx 模板")
    if (file.size > 5 * 1024 * 1024) throw new Error("文件不能超过5 MB")
    const { Workbook } = await import("exceljs")
    const workbook = new Workbook()
    try {
        await workbook.xlsx.load(await file.arrayBuffer())
    } catch {
        throw new Error(
            "无法读取 Excel，请确认文件未损坏且未加密，并另存为 .xlsx 后重试",
        )
    }
    const metadata = workbook.getWorksheet(metadataName)
    if (
        !metadata ||
        cellText(metadata.getCell("A1")) !== templateVersion ||
        cellText(metadata.getCell("B1")) !== context.operationId ||
        cellText(metadata.getCell("C1")) !== context.source.source_version
    )
        throw new Error("模板不属于当前任务或收货安排已变化，请重新下载模板")
    const sheet = workbook.getWorksheet(sheetName)
    if (
        !sheet ||
        headers.some(
            (header, index) =>
                cellText(sheet.getRow(1).getCell(index + 1)) !== header,
        )
    )
        throw new Error("表头与模板不一致，请重新下载模板并保留列顺序")
    if (sheet.columnCount > headers.length || sheet.rowCount > 20001)
        throw new Error("表格超出模板范围，请清理多余行列")
    const groups = new Map<string, DeliveryPackage>()
    const addresses = new Set<string>()
    let rows = 0
    sheet.eachRow((row, rowNumber) => {
        if (rowNumber === 1) return
        const cells = headers.map((_, index) =>
            cellText(row.getCell(index + 1), index === 4 || index === 5),
        )
        if (cells.every((cell) => !cell)) return
        if (++rows > 100)
            throw new Error(
                "每次最多导入100行包裹商品，请删除本次不需要覆盖的地址",
            )
        const [
            recipient,
            phone,
            address,
            name,
            required,
            quantity,
            carrier,
            tracking,
            key,
            lineId,
        ] = cells
        const fail = (message: string): never => {
            throw new Error(`第${rowNumber}行：${message}`)
        }
        const destination = context.source.destinations.find(
            (item) => item.key === key,
        )
        if (!destination || destination.pending_address)
            return fail("地址不属于当前任务或尚未补齐，请重新下载模板")
        if (
            recipient !== destination.recipient.trim() ||
            phone !== destination.phone.trim() ||
            address !== destination.address.trim()
        )
            fail("收货信息与当前地址不一致，请勿在包裹模板中修改地址")
        if (
            !destination.items.some(
                (item) => item.delivery_line_id === lineId,
            ) ||
            name !== context.itemName(lineId).trim()
        )
            fail("商品不属于该地址或商品资料被修改，请复制模板中的完整商品行")
        const dispatch = context.packing.dispatch
            .find((item) => item.destination_key === key)
            ?.items.find((item) => item.delivery_line_id === lineId)
        try {
            if (
                !dispatch ||
                compareDecimal(required, dispatch.quantity, 6) !== 0
            )
                fail("本次发货数量已变化或被修改，请重新下载模板")
            if (compareDecimal(quantity, "0", 6) <= 0) fail("包裹数量必须大于0")
        } catch (reason) {
            if (
                reason instanceof Error &&
                reason.message.startsWith(`第${rowNumber}行：`)
            )
                throw reason
            fail("请填写有效数量，最多6位小数")
        }
        if (!carrier || !tracking)
            fail("请填写物流公司和物流单号；不覆盖该地址时请删除该地址的全部行")
        if ([...carrier].length > 64 || [...tracking].length > 128)
            fail("物流公司最多64字，物流单号最多128字")
        const trackingKey = JSON.stringify([carrier, tracking])
        let parcel = groups.get(trackingKey)
        if (parcel && parcel.destination_key !== key)
            fail("同一物流包裹不能发往不同地址")
        if (!parcel) {
            parcel = {
                id: crypto.randomUUID(),
                destination_key: key,
                carrier,
                tracking_no: tracking,
                items: [],
            }
            groups.set(trackingKey, parcel)
        }
        if (parcel.items.some((item) => item.delivery_line_id === lineId))
            fail("同一包裹的商品重复，请合并数量或使用不同物流单号")
        parcel.items.push({
            delivery_line_id: lineId,
            quantity: compactFixed(quantity),
        })
        addresses.add(key)
    })
    if (!rows) throw new Error("文件没有包裹数据，请按模板填写后重试")
    const imported = [...groups.values()]
    for (const key of addresses) {
        const issues = packingIssues(
            {
                ...context.packing,
                dispatch: context.packing.dispatch.filter(
                    (row) => row.destination_key === key,
                ),
                packages: imported.filter(
                    (parcel) => parcel.destination_key === key,
                ),
            },
            context.source,
        )
        if (issues.length) {
            const destination = context.source.destinations.find(
                (row) => row.key === key,
            )!
            throw new Error(
                `${destination.recipient}（${destination.address}）：${issues.map((issue) => issue.message).join("；")}`,
            )
        }
    }
    const retained = context.packing.packages.filter(
        (parcel) => !addresses.has(parcel.destination_key),
    )
    if (
        retained.some((parcel) =>
            groups.has(
                JSON.stringify([
                    parcel.carrier.trim(),
                    parcel.tracking_no.trim(),
                ]),
            ),
        )
    )
        throw new Error(
            "导入物流单号与未覆盖地址的现有包裹重复，请核对物流公司和单号",
        )
    const packages = [...retained, ...imported]
    if (
        context.packing.dispatch.length > 100 ||
        packages.length > 100 ||
        packages.flatMap((parcel) => parcel.items).length > 100
    )
        throw new Error(
            "覆盖后整个任务超过100个地址、包裹或包裹商品关联，请减少拆包",
        )
    return {
        packing: { ...context.packing, packages },
        rows,
        addresses: [...addresses].map((key) => ({
            key,
            before: context.packing.packages.filter(
                (parcel) => parcel.destination_key === key,
            ).length,
            after: imported.filter((parcel) => parcel.destination_key === key)
                .length,
        })),
    }
}
