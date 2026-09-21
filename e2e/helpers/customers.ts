import { expect, type Locator, type Page } from "@playwright/test"

import {
    chooseOption,
    dismissToasts,
    expectComboboxValue,
    UI_TIMEOUT,
} from "./ui"

/**
 * 客户中心 UI。spec 不要再复制建客填表：
 *   import { createCustomerViaUi } from "../helpers/customers"
 *   await createCustomerViaUi(page, {
 *     legalName, shortName, creditCode, paymentTermLabel: "货到 15 天",
 *     contact: { name: "李测", phone: "13800138001" },
 *     address: "北京市朝阳区测试路 1 号",
 *   })
 *
 * 生产 id / 文案：
 * - `/sales/customers` heading「客户中心」
 * - `#customers-directory-create`「新建客户」
 * - 对话框「新建客户」：`#customers-form-legal-name` / `#customers-form-short-name` /
 *   `#customers-form-credit-code` / `#customers-form-payment-term`
 *   （选项 `#customers-form-payment-term-option-postpay-net15` = 货到 15 天）
 * - `#customers-form-contacts-add` / `#customers-form-addresses-add`
 * - `#customers-form-submit`「创建客户」；成功 toast「客户已创建」并关对话框
 */

const PAYMENT_TERM_VALUE: Record<string, string> = {
    "先款 100%": "PREPAY_100",
    "先款 50%": "PREPAY_50",
    "先款 30%": "PREPAY_30",
    "货到 15 天": "POSTPAY_NET15",
    "货到 30 天": "POSTPAY_NET30",
    按合同约定: "CONTRACT",
}

export type CreateCustomerContactInput = {
    name: string
    phone: string
    title?: string
}

export type CreateCustomerAddressInput = {
    address: string
    type?: string
    contactName?: string
}

export type CreateCustomerViaUiInput = {
    legalName: string
    shortName?: string
    creditCode: string
    paymentTermLabel: string
    contact?: CreateCustomerContactInput
    address?: string | CreateCustomerAddressInput
}

function toOptionSegment(value: string): string {
    return (
        value
            .normalize("NFKD")
            .toLowerCase()
            .replace(/[\u0300-\u036f]/g, "")
            .replace(/[^a-z0-9]+/g, "-")
            .replace(/^-+|-+$/g, "") || "item"
    )
}

async function expectPaymentTermWritten(
    page: Page,
    dialog: Locator,
    label: string,
): Promise<void> {
    const input = dialog.locator("#customers-form-payment-term")
    await expectComboboxValue(page, input, label)
    const code = PAYMENT_TERM_VALUE[label]
    if (!code) return

    await input.click()
    const option = page.locator(
        `#customers-form-payment-term-option-${toOptionSegment(code)}`,
    )
    if (!(await option.isVisible().catch(() => false))) {
        await page.locator("#customers-form-payment-term-trigger").click()
    }
    await expect(option).toBeVisible({ timeout: 5_000 })
    await option.click()
    await expectComboboxValue(page, input, label)
}

async function fillOptionalRows(
    page: Page,
    dialog: Locator,
    input: CreateCustomerViaUiInput,
): Promise<void> {
    if (input.contact) {
        const add = dialog.locator("#customers-form-contacts-add")
        if (!(await add.count())) {
            throw new Error("没有添加联系人按钮（可能无 party_contact:create 权限）")
        }
        await add.click()
        const name = dialog.locator(
            'input[id^="customers-form-contacts-"][id$="-name"]',
        )
        const phone = dialog.locator(
            'input[id^="customers-form-contacts-"][id$="-phone"]',
        )
        await expect(name).toBeVisible({ timeout: UI_TIMEOUT })
        await name.fill(input.contact.name)
        await phone.fill(input.contact.phone)
        if (input.contact.title) {
            await dialog
                .locator('input[id^="customers-form-contacts-"][id$="-title"]')
                .fill(input.contact.title)
        }
    }

    if (input.address) {
        const add = dialog.locator("#customers-form-addresses-add")
        if (!(await add.count())) {
            throw new Error("没有添加地址按钮（可能无 party_address:create 权限）")
        }
        await add.click()
        const row =
            typeof input.address === "string"
                ? { address: input.address }
                : input.address
        if (row.type) {
            await chooseOption(
                page,
                dialog.locator(
                    '[id^="customers-form-addresses-"][id$="-type"]',
                ),
                row.type,
            )
        }
        const address = dialog.locator(
            'input[id^="customers-form-addresses-"][id$="-address"]',
        )
        await expect(address).toBeVisible({ timeout: UI_TIMEOUT })
        await address.fill(row.address)
        if (row.contactName) {
            await dialog
                .locator(
                    'input[id^="customers-form-addresses-"][id$="-contact-name"]',
                )
                .fill(row.contactName)
        }
    }
}

async function readDialogDiagnostics(
    page: Page,
    dialog: Locator,
    network?: string,
): Promise<string> {
    const parts: string[] = []
    if (network) parts.push(network)

    const fieldErrors = (
        await dialog.locator('[data-slot="field-error"]').allTextContents()
    )
        .map((text) => text.trim())
        .filter(Boolean)
    if (fieldErrors.length) parts.push(`校验: ${fieldErrors.join("；")}`)

    const alerts = (await dialog.getByRole("alert").allTextContents())
        .map((text) => text.trim())
        .filter(Boolean)
    if (alerts.length) parts.push(`提示: ${alerts.join("；")}`)

    const submit = dialog.locator("#customers-form-submit")
    if (await submit.count()) {
        const label = (await submit.innerText()).trim()
        const disabled = await submit.isDisabled().catch(() => false)
        parts.push(`提交按钮: ${label}${disabled ? "（disabled）" : ""}`)
    } else {
        parts.push("提交按钮不存在")
    }

    if (await dialog.locator("#customers-form-complete").count()) {
        parts.push("已出现完成按钮")
    }
    const rejected = dialog.locator("#customers-form-create-rejected")
    if (await rejected.isVisible().catch(() => false)) {
        parts.push(`拒绝: ${(await rejected.innerText()).trim()}`)
    }

    const errorToast = page.locator('[data-slot="toast"]').filter({
        hasText: /失败|无法|错误|未完成|频繁/,
    })
    if (await errorToast.first().isVisible().catch(() => false)) {
        parts.push(`toast: ${(await errorToast.first().innerText()).trim()}`)
    }

    const payment = await dialog
        .locator("#customers-form-payment-term")
        .inputValue()
        .catch(() => "")
    parts.push(`付款条件输入值: ${JSON.stringify(payment)}`)
    return parts.join(" | ") || "对话框仍打开且无可见错误"
}

function createSucceeded(page: Page, dialog: Locator): Locator {
    const toast = page.locator('[data-slot="toast"]').filter({ hasText: "客户已创建" })
    const titled = page.locator('[data-slot="toast-title"]').filter({
        hasText: "客户已创建",
    })
    const text = page.getByText("客户已创建")
    const complete = dialog.locator("#customers-form-complete")
    return titled.or(toast).or(text).or(complete)
}

/**
 * 打开客户中心，新建客户并等待成功（toast、文案或对话框关闭）。
 * 提交前确认付款条件 combobox 已写入选项，而不是只显示键入文本。
 * 失败时立刻抛出含校验文案 / 按钮状态 / 网络结果的错误，不等 20s。
 */
export async function createCustomerViaUi(
    page: Page,
    input: CreateCustomerViaUiInput,
): Promise<void> {
    await page.goto("/sales/customers")
    await expect(page.getByRole("heading", { name: "客户中心" })).toBeVisible({
        timeout: UI_TIMEOUT,
    })
    await page.locator("#customers-directory-create").click()
    const dialog = page.getByRole("dialog", { name: "新建客户" })
    await expect(dialog).toBeVisible({ timeout: UI_TIMEOUT })

    await dialog.locator("#customers-form-legal-name").fill(input.legalName)
    if (input.shortName) {
        await dialog.locator("#customers-form-short-name").fill(input.shortName)
    }
    await dialog.locator("#customers-form-credit-code").fill(input.creditCode)
    await chooseOption(
        page,
        dialog.locator("#customers-form-payment-term"),
        input.paymentTermLabel,
    )
    await expectPaymentTermWritten(page, dialog, input.paymentTermLabel)
    await fillOptionalRows(page, dialog, input)

    const submit = dialog.locator("#customers-form-submit")
    await expect(submit).toBeVisible({ timeout: UI_TIMEOUT })
    if (await submit.isDisabled()) {
        throw new Error(
            `创建客户无法提交: ${await readDialogDiagnostics(page, dialog)}`,
        )
    }

    let network: string | undefined
    void page
        .waitForResponse(
            (response) =>
                response.request().method() === "POST" &&
                response.url().includes("/admin/customer-profiles"),
            { timeout: UI_TIMEOUT },
        )
        .then(async (response) => {
            if (!response.ok()) {
                network = `HTTP ${response.status()} ${await response.text()}`
                return
            }
            network = "ok"
        })
        .catch(() => {
            if (!network) network = "未发出或未完成 POST /admin/customer-profiles"
        })
    await submit.click({ force: true })

    let status = "wait"
    try {
        await expect
            .poll(
                async () => {
                    if (!(await dialog.isVisible().catch(() => false))) {
                        status = "ok"
                        return status
                    }
                    if (await createSucceeded(page, dialog).first().isVisible().catch(() => false)) {
                        status = "ok"
                        return status
                    }
                    const errors = (
                        await dialog.locator('[data-slot="field-error"]').allTextContents()
                    )
                        .map((text) => text.trim())
                        .filter(Boolean)
                    if (errors.length) {
                        status = `invalid:${errors.join("；")}`
                        return status
                    }
                    if (network && network.startsWith("HTTP")) {
                        status = `http:${network}`
                        return status
                    }
                    const rejected = dialog.locator("#customers-form-create-rejected")
                    if (await rejected.isVisible().catch(() => false)) {
                        status = `rejected:${(await rejected.innerText()).trim()}`
                        return status
                    }
                    const errorToast = page.locator('[data-slot="toast"]').filter({
                        hasText: /失败|无法|错误|未完成|频繁/,
                    })
                    if (await errorToast.first().isVisible().catch(() => false)) {
                        status = `toast:${(await errorToast.first().innerText()).trim()}`
                        return status
                    }
                    status = "wait"
                    return status
                },
                { timeout: UI_TIMEOUT, intervals: [200, 400, 800] },
            )
            .not.toBe("wait")
    } catch {
        throw new Error(
            `创建客户未成功: ${await readDialogDiagnostics(page, dialog, network)}`,
        )
    }

    if (status !== "ok") {
        throw new Error(
            `创建客户未成功: ${status} | ${await readDialogDiagnostics(page, dialog, network)}`,
        )
    }
    await dismissToasts(page)
    if (await dialog.isVisible().catch(() => false)) {
        const complete = dialog.locator("#customers-form-complete")
        if (await complete.isVisible().catch(() => false)) {
            await complete.click().catch(() => undefined)
        }
        await expect(dialog).toBeHidden({ timeout: UI_TIMEOUT })
    }
}
