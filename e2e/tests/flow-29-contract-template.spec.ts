import { readFile } from "node:fs/promises"
import { randomUUID } from "node:crypto"

import { test, expect, type Page, type Response as UiResponse } from "../helpers/test"
import { API_BASE, apiGet, apiToken } from "../helpers/api"
import { openLoggedInWorkspace } from "../helpers/login"
import { chooseOption } from "../helpers/ui"
import {
    DOCX_MIME,
    contractDocxParts,
    expectStampedContractDocx,
    zipParts,
    type ZipPart,
} from "../helpers/contract-template-docx"

type Group = "FSY" | "ZHYF" | "GYL" | "BDKJ"
type ApiPage<T> = { items: T[]; total: number; page: number; page_size: number }
type Template = {
    id: string
    version: number
    name: string
    company_id: string
    company_name: string
    group: Group
    file_name: string
    enabled: boolean
    created_at: number
}
type Company = {
    id: string
    version: number
    party_no: string
    legal_name: string
    short_name: string | null
    aliases: string[]
    unified_credit_code: string | null
    status: "active" | "disabled"
}
type Application = {
    id: string
    contract_no: string
    template_name: string
    company_name: string
    purpose: string
}
type ApplicationInput = { command_id: string; template_id: string; purpose: string }
type Counter = { group: Group; year: number; last_sequence: number; version: number | null }
type Envelope<T> = { success: boolean; data?: T; errorMessage?: string }
let restoreQualifications: (() => Promise<void>) | undefined

async function jsonRequest<T>(token: string, path: string, input: unknown, method = "POST") {
    const response = await fetch(`${API_BASE}${path}`, {
        method,
        headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
        body: JSON.stringify(input),
        signal: AbortSignal.timeout(30_000),
    })
    return { response, body: await response.json() as Envelope<T> }
}

async function command<T>(token: string, path: string, input: unknown, method = "POST"): Promise<T> {
    const { response, body } = await jsonRequest<T>(token, path, input, method)
    expect(response.ok, `${method} ${path}: ${body.errorMessage ?? response.status}`).toBe(true)
    expect(body.success).toBe(true)
    expect(body.data).toBeDefined()
    return body.data!
}

async function rejected(token: string, path: string, input: unknown, status: number, message: RegExp) {
    const { response, body } = await jsonRequest(token, path, input)
    expect(response.status, body.errorMessage).toBe(status)
    expect(body.success).toBe(false)
    expect(body.errorMessage).toMatch(message)
}

async function uiData<T>(response: Pick<UiResponse, "ok" | "json">): Promise<T> {
    const body = await response.json() as Envelope<T>
    expect(response.ok(), body.errorMessage).toBe(true)
    expect(body.success).toBe(true)
    expect(body.data).toBeDefined()
    return body.data!
}

async function upload(
    token: string,
    company: Company,
    group: Group,
    name: string,
    parts: ZipPart[],
    options: { filename?: string; mime?: string; bytes?: Buffer; duplicate?: "file" | "command"; fileFirst?: boolean } = {},
) {
    const form = new FormData()
    const metadata = JSON.stringify({ name, company_id: company.id, group })
    const blob = new Blob([new Uint8Array(options.bytes ?? zipParts(parts))], { type: options.mime ?? DOCX_MIME })
    const filename = options.filename ?? "contract.docx"
    if (options.fileFirst) form.append("file", blob, filename)
    form.append("command", metadata)
    if (!options.fileFirst) form.append("file", blob, filename)
    if (options.duplicate === "command") form.append("command", metadata)
    if (options.duplicate === "file") form.append("file", blob, filename)
    for (let attempt = 0; attempt < 2; attempt += 1) {
        const response = await fetch(`${API_BASE}/admin/contract-templates`, {
            method: "POST",
            headers: { Authorization: `Bearer ${token}` },
            body: form,
            signal: AbortSignal.timeout(60_000),
        })
        const body = await response.json() as Envelope<Template>
        if (response.status !== 429 || attempt === 1) return { response, body }
        // 上传限流明确保证请求尚未执行；按服务端窗口重试同一表单。
        const seconds = Number(response.headers.get("retry-after") ?? "60")
        expect(seconds).toBeGreaterThan(0)
        expect(seconds).toBeLessThanOrEqual(60)
        await new Promise((resolve) => setTimeout(resolve, seconds * 1000 + 200))
    }
    throw new Error("上传重试未返回结果")
}

async function createCompany(token: string, group: string, suffix: string): Promise<Company> {
    return command(token, "/admin/companies", {
        party_no: `E2E-CT-${group}-${suffix}`,
        legal_name: `E2E 合同主体 ${group} ${suffix}`,
        short_name: null,
        aliases: [],
        unified_credit_code: null,
        status: "active",
    })
}

/** 既有岗位角色允许自定义；在隔离库显式授予本流程资格，结束后恢复原绑定。 */
async function prepareQualifications(token: string, suffix: string): Promise<void> {
    const accounts = await apiGet<Array<{ id: string; account: string; role_ids: string[] }>>(token, "/admin/admins")
    const saved: Array<{ id: string; roles: string[]; temporary: string }> = []
    restoreQualifications = async () => {
        for (const item of saved.reverse()) {
            const restored = await jsonRequest(token, `/admin/admins/${item.id}/role`, { role_ids: item.roles }, "PUT")
            expect(restored.response.ok, restored.body.errorMessage).toBe(true)
            expect(restored.body.success).toBe(true)
            const removed = await jsonRequest(token, `/admin/roles/${item.temporary}`, undefined, "DELETE")
            expect(removed.response.ok, removed.body.errorMessage).toBe(true)
            expect(removed.body.success).toBe(true)
        }
    }
    for (const spec of [
        { account: "xitong", permissions: ["contract_template:list", "contract_template:manage", "company:list", "company:detail"] },
        { account: "xiaoshou", permissions: ["contract_template:list", "contract_application:list", "contract_application:create", "contract_application:download"] },
    ]) {
        const account = accounts.find((row) => row.account === spec.account)
        expect(account, `固定岗位账号 ${spec.account}`).toBeTruthy()
        const temporary = await command<string>(token, "/admin/roles", { name: `E2E 合同资格 ${spec.account} ${suffix}`, permissions: spec.permissions })
        saved.push({ id: account!.id, roles: [...account!.role_ids], temporary })
        const bound = await jsonRequest(token, `/admin/admins/${account!.id}/role`, { role_ids: [...account!.role_ids, temporary] }, "PUT")
        expect(bound.response.ok, bound.body.errorMessage).toBe(true)
        expect(bound.body.success).toBe(true)
    }
}

test.afterEach(async () => {
    const restore = restoreQualifications
    restoreQualifications = undefined
    if (restore) await restore()
})

async function saveCompany(token: string, company: Company, changes: Partial<Company>): Promise<Company> {
    const { id, party_no, version, legal_name, short_name, aliases, unified_credit_code, status } = { ...company, ...changes }
    const input = { party_no, version, legal_name, short_name, aliases, unified_credit_code, status }
    return command(token, `/admin/companies/${id}`, input, "PUT")
}

const counters = (token: string) => apiGet<Counter[]>(token, "/admin/contract-number-counters")
const counter = (rows: Counter[], group: Group) => rows.find((row) => row.group === group)!
const applications = (token: string) => apiGet<ApiPage<Application>>(token, "/admin/contract-applications", { page_size: 100 })
const applyInput = (template: Template, purpose: string): ApplicationInput => ({ command_id: randomUUID(), template_id: template.id, purpose })
const applicationPath = "/admin/contract-applications"

async function browserApply(page: Page, template: Template, purpose: string) {
    await page.locator(`#contract-template-apply-${template.id}`).click()
    await page.locator("#template-apply-purpose").fill(purpose)
    const [response, download] = await Promise.all([
        page.waitForResponse((candidate) => candidate.request().method() === "POST" && new URL(candidate.url()).pathname === applicationPath),
        page.waitForEvent("download"),
        page.locator("#template-apply-submit").click(),
    ])
    const result = await uiData<Application>(response)
    expect(download.suggestedFilename()).toBe(`${result.contract_no}.docx`)
    await expect(page.locator("#template-apply-dialog")).toContainText(result.contract_no)
    const input = response.request().postDataJSON() as ApplicationInput
    const bytes = await readFile((await download.path())!)
    await page.locator("#template-apply-done").click()
    return { result, input, bytes }
}

async function apiDownload(token: string, id: string): Promise<Buffer> {
    const response = await fetch(`${API_BASE}${applicationPath}/${id}/download`, {
        headers: { Authorization: `Bearer ${token}` },
        signal: AbortSignal.timeout(60_000),
    })
    expect(response.status).toBe(200)
    expect(response.headers.get("content-type")).toBe(DOCX_MIME)
    expect(response.headers.get("cache-control")).toBe("private, no-store")
    expect(response.headers.get("x-content-type-options")).toBe("nosniff")
    return Buffer.from(await response.arrayBuffer())
}

test("[flow-29] Word 模板维护、编号申请恢复、历史文件及权限和流水边界", async ({ browser }, testInfo) => {
    test.setTimeout(6 * 60 * 1000)
    // 本流程包含流水耗尽验收，只允许在可整体销毁的隔离数据库执行。
    expect(Boolean(process.env.ERP_E2E_CONFIG_PATH) || process.env.ERP_E2E_ISOLATED === "1",
        "flow-29 必须通过 python3 scripts/run-e2e-parallel.py e2e/tests/flow-29-contract-template.spec.ts 执行，禁止消耗开发库流水").toBe(true)
    const suffix = Date.now().toString(36).toUpperCase()
    const marker = `FLOW29-${suffix}`
    const parts = contractDocxParts(marker)
    const adminToken = await apiToken("admin")
    await prepareQualifications(adminToken, suffix)
    const { page: manager } = await openLoggedInWorkspace(browser, "xitong")
    const managerToken = await apiToken("xitong")
    const { page: sales } = await openLoggedInWorkspace(browser, "xiaoshou")
    const salesToken = await apiToken("xiaoshou")
    const company = await createCompany(adminToken, "FSY", suffix)
    let main!: Template
    const templates = new Map<Group, Template>()
    const companies = new Map<Group, Company>([["FSY", company]])
    const initial = await counters(managerToken)
    expect(initial.map((row) => row.group)).toEqual(["FSY", "ZHYF", "GYL", "BDKJ"])
    for (const row of initial) {
        expect(row.last_sequence).toBeLessThan(9900)
        if (row.version === null) expect(row.last_sequence).toBe(row.year === 2026
            ? { FSY: 452, ZHYF: 22, GYL: 15, BDKJ: 24 }[row.group] : 0)
    }

    await test.step("管理员浏览器上传两节含图片 Word，样张保留正文且不占号", async () => {
        await manager.goto("/sales/contract-templates")
        await manager.locator("#contract-templates-page-action-upload").click()
        await manager.locator("#template-upload-name").fill(`E2E 合同模板 ${suffix}`)
        await chooseOption(manager, manager.locator("#template-upload-company"), company.legal_name)
        await manager.locator("#template-upload-file-input").setInputFiles({
            name: "contract.docx", mimeType: DOCX_MIME, buffer: zipParts(parts),
        })
        const [response] = await Promise.all([
            manager.waitForResponse((candidate) => candidate.request().method() === "POST" && new URL(candidate.url()).pathname === "/admin/contract-templates"),
            manager.locator("#template-upload-submit").click(),
        ])
        main = await uiData<Template>(response)
        templates.set("FSY", main)
        expect(main).toMatchObject({ enabled: true, group: "FSY", company_id: company.id, company_name: company.legal_name, file_name: "contract.docx" })
        expect(main).not.toHaveProperty("object_key")
        await expect(manager.locator("#template-upload-dialog")).toHaveCount(0)
        const [sampleResponse, download] = await Promise.all([
            manager.waitForResponse((candidate) => new URL(candidate.url()).pathname === `/admin/contract-templates/${main.id}/sample`),
            manager.waitForEvent("download"),
            manager.locator(`#contract-template-sample-${main.id}`).click(),
        ])
        expect(sampleResponse.headers()["content-type"]).toBe(DOCX_MIME)
        expect(sampleResponse.headers()["content-disposition"]).toContain("FSY-S-SAMPLE.docx")
        expect(download.suggestedFilename()).toBe("FSY-S-SAMPLE.docx")
        await expectStampedContractDocx(manager, await readFile((await download.path())!), parts, "FSY-S-SAMPLE", marker)
        expect(await counters(managerToken)).toEqual(initial)
    })

    await test.step("服务器拒绝伪装 DOCX、危险压缩包和重复 multipart，失败不产生模板", async () => {
        const before = await apiGet<ApiPage<Template>>(managerToken, "/admin/contract-templates", { include_disabled: true, page_size: 100 })
        const invalid: Array<{ label: string; parts?: ZipPart[]; options?: Parameters<typeof upload>[5] }> = [
            { label: "旧 Word 扩展名", options: { filename: "contract.doc" } },
            { label: "伪装 MIME", options: { mime: "application/pdf" } },
            { label: "非 ZIP 内容", options: { bytes: Buffer.from("not a DOCX package") } },
            { label: "超过 20 MB 文件", options: { bytes: Buffer.alloc(20 * 1024 * 1024 + 1) } },
            { label: "不兼容 Word 主文档类型", parts: parts.map((part) => part.name === "[Content_Types].xml"
                ? { ...part, bytes: Buffer.from(part.bytes.toString().replace("wordprocessingml.document.main+xml", "wordprocessingml.template.main+xml")) } : part) },
            { label: "路径穿越", parts: [...parts, { name: "../outside.xml", bytes: Buffer.from("x") }] },
            { label: "宏项目", parts: [...parts, { name: "word/vbaProject.bin", bytes: Buffer.from("x") }] },
            { label: "包签名", parts: [...parts, { name: "_xmlsignatures/sig1.xml", bytes: Buffer.from("x") }] },
            { label: "ZIP 重复条目", parts: [...parts, parts[0]!] },
            { label: "超限 XML 展开", parts: [...parts, { name: "word/large.xml", bytes: Buffer.alloc(10 * 1024 * 1024 + 1, 32) }] },
            { label: "超限单条目展开", parts: [...parts, { name: "word/media/large.bin", bytes: Buffer.alloc(32 * 1024 * 1024 + 1) }] },
            { label: "超限条目数量", parts: [...parts, ...Array.from({ length: 2049 }, (_, index) => ({ name: `extra/${index}`, bytes: Buffer.alloc(0) }))] },
            { label: "重复 command", options: { duplicate: "command" } },
            { label: "重复 file", options: { duplicate: "file" } },
        ]
        for (const [index, invalidCase] of invalid.entries()) {
            const { response, body } = await upload(index % 2 === 0 ? managerToken : adminToken, company, "FSY", invalidCase.label, invalidCase.parts ?? parts, invalidCase.options)
            expect(response.status, invalidCase.label).toBe(400)
            expect(body.success, invalidCase.label).toBe(false)
        }
        const after = await apiGet<ApiPage<Template>>(managerToken, "/admin/contract-templates", { include_disabled: true, page_size: 100 })
        expect(after).toEqual(before)
        expect(await counters(managerToken)).toEqual(initial)
    })

    await test.step("四编号组目录与销售维护权限隔离", async () => {
        for (const group of ["ZHYF", "GYL", "BDKJ"] as const) {
            const nextCompany = await createCompany(adminToken, group, suffix)
            companies.set(group, nextCompany)
            const { response, body } = await upload(adminToken, nextCompany, group, `E2E ${group} 模板 ${suffix}`, parts, { fileFirst: true })
            expect(response.ok, body.errorMessage).toBe(true)
            templates.set(group, body.data!)
        }
        await sales.goto("/sales/contract-templates")
        await expect(sales.locator(`#contract-template-apply-${main.id}`)).toBeVisible()
        await expect(sales.locator("#contract-templates-page-action-upload")).toHaveCount(0)
        await expect(sales.locator("#contract-templates-page-action-counter")).toHaveCount(0)
        await expect(sales.locator(`[id^="contract-template-status-"]`)).toHaveCount(0)
        const deniedUpload = await upload(salesToken, company, "FSY", "越权上传", parts)
        expect(deniedUpload.response.status).toBe(403)
        await rejected(salesToken, `/admin/contract-templates/${main.id}/status`, { version: main.version, enabled: false }, 403, /权限/)
        await rejected(salesToken, "/admin/contract-number-counters", initial[0], 403, /权限/)
        for (const path of ["/admin/contract-number-counters", `/admin/contract-templates/${main.id}/sample`]) {
            const response = await fetch(`${API_BASE}${path}`, { headers: { Authorization: `Bearer ${salesToken}` } })
            expect(response.status).toBe(403)
        }
        await rejected(managerToken, applicationPath, applyInput(main, "管理员不得代领"), 403, /权限/)
    })

    let first!: Awaited<ReturnType<typeof browserApply>>
    await test.step("销售申请带原编号 Word，重放、重复下载及本人目录不多占号", async () => {
        first = await browserApply(sales, main, `客户福利 ${suffix}`)
        const initialFsy = counter(initial, "FSY")
        expect(first.result.contract_no).toBe(`FSY-S-${String(initialFsy.year % 100).padStart(2, "0")}${String(initialFsy.last_sequence + 1).padStart(4, "0")}`)
        await expectStampedContractDocx(sales, first.bytes, parts, first.result.contract_no, marker)
        const after = await counters(managerToken)
        expect(counter(after, "FSY").last_sequence).toBe(initialFsy.last_sequence + 1)
        for (const group of ["ZHYF", "GYL", "BDKJ"] as const) expect(counter(after, group)).toEqual(counter(initial, group))
        expect(await command(salesToken, applicationPath, first.input)).toEqual(first.result)
        await rejected(salesToken, applicationPath, { ...first.input, purpose: "修改申请用途" }, 409, /已登记/)
        await sales.locator("#contract-applications-view").click()
        await expect(sales.locator("#contract-applications-table")).toContainText(first.result.contract_no)
        const [download] = await Promise.all([
            sales.waitForEvent("download"),
            sales.locator(`#contract-application-download-${first.result.id}`).click(),
        ])
        expect(download.suggestedFilename()).toBe(`${first.result.contract_no}.docx`)
        expect(await readFile((await download.path())!)).toEqual(first.bytes)
        expect(await counters(managerToken)).toEqual(after)
        const foreignList = await applications(adminToken)
        expect(foreignList.items.map((row) => row.id)).not.toContain(first.result.id)
        const ownList = await applications(salesToken)
        expect(ownList.items).toContainEqual(expect.objectContaining({ id: first.result.id }))
        for (const id of [first.result.id, "missing-application"]) {
            const foreign = await fetch(`${API_BASE}${applicationPath}/${id}/download`, { headers: { Authorization: `Bearer ${adminToken}` } })
            expect(foreign.status).toBe(404)
            expect((await foreign.json()).errorMessage).toMatch(/不存在或无权/)
        }
        const archives = await apiGet<ApiPage<{ contract_no: string }>>(salesToken, "/admin/contracts", { q: first.result.contract_no })
        expect(archives.items).toHaveLength(0)
        await sales.locator("#contract-templates-view").click()
    })

    await test.step("真实服务器提交后丢失响应，刷新恢复同一请求且仅消耗一次流水", async () => {
        const before = await counters(managerToken)
        let committed!: Application
        let originalInput!: ApplicationInput
        const dropResponse = async (route: import("@playwright/test").Route) => {
            if (route.request().method() !== "POST") return route.fallback()
            originalInput = route.request().postDataJSON() as ApplicationInput
            // 实际提交到当前 shard，只丢弃浏览器响应以制造已提交但未确认的网络状态。
            const response = await route.fetch({ url: `${API_BASE}${applicationPath}` })
            committed = await uiData<Application>(response)
            await route.abort("failed")
        }
        await sales.route("**/admin/contract-applications", dropResponse)
        try {
            await sales.locator(`#contract-template-apply-${main.id}`).click()
            await sales.locator("#template-apply-purpose").fill(`待核对用途 ${suffix}`)
            await sales.locator("#template-apply-submit").click()
            await expect(sales.locator("#template-apply-dialog [role=alert]")).toContainText("结果暂未确认")
            await expect(sales.locator("#template-apply-purpose")).toBeDisabled()
        } finally {
            await sales.unroute("**/admin/contract-applications", dropResponse)
        }
        expect(committed.id).toBeTruthy()
        await sales.reload()
        await sales.locator("#contract-application-resume").click()
        await expect(sales.locator("#template-apply-purpose")).toHaveValue(originalInput.purpose)
        await expect(sales.locator("#template-apply-purpose")).toBeDisabled()
        const [response, download] = await Promise.all([
            sales.waitForResponse((candidate) => candidate.request().method() === "POST" && new URL(candidate.url()).pathname === applicationPath),
            sales.waitForEvent("download"),
            sales.locator("#template-apply-submit").click(),
        ])
        expect(response.request().postDataJSON()).toEqual(originalInput)
        expect(await uiData<Application>(response)).toEqual(committed)
        expect(download.suggestedFilename()).toBe(`${committed.contract_no}.docx`)
        await sales.locator("#template-apply-done").click()
        await expect(sales.locator("#contract-application-resume")).toHaveCount(0)
        expect(counter(await counters(managerToken), "FSY").last_sequence).toBe(counter(before, "FSY").last_sequence + 1)
    })

    await test.step("Word 下载失败后通过原记录重下载，其他编号组独立分配", async () => {
        const zhyf = templates.get("ZHYF")!
        await sales.route("**/admin/contract-applications/*/download", (route) => route.abort("failed"), { times: 1 })
        await sales.locator(`#contract-template-apply-${zhyf.id}`).click()
        await sales.locator("#template-apply-purpose").fill("下载失败后重试")
        const [response] = await Promise.all([
            sales.waitForResponse((candidate) => candidate.request().method() === "POST" && new URL(candidate.url()).pathname === applicationPath),
            sales.locator("#template-apply-submit").click(),
        ])
        const result = await uiData<Application>(response)
        await expect(sales.locator("#template-apply-dialog [role=alert]")).toBeVisible()
        await expect(sales.locator("#template-apply-redownload")).toBeEnabled()
        const afterFailedDownload = await counters(managerToken)
        expect(counter(afterFailedDownload, "ZHYF").last_sequence).toBe(counter(initial, "ZHYF").last_sequence + 1)
        const [download] = await Promise.all([
            sales.waitForEvent("download"),
            sales.locator("#template-apply-redownload").click(),
        ])
        expect(download.suggestedFilename()).toBe(`${result.contract_no}.docx`)
        await expectStampedContractDocx(sales, await readFile((await download.path())!), parts, result.contract_no, marker)
        await sales.locator("#template-apply-done").click()
        expect(await counters(managerToken)).toEqual(afterFailedDownload)
        for (const group of ["GYL", "BDKJ"] as const) {
            const result = await command<Application>(salesToken, applicationPath, applyInput(templates.get(group)!, `${group} 独立流水`))
            const base = counter(initial, group)
            expect(result.contract_no).toBe(`${group}-S-${String(base.year % 100).padStart(2, "0")}${String(base.last_sequence + 1).padStart(4, "0")}`)
        }
    })

    await test.step("同组不同公司并发申请号码唯一，相同命令并发重试不多发号", async () => {
        const secondCompany = await createCompany(adminToken, "FSY2", suffix)
        const uploaded = await upload(managerToken, secondCompany, "FSY", `E2E 共享 FSY ${suffix}`, parts)
        expect(uploaded.response.ok, uploaded.body.errorMessage).toBe(true)
        const second = uploaded.body.data!
        const before = counter(await counters(managerToken), "FSY")
        const inputs = [applyInput(main, "并发甲"), applyInput(second, "并发乙")]
        const withContentionRetry = async (input: ApplicationInput): Promise<Application> => {
            for (let attempt = 0; attempt < 5; attempt += 1) {
                const { response, body } = await jsonRequest<Application>(salesToken, applicationPath, input)
                if (response.ok) return body.data!
                expect(response.status, body.errorMessage).toBe(409)
            }
            throw new Error("同一申请连续五次事务冲突")
        }
        const results = await Promise.all([...inputs, inputs[0]!].map(withContentionRetry))
        expect(results[0]).toEqual(results[2])
        expect(new Set(results.map((result) => result.contract_no)).size).toBe(2)
        expect(counter(await counters(managerToken), "FSY").last_sequence).toBe(before.last_sequence + 2)
        const own = await applications(salesToken)
        expect(own.items.filter((row) => results.some((result) => result.id === row.id))).toHaveLength(2)
    })

    await test.step("模板停用与旧版本拒绝，原申请仍下载原正文；替换模板沿用公司编号组", async () => {
        await manager.reload()
        const [response] = await Promise.all([
            manager.waitForResponse((candidate) => candidate.request().method() === "POST" && new URL(candidate.url()).pathname === `/admin/contract-templates/${main.id}/status`),
            manager.locator(`#contract-template-status-${main.id}`).click(),
        ])
        const disabled = await uiData<Template>(response)
        expect(disabled).toMatchObject({ enabled: false, version: main.version + 1, company_id: main.company_id, group: "FSY" })
        const before = await counters(managerToken)
        await rejected(managerToken, `/admin/contract-templates/${main.id}/status`, { version: main.version, enabled: true }, 409, /已变化/)
        await rejected(salesToken, applicationPath, applyInput(main, "停用后申请"), 422, /模板已停用/)
        expect(await command(salesToken, applicationPath, first.input)).toEqual(first.result)
        expect(await apiDownload(salesToken, first.result.id)).toEqual(first.bytes)
        const replacementParts = contractDocxParts(`REPLACEMENT-${suffix}`)
        const replacement = await upload(managerToken, company, "FSY", `E2E 替换正文 ${suffix}`, replacementParts)
        expect(replacement.response.ok, replacement.body.errorMessage).toBe(true)
        expect(replacement.body.data!.id).not.toBe(main.id)
        const wrongGroup = await upload(managerToken, company, "GYL", "禁止变更原编号组", parts)
        expect(wrongGroup.response.status).toBe(409)
        expect(wrongGroup.body.errorMessage).toMatch(/沿用原编号组/)
        expect(await counters(managerToken)).toEqual(before)
        await sales.reload()
        await expect(sales.locator(`#contract-template-apply-${main.id}`)).toHaveCount(0)
        await expect(sales.locator(`#contract-template-apply-${replacement.body.data!.id}`)).toBeVisible()
    })

    await test.step("公司改名仍沿用 GYL，停用公司拒绝新申请且保留历史快照", async () => {
        const gylTemplate = templates.get("GYL")!
        const oldCompany = companies.get("GYL")!
        const oldApplication = await command<Application>(salesToken, applicationPath, applyInput(gylTemplate, "公司停用前申请"))
        const renamed = await saveCompany(adminToken, oldCompany, { legal_name: `${oldCompany.legal_name} 新名称` })
        const updated = await upload(managerToken, renamed, "GYL", `E2E 改名后 GYL ${suffix}`, parts)
        expect(updated.response.ok, updated.body.errorMessage).toBe(true)
        const current = await command<Application>(salesToken, applicationPath, applyInput(updated.body.data!, "改名后申请"))
        expect(current.contract_no).toMatch(/^GYL-S-\d{6}$/)
        expect(current.company_name).toBe(renamed.legal_name)
        await saveCompany(adminToken, renamed, { status: "disabled" })
        const before = await counters(managerToken)
        await rejected(salesToken, applicationPath, applyInput(updated.body.data!, "公司停用后申请"), 400, /启用的公司/)
        expect(await counters(managerToken)).toEqual(before)
        const own = await applications(salesToken)
        expect(own.items.find((item) => item.id === oldApplication.id)?.company_name).toBe(oldCompany.legal_name)
        await expectStampedContractDocx(sales, await apiDownload(salesToken, oldApplication.id), parts, oldApplication.contract_no, marker)
    })

    await test.step("目录稳定分页、空页和非法申请不会写入或消耗号码", async () => {
        for (const path of ["/admin/contract-templates", applicationPath]) {
            const token = path === applicationPath ? salesToken : managerToken
            const firstPage = await apiGet<ApiPage<{ id: string }>>(token, path, { page: 1, page_size: 2, include_disabled: true })
            const secondPage = await apiGet<ApiPage<{ id: string }>>(token, path, { page: 2, page_size: 2, include_disabled: true })
            expect(firstPage.items).toHaveLength(2)
            expect(secondPage.items).toHaveLength(2)
            expect(new Set([...firstPage.items, ...secondPage.items].map((row) => row.id)).size).toBe(4)
            expect(await apiGet(token, path, { page: 1, page_size: 2, include_disabled: true })).toEqual(firstPage)
            const empty = await apiGet<ApiPage<unknown>>(token, path, { page: 1000, page_size: 2 })
            expect(empty.items).toHaveLength(0)
            const capped = await apiGet<ApiPage<unknown>>(token, path, { page_size: 1000 })
            expect(capped.page_size).toBe(100)
        }
        const before = await counters(managerToken)
        await rejected(salesToken, applicationPath, { ...applyInput(templates.get("ZHYF")!, ""), command_id: "bad_key" }, 400, /申请信息无效/)
        await rejected(salesToken, applicationPath, applyInput(templates.get("ZHYF")!, "字".repeat(257)), 400, /256/)
        const nominated = await fetch(`${API_BASE}${applicationPath}`, {
            method: "POST",
            headers: { Authorization: `Bearer ${salesToken}`, "Content-Type": "application/json" },
            body: JSON.stringify({ ...applyInput(templates.get("ZHYF")!, "不得指定其他申请人"), applicant_id: "other-applicant" }),
        })
        expect(nominated.status).toBe(422)
        expect(await nominated.text()).toContain("unknown field")
        expect(await counters(managerToken)).toEqual(before)
    })

    await test.step("真实目录请求失败可重新加载，窄窗口保留维护、申请和下载动作", async () => {
        const failList = (route: import("@playwright/test").Route) => route.abort("failed")
        await sales.route("**/admin/contract-templates?*", failList)
        try {
            await sales.reload()
            await expect(sales.locator("#contract-templates-retry")).toBeVisible()
            await expect(sales.locator("[role=alert]").first()).toBeVisible()
        } finally {
            await sales.unroute("**/admin/contract-templates?*", failList)
        }
        await sales.locator("#contract-templates-retry").click()
        await expect(sales.locator(`#contract-template-apply-${templates.get("ZHYF")!.id}`)).toBeVisible()
        await sales.setViewportSize({ width: 760, height: 900 })
        await expect(sales.locator(`#contract-template-apply-${templates.get("ZHYF")!.id}`)).toBeVisible()
        await sales.locator("#contract-applications-view").click()
        await expect(sales.locator(`#contract-application-download-${first.result.id}`)).toBeVisible()
        const salesDimensions = await sales.evaluate(() => ({ width: document.documentElement.clientWidth, scroll: document.documentElement.scrollWidth }))
        expect(salesDimensions.scroll).toBeLessThanOrEqual(salesDimensions.width + 1)
        await manager.setViewportSize({ width: 760, height: 900 })
        await expect(manager.locator("#contract-templates-page-action-upload")).toBeVisible()
        await expect(manager.locator("#contract-templates-page-action-counter")).toBeVisible()
        const managerDimensions = await manager.evaluate(() => ({ width: document.documentElement.clientWidth, scroll: document.documentElement.scrollWidth }))
        expect(managerDimensions.scroll).toBeLessThanOrEqual(managerDimensions.width + 1)
        await sales.setViewportSize({ width: 1440, height: 900 })
        await manager.setViewportSize({ width: 1440, height: 900 })
    })

    await test.step("浏览器向前校准流水，回退与旧版本拒绝，9999 后不生成新号", async () => {
        await manager.locator("#contract-templates-page-action-counter").click()
        const before = counter(await counters(managerToken), "FSY")
        await manager.locator("#template-counter-fsy-last").fill(String(before.last_sequence + 2))
        const [response] = await Promise.all([
            manager.waitForResponse((candidate) => candidate.request().method() === "POST" && new URL(candidate.url()).pathname === "/admin/contract-number-counters"),
            manager.locator("#template-counter-fsy-save").click(),
        ])
        const advanced = await uiData<Counter>(response)
        expect(advanced.last_sequence).toBe(before.last_sequence + 2)
        await expect(manager.locator("#template-counter-fsy-last")).toHaveValue(String(advanced.last_sequence))
        await manager.locator("#template-counter-close").click()
        await rejected(managerToken, "/admin/contract-number-counters", { ...before, last_sequence: advanced.last_sequence + 1 }, 409, /流水已变化/)
        await rejected(managerToken, "/admin/contract-number-counters", { ...advanced, last_sequence: advanced.last_sequence - 1 }, 400, /只能向前/)
        const bdkj = counter(await counters(managerToken), "BDKJ")
        const exhausted = await command<Counter>(managerToken, "/admin/contract-number-counters", { ...bdkj, last_sequence: 9999 })
        const ownBefore = await applications(salesToken)
        await rejected(salesToken, applicationPath, applyInput(templates.get("BDKJ")!, "流水耗尽不得回绕"), 422, /流水已用完/)
        expect(counter(await counters(managerToken), "BDKJ")).toEqual(exhausted)
        expect(await applications(salesToken)).toEqual(ownBefore)
    })

    await testInfo.attach("合同模板与编号验收记录", {
        body: JSON.stringify({ template_id: main.id, application_id: first.result.id, contract_no: first.result.contract_no, initial_counters: initial, final_counters: await counters(managerToken) }, null, 2),
        contentType: "application/json",
    })
})
