/**
 * Excel（.xlsx）读取与下载的统一入口。
 *
 * 各导入模板、失败清单和导出文件只拼接工作簿内容；文件格式、Blob 类型和
 * 下载触发集中在此，避免每个功能各写一份 `new Workbook()` 与 `<a download>`。
 */

import type { Workbook } from "exceljs"

import { downloadBlob } from "./download-blob"

const XLSX_MIME =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"

export async function createWorkbook(): Promise<Workbook> {
    const excel = await import("exceljs")
    return new (excel.default ?? excel).Workbook()
}

/** 读取浏览器选择的 .xlsx 文件。 */
export async function loadWorkbookFromFile(file: File): Promise<Workbook> {
    const workbook = await createWorkbook()
    await workbook.xlsx.load(await file.arrayBuffer())
    return workbook
}

/** 把工作簿序列化为可下载的 Excel Blob。 */
export async function workbookBlob(workbook: Workbook): Promise<Blob> {
    const buffer = await workbook.xlsx.writeBuffer()
    return new Blob([new Uint8Array(buffer)], { type: XLSX_MIME })
}

/** 直接下载工作簿；`linkId` 供自动化定位下载入口。 */
export async function downloadWorkbook(
    workbook: Workbook,
    fileName: string,
    options?: { linkId?: string; revokeDelayMs?: number },
): Promise<void> {
    downloadBlob(await workbookBlob(workbook), fileName, options)
}
