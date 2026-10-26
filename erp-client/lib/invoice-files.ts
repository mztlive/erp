/** 财务开票附件的选择、校验与单次请求引用。 */

export const INVOICE_FILE_ACCEPT =
    "application/pdf,image/jpeg,image/png,image/webp,.pdf,.jpg,.jpeg,.png,.webp"
export const INVOICE_FILE_MAX_COUNT = 32
const INVOICE_FILE_MAX_BYTES = 5 * 1024 * 1024
const INVOICE_FILE_MIME_BY_EXTENSION: Readonly<Record<string, string>> = {
    pdf: "application/pdf",
    jpg: "image/jpeg",
    jpeg: "image/jpeg",
    png: "image/png",
    webp: "image/webp",
}

/** 同一文件选择后沿用稳定引用，重复选择只保留一份。 */
export function invoiceFileReference(file: File): string {
    return `pending-file:invoice:${encodeURIComponent(file.name)}:${file.size}`
}

/** 校验开票附件数量、单文件大小和扩展名与 MIME 类型。 */
export function invoiceFilesError(files: readonly File[]): string | undefined {
    if (files.length > INVOICE_FILE_MAX_COUNT) {
        return `发票附件最多上传 ${INVOICE_FILE_MAX_COUNT} 个文件`
    }
    for (const file of files) {
        if (file.size === 0) return `${file.name}：文件不能为空`
        if (file.size > INVOICE_FILE_MAX_BYTES) {
            return `${file.name}：单个文件不能超过 5 MB`
        }
        const extension = file.name.split(".").at(-1)?.toLowerCase() ?? ""
        if (
            !INVOICE_FILE_MIME_BY_EXTENSION[extension] ||
            file.type !== INVOICE_FILE_MIME_BY_EXTENSION[extension]
        ) {
            return `${file.name}：仅支持 PDF、JPG、PNG 或 WebP，文件类型须与扩展名一致`
        }
    }
    return undefined
}
