import { execFileSync } from "node:child_process"
import { fileURLToPath } from "node:url"
import { expect, type Page } from "./test"
import { API_BASE } from "./api"

type Pdf = string | { name: string; mimeType: string; buffer: Buffer }
export const SIGNING_COMPANY = "广东福尚云科技有限公司"

/** Exercise real upload, identity selection, three-step review and archive; stub OCR/AI only. */
export async function archiveContractViaUi(page: Page, input: {
    contractNo: string
    customerName: string
    pdf: Pdf
    paymentTerms?: string
    validTo?: string
}) {
    const dialog = page.getByRole("dialog", { name: /^导入合同(?:新版本)?$/ })
    await expect(dialog).toBeVisible()
    const today = new Date().toLocaleDateString("en-CA", { timeZone: "Asia/Shanghai" })
    const fields = {
        customer_name: input.customerName,
        company_name: SIGNING_COMPANY,
        company_credit_code: "91440106MAC9AC16X9",
        payment_terms: input.paymentTerms ?? "货到 15 天",
        invoice_type: "增值税专用发票", tax_point: "13",
        signed_at: today, valid_from: today, valid_to: input.validTo ?? "长期",
        business_scope: "E2E 员工福利业务回归夹具",
    }
    const pattern = /\/admin\/contract-imports\/[^/]+\/run$/
    await page.route(pattern, async (route) => {
        const url = new URL(route.request().url())
        const id = url.pathname.split("/").at(-2)!
        execFileSync(process.execPath, [fileURLToPath(new URL("../fixtures/contract-recognition.mjs", import.meta.url))], {
            input: JSON.stringify({ id, fields }), stdio: ["pipe", "pipe", "pipe"], timeout: 35_000,
        })
        // The real run endpoint observes REVIEW and does not call external vendors.
        await route.continue({ url: `${API_BASE}${url.pathname}` })
    }, { times: 1 })
    await dialog.locator("#card-contracts-upload-pdf-input").setInputFiles(input.pdf)
    const customer = dialog.locator("#contract-import-customer")
    await expect(customer).toBeVisible({ timeout: 60_000 })
    await expect(customer).toHaveValue(new RegExp(input.customerName))
    await expect(dialog.locator("#contract-import-company")).toHaveValue(new RegExp(SIGNING_COMPANY))
    await dialog.locator("#contract-import-next").click()
    await dialog.locator("#contract-import-edit-contract-no").fill(input.contractNo)
    await dialog.locator("#contract-import-next").click()
    const confirmed = page.waitForResponse(response => response.request().method() === "POST" && /\/contract-imports\/[^/]+\/confirm$/.test(new URL(response.url()).pathname))
    await dialog.locator("#contract-import-confirm").click()
    const response = await confirmed
    expect(response.ok(), await response.text()).toBeTruthy()
    const body = await response.json()
    expect(body.data.status).toBe("succeeded")
    if (new URL(page.url()).pathname === "/sales/contracts") {
        await dialog.locator("#contract-import-use").click()
    }
    await expect(dialog).toBeHidden()
    return body.data.result as { id: string; contract_no: string; revision_id: string }
}
