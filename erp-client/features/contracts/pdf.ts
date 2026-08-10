const MAX_CONTRACT_PDF_BYTES = 20 * 1024 * 1024

export function contractPdfError(file: File | null): string | null {
  if (!file) return "请上传合同 PDF"
  const hasPdfName = file.name.toLowerCase().endsWith(".pdf")
  const hasPdfType = file.type === "application/pdf" || file.type === ""
  if (!hasPdfName || !hasPdfType) return "合同只支持 PDF 文件"
  if (file.size <= 0) return "合同 PDF 不能为空文件"
  if (file.size > MAX_CONTRACT_PDF_BYTES) return "合同 PDF 不能超过 20 MB"
  return null
}
