import type { ContractAttachmentView } from "@/features/contracts/types"

export const MAX_CONTRACT_PDF_BYTES = 20 * 1024 * 1024

export function contractPdfError(file: File | null): string | null {
  if (!file) return "请上传合同 PDF"
  const hasPdfName = file.name.toLowerCase().endsWith(".pdf")
  const hasPdfType = file.type === "application/pdf" || file.type === ""
  if (!hasPdfName || !hasPdfType) return "合同只支持 PDF 文件"
  if (file.size <= 0) return "合同 PDF 不能为空文件"
  if (file.size > MAX_CONTRACT_PDF_BYTES) return "合同 PDF 不能超过 20 MB"
  return null
}

/** 选取合同中心中可作为「原始签署 PDF」下载的附件（优先高版本、可下载）。 */
export function pickOriginalContractPdfAttachment(
  attachments: readonly ContractAttachmentView[]
): ContractAttachmentView | null {
  const ready = attachments.filter(
    (file) =>
      file.canDownload &&
      file.securityState === "done" &&
      (file.contentType === "application/pdf" ||
        file.name.toLowerCase().endsWith(".pdf"))
  )
  if (ready.length === 0) return null
  return [...ready].sort(
    (a, b) => (b.revisionNo ?? 0) - (a.revisionNo ?? 0)
  )[0]
}

export type OriginalContractPdfTarget = {
  contractId: string | null
  contractNo: string
  fileName: string
  attachment: ContractAttachmentView | null
}

/**
 * 按合同号解析原始 PDF 下载目标。
 * 真实文件流应由鉴权下载 URL 提供；此处仅返回文件名元数据，避免依赖 mock 会话。
 */
export function resolveOriginalContractPdf(
  contractNo: string
): OriginalContractPdfTarget {
  const normalized = contractNo.trim()
  return {
    contractId: null,
    contractNo: normalized,
    fileName: `${normalized || "contract"}.pdf`,
    attachment: null,
  }
}

/**
 * 演示用最小合法 PDF（空白页）。
 * 生产环境应改为短时签名 URL / 鉴权流式下载真实归档对象。
 */
export function buildDemoContractPdfBlob(contractNo: string): Blob {
  // 固定偏移的最小 PDF-1.4；内容为单页空白纸，文件名由调用方决定。
  const body = [
    "%PDF-1.4",
    "1 0 obj<< /Type /Catalog /Pages 2 0 R >>endobj",
    "2 0 obj<< /Type /Pages /Kids [3 0 R] /Count 1 >>endobj",
    "3 0 obj<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>endobj",
    "xref",
    "0 4",
    "0000000000 65535 f ",
    "0000000009 00000 n ",
    "0000000058 00000 n ",
    "0000000115 00000 n ",
    "trailer<< /Size 4 /Root 1 0 R >>",
    "startxref",
    "190",
    `%%EOF`,
    `% ${contractNo}`,
  ].join("\n")
  return new Blob([body], { type: "application/pdf" })
}

/** 触发浏览器下载合同原始 PDF（演示 blob；有附件元数据时用归档文件名）。 */
export function downloadOriginalContractPdf(contractNo: string): OriginalContractPdfTarget {
  const target = resolveOriginalContractPdf(contractNo)
  if (target.attachment && !target.attachment.canDownload) {
    const err = new Error("原始合同 PDF 当前不可下载（安全检查未通过或处理中）")
    throw err
  }

  const blob = buildDemoContractPdfBlob(target.contractNo)
  const url = URL.createObjectURL(blob)
  const anchor = document.createElement("a")
  anchor.href = url
  anchor.download = target.fileName
  anchor.rel = "noopener"
  document.body.appendChild(anchor)
  anchor.click()
  anchor.remove()
  URL.revokeObjectURL(url)
  return target
}
