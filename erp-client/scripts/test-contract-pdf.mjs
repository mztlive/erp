/**
 * Exercises the PDF-only contract archive and sales-order upload path via tsx.
 * Run: node scripts/test-contract-pdf.mjs
 */
import { spawnSync } from "node:child_process"
import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..")

const runner = `
import { contractPdfError, MAX_CONTRACT_PDF_BYTES } from "../features/contracts/pdf.ts"
import { createSalesOrder } from "../features/sales-orders/api.ts"
import { getW04ContractCenter, listW04Contracts, uploadW04ContractPdf } from "../mock/session-state.ts"

let failed = 0
function assert(cond: boolean, msg: string) {
  if (!cond) {
    console.error("FAIL:", msg)
    failed += 1
  } else {
    console.log("OK:", msg)
  }
}

function file(name: string, type: string, size: number): File {
  return { name, type, size } as File
}

assert(contractPdfError(null) === "请上传合同 PDF", "PDF is required")
assert(
  contractPdfError(file("contract.jpg", "image/jpeg", 100)) === "合同只支持 PDF 文件",
  "non-PDF is rejected"
)
assert(
  contractPdfError(file("contract.pdf", "application/pdf", MAX_CONTRACT_PDF_BYTES + 1)) ===
    "合同 PDF 不能超过 20 MB",
  "PDF size limit is enforced"
)
assert(
  contractPdfError(file("contract.pdf", "application/pdf", 1024)) === null,
  "valid PDF is accepted"
)

const archiveNo = "HT-TEST-PDF-001"
const archiveInput = {
  pdfFile: file("HT-TEST-PDF-001.pdf", "application/pdf", 2048),
  contractNo: archiveNo,
  customerName: "PDF 测试客户",
  settlementPartyName: "PDF 测试客户",
  signedAt: "2026-08-02",
  validFrom: "2026-08-02",
  validTo: "2027-08-01",
  paymentTerms: "月结 30 天",
  idempotencyKey: "test-contract-pdf-archive",
}
const archived = uploadW04ContractPdf(archiveInput)
const replayed = uploadW04ContractPdf(archiveInput)
const center = getW04ContractCenter(archived.contractId)
assert(archived.contractId === replayed.contractId, "contract upload is idempotent")
assert(center?.selectableForNewSalesOrder === true, "uploaded contract is selectable")
assert(
  center?.attachments.length === 1 &&
    center.attachments[0]?.contentType === "application/pdf",
  "uploaded version owns one PDF"
)

const invalidContractNo = "HT-TEST-ATOMIC-INVALID"
let invalidRejected = false
try {
  await createSalesOrder({
    contract: {
      source: "upload_pdf",
      pdfFile: file("invalid.pdf", "application/pdf", 1024),
      contractNo: invalidContractNo,
      customerName: "原子性测试客户",
      settlementPartyName: "原子性测试客户",
      signedAt: "2026-08-02",
      validFrom: "2026-08-02",
      validTo: "2027-08-01",
    },
    nature: "physical_service",
    ownerName: "测试销售",
    welfareScene: "测试",
    paymentTerms: "月结 30 天",
    fulfillmentDeadline: "2026-12-31",
    taxRatePercent: "13.00",
    remark: "",
    lineItems: [],
    intent: "SAVE_DRAFT",
    idempotencyKey: "test-invalid-sales-order",
  })
} catch (error) {
  invalidRejected = error instanceof Error && error.message === "LINE_ITEM_REQUIRED"
}
assert(invalidRejected, "invalid sales order is rejected")
assert(
  !listW04Contracts().some((contract) => contract.contractNo === invalidContractNo),
  "failed sales order leaves no contract archive"
)

const salesContractNo = "HT-TEST-SALES-PDF-001"
const created = await createSalesOrder({
  contract: {
    source: "upload_pdf",
    pdfFile: file("HT-TEST-SALES-PDF-001.pdf", "application/pdf", 4096),
    contractNo: salesContractNo,
    customerName: "随单上传测试客户",
    settlementPartyName: "随单上传测试客户",
    signedAt: "2026-08-02",
    validFrom: "2026-08-02",
    validTo: "2027-08-01",
  },
  nature: "physical_service",
  ownerName: "测试销售",
  welfareScene: "年节礼包",
  paymentTerms: "月结 30 天",
  fulfillmentDeadline: "2026-12-31",
  taxRatePercent: "13.00",
  remark: "随单上传合同 PDF",
  lineItems: [
    {
      rowKey: "line-1",
      name: "测试礼盒",
      sku: "SKU-TEST-1",
      quantity: "1",
      unit: "件",
      unitPriceGross: "100.00",
      fulfillmentMode: "公司仓发",
      dueDate: "2026-12-01",
      faceValue: "",
      giftRate: "",
      cardForm: "",
    },
  ],
  intent: "SAVE_DRAFT",
  idempotencyKey: "test-sales-order-with-contract-pdf",
})
const uploadedForSales = listW04Contracts().find(
  (contract) => contract.contractNo === salesContractNo
)
assert(Boolean(created.salesOrderId), "sales order is created")
assert(Boolean(uploadedForSales), "sales-order upload creates contract archive")
assert(
  uploadedForSales?.salesOrderCount === 1,
  "sales order is linked back to the uploaded contract"
)

if (failed) process.exit(1)
console.log("All contract PDF archive checks passed")
`

const tmp = path.join(root, "scripts", ".run-contract-pdf-test.mts")
fs.writeFileSync(tmp, runner)

const result = spawnSync(
  "npx",
  ["--yes", "tsx", path.join("scripts", ".run-contract-pdf-test.mts")],
  { cwd: root, encoding: "utf8", env: process.env }
)

try {
  fs.unlinkSync(tmp)
} catch {
  // ignore
}

console.log(result.stdout || result.stderr)
process.exit(result.status ?? 1)
