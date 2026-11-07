import { downloadBlob } from "@/lib/download-blob"

/** 文件内容完成后立即下载；不创建可在撤权后复用的后台下载入口。 */
export const downloadListCsv = (content: string, filename: string): void => {
    downloadBlob(
        new Blob([content], { type: "text/csv;charset=utf-8" }),
        filename,
    )
}
