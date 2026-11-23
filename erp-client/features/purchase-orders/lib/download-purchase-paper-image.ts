import JSZip from "jszip"

import {
    buildPurchaseDeliveryPlanSummary,
    IMAGE_INLINE_ADDRESS_LIMIT,
    purchaseDeliveryWorkbookFileName,
    type PurchaseDeliveryPlanLine,
} from "./purchase-delivery-plan"
import { createPurchaseDeliveryWorkbookBlob } from "./purchase-delivery-workbook"
import {
    downloadPurchasePaperFile,
    purchasePaperFileNamePart,
    renderPurchasePaperImage,
} from "./purchase-paper-image"

export type PurchasePaperImageDownloadResult = Readonly<{
    bundledWorkbook: boolean
    deliveryRowCount: number
}>

function extractServerDeliveryLines(paperDocument: Document): {
    warehouse: boolean
    lines: PurchaseDeliveryPlanLine[]
} {
    const section = paperDocument.querySelector<HTMLElement>(
        "[data-purchase-delivery='true']",
    )
    if (!section) return { warehouse: false, lines: [] }
    if (section.dataset.purchaseDeliveryMode === "warehouse") {
        return { warehouse: true, lines: [] }
    }

    const groups = section.querySelectorAll<HTMLElement>(
        "[data-purchase-delivery-group='true']",
    )
    const lines: PurchaseDeliveryPlanLine[] = []
    groups.forEach((group, index) => {
        const rows = group.querySelectorAll<HTMLElement>(
            "[data-purchase-delivery-row='true']",
        )
        const deliveryAllocations = Array.from(rows, (row) => ({
            recipient: row.dataset.recipient ?? "",
            phone: row.dataset.phone ?? "",
            region_code: "",
            region_name: row.dataset.regionName ?? "",
            address: row.dataset.address ?? "",
            quantity: row.dataset.quantity ?? "0",
        }))
        lines.push({
            lineId: `server-group-${index}`,
            itemName: group.dataset.itemName ?? "—",
            itemSku: group.dataset.itemSku || undefined,
            unit: group.dataset.unit || undefined,
            quantity: group.dataset.quantity || undefined,
            expectedDeliveryDate: group.dataset.expectedDate || undefined,
            deliveryAllocations,
        })
    })
    return { warehouse: false, lines }
}

function replaceDeliveryWithSummary(
    paperDocument: Document,
    lines: readonly PurchaseDeliveryPlanLine[],
    workbookFilename: string,
): void {
    const section = paperDocument.querySelector<HTMLElement>(
        "[data-purchase-delivery='true']",
    )
    if (!section) return
    const summary = buildPurchaseDeliveryPlanSummary(lines)

    const heading = paperDocument.createElement("h2")
    heading.textContent = "交付安排（摘要）"

    const table = paperDocument.createElement("table")
    const thead = paperDocument.createElement("thead")
    const headRow = paperDocument.createElement("tr")
    const headers: ReadonlyArray<{ label: string; numeric?: boolean }> = [
        { label: "商品 / 规格" },
        { label: "地址数", numeric: true },
        { label: "分配数量", numeric: true },
        { label: "收货地区 / 示例" },
    ]
    for (const column of headers) {
        const th = paperDocument.createElement("th")
        th.textContent = column.label
        if (column.numeric) th.className = "num"
        headRow.appendChild(th)
    }
    thead.appendChild(headRow)
    table.appendChild(thead)

    const tbody = paperDocument.createElement("tbody")
    for (const group of summary.itemGroups) {
        const tr = paperDocument.createElement("tr")

        const itemCell = paperDocument.createElement("td")
        const title = paperDocument.createElement("div")
        title.textContent = group.itemName
        itemCell.appendChild(title)
        if (group.itemSku) {
            const sku = paperDocument.createElement("div")
            sku.className = "muted"
            sku.textContent = group.itemSku
            itemCell.appendChild(sku)
        }
        tr.appendChild(itemCell)

        const countCell = paperDocument.createElement("td")
        countCell.className = "num"
        countCell.textContent =
            group.allocations.length > 0
                ? `${group.allocations.length} 处`
                : "未指定"
        tr.appendChild(countCell)

        const qtyCell = paperDocument.createElement("td")
        qtyCell.className = "num"
        qtyCell.textContent =
            group.allocations.length > 0
                ? `${group.allocatedTotal}${group.unit ? ` ${group.unit}` : ""}`
                : "—"
        tr.appendChild(qtyCell)

        const regionCell = paperDocument.createElement("td")
        regionCell.className = "muted"
        regionCell.textContent = group.regionSummary
        tr.appendChild(regionCell)

        tbody.appendChild(tr)
    }
    table.appendChild(tbody)

    const note = paperDocument.createElement("div")
    note.className = "delivery-note"
    note.textContent = `本采购单共含 ${summary.totalRows} 条收件分配（${summary.uniqueAddressCount} 处独立地址）。为保证单据图片清晰可读，完整收货人、联系电话、详细地址及数量请查阅同包配套表格：《${workbookFilename}》。`

    section.replaceChildren(heading, table, note)
}

/**
 * 使用独立文档保留纸质采购单样式，导出供应商命名的采购单图片。
 * 当直发地址过多时自动切换为「清晰主单 PNG + 配套发货地址 Excel 表」并打包下载。
 */
export async function downloadPurchasePaperImage(
    html: string,
    supplierName: string,
): Promise<PurchasePaperImageDownloadResult> {
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
        // 去掉页面外边距，保留纸张内部留白，按完整内容高度导出。
        paper.style.margin = "0"
        paper.style.boxSizing = "border-box"
        paper.style.maxWidth = "none"
        // 独立打印文档使用白纸背景，不受应用深浅主题影响。
        paper.style.backgroundColor = "white"

        const extracted = extractServerDeliveryLines(paperDocument)
        const deliveryRowCount = extracted.warehouse
            ? 0
            : extracted.lines.reduce(
                  (total, line) =>
                      total + (line.deliveryAllocations?.length ?? 0),
                  0,
              )

        const safeSupplier = purchasePaperFileNamePart(supplierName)
        if (deliveryRowCount > IMAGE_INLINE_ADDRESS_LIMIT) {
            const workbookFilename =
                purchaseDeliveryWorkbookFileName(supplierName)
            const purchaseNo = paper.dataset.purchaseNo?.trim() || undefined
            const workbookBlob = await createPurchaseDeliveryWorkbookBlob({
                documentLabel: purchaseNo,
                supplierName,
                lines: extracted.lines,
            })
            replaceDeliveryWithSummary(
                paperDocument,
                extracted.lines,
                workbookFilename,
            )
            const imageBlob = await renderPurchasePaperImage(paper)
            const zip = new JSZip()
            zip.file(`${safeSupplier}-采购单.png`, imageBlob)
            zip.file(workbookFilename, workbookBlob)
            const archive = await zip.generateAsync({ type: "blob" })
            downloadPurchasePaperFile(
                archive,
                `${safeSupplier}-采购单及发货地址.zip`,
            )
            return { bundledWorkbook: true, deliveryRowCount }
        }

        const blob = await renderPurchasePaperImage(paper)
        downloadPurchasePaperFile(blob, `${safeSupplier}.png`)
        return { bundledWorkbook: false, deliveryRowCount }
    } finally {
        frame.remove()
    }
}
