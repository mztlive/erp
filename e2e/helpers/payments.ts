import { execFileSync } from "node:child_process"
import { randomUUID } from "node:crypto"
import { readFileSync } from "node:fs"
import { fileURLToPath } from "node:url"

import { expect, type Page } from "@playwright/test"

import { API_BASE, apiGet, apiToken } from "./api"

export const BANK_RECEIPT_FILE = {
    name: "e2e-bank-receipt.png",
    mimeType: "image/png",
    buffer: Buffer.from(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j5xkAAAAASUVORK5CYII=",
        "base64",
    ),
}

type SupplierPaymentResult = { id: string; payment_no: string; status: string }

/**
 * 付款任务要求供应商主体有唯一当前默认收款账户。
 * 供应商主体不能走通用 Party 银行账户接口，种子里也没有这行，这里按主体补一条。
 */
async function ensureDefaultSupplierBankAccount(
    keyword: string,
): Promise<void> {
    const token = await apiToken("admin")
    const suppliers = await apiGet<{
        items?: Array<{ party_id?: string; legal_name?: string }>
    }>(token, "/admin/suppliers", { keyword, page: 1, page_size: 5 })
    const supplier = suppliers.items?.find((row) => row.party_id)
    if (!supplier?.party_id) throw new Error(`未找到供应商收款主体：${keyword}`)
    const config =
        process.env.ERP_E2E_CONFIG_PATH ??
        fileURLToPath(new URL("../../backend/config.toml", import.meta.url))
    const settings = JSON.parse(
        execFileSync(
            "python3",
            [
                "-c",
                "import json,sys,tomllib; print(json.dumps(tomllib.load(open(sys.argv[1], 'rb'))['database']))",
                config,
            ],
            { encoding: "utf8", timeout: 10_000 },
        ),
    ) as { uri: string; db_name: string }
    const now = Math.floor(Date.now() / 1000)
    const accountId = randomUUID().replaceAll("-", "")
    const script = `const target = db.getSiblingDB(${JSON.stringify(settings.db_name)});
        const partyId = ${JSON.stringify(supplier.party_id)};
        const existing = target.party_bank_accounts.findOne({
          party_id: partyId, is_default: true, status: "active", deleted_at: NumberLong(0)
        });
        if (!existing) {
          target.party_bank_accounts.insertOne({
            id: ${JSON.stringify(accountId)},
            version: NumberLong(1),
            created_at: NumberLong(${now}),
            updated_at: NumberLong(${now}),
            deleted_at: NumberLong(0),
            created_by: "e2e",
            updated_by: "e2e",
            bank_account_no: ${JSON.stringify(`E2E-${supplier.party_id.slice(0, 8)}`)},
            party_id: partyId,
            account_name: ${JSON.stringify(supplier.legal_name || keyword)},
            bank_name: "中国工商银行",
            bank_branch_name: null,
            account_number_ciphertext: "e2e",
            account_number_query_hmac: ${JSON.stringify(`e2e-${supplier.party_id}`)},
            account_number_last4: "0001",
            valid_from: "2020-01-01",
            valid_to: null,
            status: "active",
            is_default: true
          });
        }`
    execFileSync(
        "mongosh",
        ["--norc", "--quiet", settings.uri, "--eval", script],
        {
            stdio: "pipe",
            timeout: 30_000,
        },
    )
}

/** 出纳从唯一待付款任务全额付款，等待正式提交落定后再离开。 */
export async function payOnlySupplierTask(
    page: Page,
    documentHint?: string,
): Promise<SupplierPaymentResult> {
    await ensureDefaultSupplierBankAccount("狮峰")
    await page.goto("/workspace?family=finance")
    const tasks = page
        .getByRole("list", { name: "待办列表" })
        .getByRole("button", { name: /供应商付款处理/ })
    const task = documentHint
        ? tasks.filter({ hasText: documentHint }).first()
        : tasks.first()
    await expect(task).toBeVisible({ timeout: 20_000 })
    await task.click()
    const amount = page.locator("#supplier-payables-allocation-form-amount")
    await expect(amount).toHaveValue(/^[1-9]\d*(?:\.\d+)?$|^0\.0*[1-9]\d*$/, {
        timeout: 20_000,
    })
    await page
        .locator("#supplier-payables-allocation-form-bank-receipt-input")
        .setInputFiles(BANK_RECEIPT_FILE)
    await page.locator("#supplier-payables-allocation-form-submit").click()
    const dialog = page.getByRole("alertdialog").filter({ hasText: "确认付款" })
    await expect(dialog).toBeVisible()
    await expect(dialog.getByText("提交审批")).toHaveCount(0)
    const committed = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            response.url().includes("/admin/supplier-payments/commit"),
        { timeout: 60_000 },
    )
    await dialog
        .locator("#supplier-payables-payment-submit-confirm-confirm")
        .click()
    const response = await committed
    expect(response.ok(), await response.text()).toBe(true)
    const result = (await response.json()).data as SupplierPaymentResult
    expect(result).toMatchObject({ status: "posted" })
    expect(result.id).toBeTruthy()
    expect(result.payment_no).toBeTruthy()
    await expect(dialog).toBeHidden({ timeout: 20_000 })
    return result
}

/** 当前采购履约任务只提供其来源采购已付款回单，并可下载真实银行凭证。 */
export async function expectProcurementPaymentReceipt(
    page: Page,
    payment: SupplierPaymentResult,
): Promise<void> {
    await expect(
        page.getByRole("heading", { name: "财务付款回单", exact: true }),
    ).toBeVisible({
        timeout: 20_000,
    })
    await expect
        .poll(() => new URL(page.url()).searchParams.get("currentWorkItemId"), {
            timeout: 20_000,
        })
        .toBeTruthy()
    const workItemId = new URL(page.url()).searchParams.get(
        "currentWorkItemId",
    )!
    const token = await apiToken("caigou")
    const headers = { Authorization: `Bearer ${token}` }
    const listPath = `/admin/work-items/${workItemId}/payment-receipts`
    const listed = await page.request.get(`${API_BASE}${listPath}`, { headers })
    expect(listed.ok(), await listed.text()).toBeTruthy()
    const files = (await listed.json()).data as Array<{
        document_id: string
        document_no: string
        file_name: string
        content_type: string
        byte_size: number
    }>
    expect(files).toHaveLength(1)
    expect(files[0]).toMatchObject({
        document_id: payment.id,
        document_no: payment.payment_no,
        file_name: BANK_RECEIPT_FILE.name,
        content_type: BANK_RECEIPT_FILE.mimeType,
        byte_size: BANK_RECEIPT_FILE.buffer.length,
    })
    const row = page
        .getByRole("listitem")
        .filter({ hasText: BANK_RECEIPT_FILE.name })
    await expect(row).toContainText(payment.payment_no, { timeout: 20_000 })
    const downloadPath = `${listPath}/${payment.id}/download`
    const [download, response] = await Promise.all([
        page.waitForEvent("download"),
        page.waitForResponse(
            (result) => new URL(result.url()).pathname === downloadPath,
        ),
        row.getByRole("button", { name: "下载", exact: true }).click(),
    ])
    expect(download.suggestedFilename()).toBe(BANK_RECEIPT_FILE.name)
    expect(await download.failure()).toBeNull()
    expect(response.ok()).toBeTruthy()
    expect(response.headers()["content-type"]).toBe(BANK_RECEIPT_FILE.mimeType)
    expect(response.headers()["cache-control"]).toBe("private, no-store")
    const downloadedPath = await download.path()
    expect(downloadedPath, "浏览器下载应生成真实回单文件").not.toBeNull()
    expect(readFileSync(downloadedPath!)).toEqual(BANK_RECEIPT_FILE.buffer)
    const unrelated = await page.request.get(
        `${API_BASE}${listPath}/payment-not-linked-to-task/download`,
        { headers },
    )
    expect(unrelated.status(), await unrelated.text()).toBe(404)
}
