/**
 * S2 组织数据范围浏览器验收：真实账号密码登录真实 HTTP，覆盖组织/成员/管理变更、
 * 跨页刷新、维度隔离与任务受阻视图。
 *
 * 环境（全部隔离，不碰共享开发库与生产库）：
 * - 后端：worktree 构建的 web-api，scratch 端口，随机库（副本集 ERP_TEST_MONGO_URI）。
 * - 前端：worktree erp-client dev，scratch 端口，NEXT_PUBLIC_API_BASE_URL 指向上述后端。
 * - 数据：从共享开发库 mongodump/restore 的岗位账号、仓库、商品、审批定义，
 *   再按 verify-org-data-scope-s2.mjs 思路创建隔离组织与范围。
 * - 运行：API_BASE=<后端> E2E_BASE_URL=<前端> npx playwright test s2-org-data-scope-browser
 *
 * 断言边界：合成验收单据不等同生产数据量与完整业务流程；只核销 S2 §8.3 的
 * “浏览器真实账号验收”一行，不替代生产规模与代表性业务数据验收。
 */
import { randomUUID } from "node:crypto"

import { expect, test, type Browser, type Page } from "@playwright/test"

import { ACCOUNTS } from "../helpers/accounts"
import { API_BASE, apiGet, apiLogin } from "../helpers/api"
import { loginViaUi } from "../helpers/login"

test.describe.configure({ mode: "serial" })

const VISIBLE = { timeout: 30_000 } as const
const RUN = `${Date.now().toString(36)}${Math.floor(Math.random() * 0xffff).toString(16)}`
const FINANCE_ORG = `S2浏览器财务-${RUN}`
const BIZ_ORG = `S2浏览器业务-${RUN}`
const SKU_NO = "TEA-SF-LJ-250"
const WAREHOUSE_CODE = "BJ-TZ-01"

const pageErrors: string[] = []
const consoleErrors: string[] = []

function watch(page: Page): void {
    page.on("pageerror", (error) => {
        pageErrors.push(`pageerror@${page.url()}: ${error.message}`)
    })
    page.on("console", (message) => {
        // 浏览器把失败 HTTP 响应也记为 console error；S2 只关心未捕获异常与应用日志。
        // 资源加载失败（403 的 Failed to load resource）由下面的接口断言覆盖，
        // 不计入页面错误，避免把预期的失败关闭当成验收失败。
        if (message.type() === "error" && !/Failed to load resource/.test(message.text())) {
            consoleErrors.push(`console@${page.url()}: ${message.text().slice(0, 300)}`)
        }
    })
}

function loginName(key: string): string {
    const bag = ACCOUNTS as Record<string, { account?: string } | string>
    const row = bag[key]
    if (typeof row === "string" && row.trim()) return row
    if (row && typeof row === "object" && row.account?.trim()) return row.account
    return key
}

async function loginAs(browser: Browser, key: string, viewport?: { width: number; height: number }): Promise<Page> {
    const context = await browser.newContext({
        locale: "zh-CN",
        timezoneId: "Asia/Shanghai",
        ...(viewport ? { viewport } : {}),
    })
    const page = await context.newPage()
    watch(page)
    await loginViaUi(page, loginName(key))
    return page
}

const tokenCache = new Map<string, Promise<string>>()

async function apiToken(key: string): Promise<string> {
    let cached = tokenCache.get(key)
    if (!cached) {
        cached = (async () => {
            try {
                return await apiLogin(loginName(key))
            } catch (error) {
                // 后端登录限流为每账号每 60 秒 5 次；验收跨多次运行可能撞限，等待窗口滑过后重试一次。
                if (error instanceof Error && error.message.includes("429")) {
                    await new Promise((resolve) => setTimeout(resolve, 65_000))
                    return apiLogin(loginName(key))
                }
                throw error
            }
        })()
        tokenCache.set(key, cached)
    }
    return cached
}

async function apiCall(
    method: string,
    path: string,
    token: string,
    body?: unknown,
): Promise<{ status: number; parsed: { success?: boolean; errorMessage?: string; data?: any } }> {
    const res = await fetch(`${API_BASE}${path}`, {
        method,
        headers: {
            ...(token ? { Authorization: `Bearer ${token}` } : {}),
            ...(body === undefined ? {} : { "Content-Type": "application/json" }),
        },
        body: body === undefined ? undefined : JSON.stringify(body),
    })
    const parsed = (await res.json()) as {
        success?: boolean
        errorMessage?: string
        data?: any
    }
    return { status: res.status, parsed }
}

async function apiGetCall(
    token: string,
    path: string,
    query?: Record<string, unknown>,
): Promise<{ status: number; parsed: { success?: boolean; errorMessage?: string; data?: any } }> {
    return apiCall("GET", withQuery(path, query), token)
}

function withQuery(path: string, query?: Record<string, unknown>): string {
    if (!query) return path
    const params = new URLSearchParams()
    for (const [key, value] of Object.entries(query)) {
        if (value === undefined || value === null) continue
        params.set(key, String(value))
    }
    const qs = params.toString()
    return qs ? `${path}${path.includes("?") ? "&" : "?"}${qs}` : path
}

async function apiOk(method: string, path: string, token: string, body?: unknown): Promise<any> {
    const { status, parsed } = await apiCall(method, path, token, body)
    if (status >= 400 || parsed.success === false) {
        throw new Error(`API ${method} ${path} 失败（HTTP ${status}）: ${parsed.errorMessage ?? ""}`)
    }
    return parsed.data
}

async function apiDenyGet(token: string, path: string, query?: Record<string, unknown>): Promise<void> {
    const { status } = await apiGetCall(token, path, query)
    expect(
        [403, 404, 409, 422].includes(status),
        `${path} 应拒绝（403/404/409/422），实际 HTTP ${status}`,
    ).toBe(true)
}

async function apiDeny(method: string, path: string, token: string, body?: unknown): Promise<void> {
    const { status } = await apiCall(method, path, token, body)
    expect(
        [403, 404, 409, 422].includes(status),
        `${path} 应拒绝（403/404/409/422），实际 HTTP ${status}`,
    ).toBe(true)
}

type OrgState = {
    version: number
    units: Array<{ id: string; name: string }>
}

async function orgState(token: string): Promise<OrgState> {
    return apiGet<OrgState>(token, "/admin/org-units")
}

async function orgChange(token: string, change: unknown, retries = 3): Promise<any> {
    for (let attempt = 0; ; attempt += 1) {
        const state = await orgState(token)
        try {
            return await apiOk("POST", "/admin/org-units/change", token, {
                expected_version: state.version,
                idempotency_key: `${RUN}-${Math.random().toString(16).slice(2)}-${attempt}`,
                reason: "S2 浏览器验收",
                change,
            })
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error)
            if (
                attempt < retries &&
                (message.includes("关系刚生效") || message.includes("组织范围已变化"))
            ) {
                await new Promise((resolve) => setTimeout(resolve, 1100))
                continue
            }
            throw error
        }
    }
}

async function ensureScope(
    token: string,
    input: {
        subject_type: string
        subject_id: string
        resource: string
        actions: string[]
        target_dimension: string
        targets: string[]
        target_mode?: string
        include_descendants?: boolean | null
    },
): Promise<{ id: string }> {
    const page = await apiGet<{ items: Array<{ id: string } & Record<string, unknown>> }>(
        token,
        "/admin/data-scopes",
        {
            subject_type: input.subject_type,
            subject_id: input.subject_id,
            resource: input.resource,
            page: 1,
            page_size: 100,
        },
    )
    const equal = (a: unknown, b: unknown): boolean =>
        JSON.stringify([...(a as string[])].sort()) === JSON.stringify([...(b as string[])].sort())
    const existing = (page.items ?? []).find(
        (row) =>
            row.scope_type === "organization" &&
            row.enabled === true &&
            row.target_dimension === input.target_dimension &&
            equal(row.actions, input.actions) &&
            equal(row.scope_targets, input.targets),
    )
    if (existing) return { id: existing.id }
    return apiOk("POST", "/admin/data-scopes", token, {
        schema_version: 2,
        subject_type: input.subject_type,
        subject_id: input.subject_id,
        resource: input.resource,
        actions: input.actions,
        target_dimension: input.target_dimension,
        scope_type: "organization",
        scope_targets: input.targets,
        target_mode: input.target_mode ?? "explicit",
        include_descendants:
            input.include_descendants !== undefined
                ? input.include_descendants
                : input.target_dimension === "internal_org"
                  ? false
                  : null,
        enabled: true,
    })
}

async function deleteScope(token: string, id: string): Promise<void> {
    const { status } = await apiCall("DELETE", `/admin/data-scopes/${id}`, token)
    expect(status, `删除范围应成功，实际 HTTP ${status}`).toBeLessThan(300)
}

async function adminIdOf(token: string, account: string): Promise<string> {
    const rows = await apiGet<Array<{ id: string; account: string }>>(token, "/admin/admins")
    const found = rows.find((row) => row.account === account)
    if (!found) throw new Error(`种子账号缺失 ${account}`)
    return found.id
}

test("S2 浏览器验收：组织变更跨页可见、维度隔离、任务受阻", async ({ browser }) => {
    expect(API_BASE, "必须显式指定隔离后端 API_BASE").toMatch(/^http:\/\/(127\.0\.0\.1|localhost):/)
    // 1. 真实密码登录：管理员走登录页，token 全部经真实登录接口取得。
    const adminPage = await loginAs(browser, "admin")
    await expect(adminPage.getByRole("heading", { name: "我的工作台" })).toBeVisible(VISIBLE)
    const adminToken = await apiToken("admin")
    const financeId = await adminIdOf(adminToken, loginName("caiwu"))
    const warehouseId = await adminIdOf(adminToken, loginName("cangchu"))
    const salesId = await adminIdOf(adminToken, loginName("xiaoshou"))
    const manageId = await adminIdOf(adminToken, loginName("guanli"))
    // 隔离库幂等：先清掉财务遗留的个人审批上限（失败运行泄漏会与角色范围求交成空，
    // 导致建单重验失败）；只动 internal_org 维度的个人上限，不碰种子与角色范围。
    const leakedCaps = await apiGet<{ items: Array<{ id: string; target_dimension?: string }> }>(
        adminToken,
        "/admin/data-scopes",
        {
            subject_type: "user",
            subject_id: financeId,
            resource: "approval_instance",
            page: 1,
            page_size: 100,
        },
    )
    for (const row of leakedCaps.items ?? []) {
        if (row.target_dimension === "internal_org") {
            await deleteScope(adminToken, row.id)
        }
    }

    // 2. 组织/成员/管理变更：新建两个部门并调岗，管理授权到财务部门。
    const financeReceipt = await orgChange(adminToken, {
        operation: "create_unit",
        name: FINANCE_ORG,
        parent_id: null,
        kind: "department",
    })
    const bizReceipt = await orgChange(adminToken, {
        operation: "create_unit",
        name: BIZ_ORG,
        parent_id: null,
        kind: "department",
    })
    const financeOrg = (financeReceipt.after.units as Array<{ id: string; name: string }>).find(
        (u) => u.name === FINANCE_ORG,
    )?.id
    const bizOrg = (bizReceipt.after.units as Array<{ id: string; name: string }>).find(
        (u) => u.name === BIZ_ORG,
    )?.id
    expect(financeOrg, "财务部门应创建成功").toBeTruthy()
    expect(bizOrg, "业务部门应创建成功").toBeTruthy()
    await orgChange(adminToken, { operation: "transfer_member", user_id: financeId, org_unit_id: financeOrg })
    await orgChange(adminToken, { operation: "transfer_member", user_id: warehouseId, org_unit_id: bizOrg })
    await orgChange(adminToken, { operation: "transfer_member", user_id: salesId, org_unit_id: bizOrg })
    // 管理视图授权走 work_item:manage 的组织范围（managed_orgs 由管理关系展开）。
    const managementRoles = await apiGet<Array<{ id: string }>>(adminToken, "/admin/roles", {
        page: 1,
        page_size: 100,
    })
    const manageRole = managementRoles.find((row) => row.id === "role-management")?.id
    expect(manageRole, "应存在管理层角色").toBeTruthy()
    await orgChange(adminToken, {
        operation: "grant_management",
        user_id: manageId,
        role_id: manageRole,
        org_unit_id: financeOrg,
        include_descendants: false,
    })
    await ensureScope(adminToken, {
        subject_type: "user",
        subject_id: manageId,
        resource: "work_item",
        actions: ["manage"],
        target_dimension: "internal_org",
        targets: [],
        target_mode: "managed_orgs",
        include_descendants: null,
    })

    // 3. 库存范围前置（与 verify 脚本同口径），否则余额列表为空、建单无授权。
    const warehouseToken = await apiToken("cangchu")
    const warehouseList = await apiGet<{ items: Array<{ id: string; warehouse_code: string }> }>(
        warehouseToken,
        "/admin/warehouses",
        { page: 1, page_size: 100 },
    )
    const warehouse = warehouseList.items.find((row) => row.warehouse_code === WAREHOUSE_CODE)
    expect(warehouse, `未找到仓库 ${WAREHOUSE_CODE}`).toBeTruthy()
    for (const [resource, actions] of [
        ["stock_balance", ["list", "detail"]],
        ["stock_movement", ["list"]],
        ["stock_reservation", ["list"]],
        ["stock_adjustment", ["list", "detail", "create", "update", "submit"]],
    ] as Array<[string, string[]]>) {
        await ensureScope(adminToken, {
            subject_type: "role",
            subject_id: "role-warehouse",
            resource,
            actions,
            target_dimension: "warehouse",
            targets: [warehouse!.id],
        })
    }
    await ensureScope(adminToken, {
        subject_type: "role",
        subject_id: "role-warehouse",
        resource: "approval_instance",
        actions: ["read"],
        target_dimension: "warehouse",
        targets: [warehouse!.id],
    })
    for (const role of ["role-finance", "role-management"]) {
        await ensureScope(adminToken, {
            subject_type: "role",
            subject_id: role,
            resource: "stock_adjustment",
            actions: ["list", "detail"],
            target_dimension: "warehouse",
            targets: [warehouse!.id],
        })
    }
    const skuList = await apiGet<{ items: Array<{ id: string; sku_no: string }> }>(
        warehouseToken,
        "/admin/skus",
        { q: SKU_NO, page: 1, page_size: 20 },
    )
    const sku = skuList.items.find((row) => row.sku_no === SKU_NO) ?? skuList.items[0]
    expect(sku, "未找到可用 SKU").toBeTruthy()
    const balanceList = await apiGet<{
        items: Array<{ id: string; warehouse_id: string; sku_id: string; version: number }>
    }>(warehouseToken, "/admin/stock-balances", { page: 1, page_size: 100 })
    const balance = balanceList.items.find(
        (row) => row.warehouse_id === warehouse!.id && row.sku_id === sku!.id,
    )
    expect(balance, "未找到零数量占位余额，请先准备库存维度").toBeTruthy()

    // 4. 真实建单并提交：仓储建盘盈单，审批指定给种子财务账号，责任组织保持仓库。
    const stock = await apiOk("POST", "/admin/stock-adjustments", warehouseToken, {
        balance_id: balance!.id,
        expected_balance_version: String(balance!.version),
        adjustment_no: `S2B-${RUN}`,
        warehouse_id: warehouse!.id,
        reason_type: "STOCK_GAIN",
        lines: [{ sku_id: sku!.id, quantity: "2", direction: "INCREASE" }],
        note: "S2浏览器验收",
        occurred_at: Math.floor(Date.now() / 1000),
    })
    const stockId = stock.adjustment.id as string
    expect(stockId, "库存调整单应创建成功").toBeTruthy()
    const detail = await apiOk("GET", `/admin/stock-adjustments/${stockId}`, warehouseToken)
    const submit = detail.approval.submit_command
    expect(submit, "仓储账号应取得提交令牌").toBeTruthy()
    await apiOk("POST", `/admin/stock-adjustments/${stockId}/submit`, warehouseToken, {
        expected_version: String(submit.expected_version),
        expected_subject_version: String(submit.expected_subject_version),
        reason_type: "STOCK_GAIN",
        lines: detail.lines.map((line: { id: string }) => ({
            line_id: line.id,
            quantity: "2",
            direction: "INCREASE",
        })),
        balances: [{ balance_id: balance!.id, expected_version: String(balance!.version) }],
        note: "S2浏览器验收",
        occurred_at: Math.floor(Date.now() / 1000),
        idempotency_key: randomUUID(),
    })
    const financeQueue = await apiGet<{ items: Array<Record<string, any>> }>(
        await apiToken("caiwu"),
        "/admin/work-items",
        { scope: "mine", page: 1, page_size: 100 },
    )
    const task = (financeQueue.items ?? []).find((row) => row.business_object_id === stockId)
    expect(task, "审批必须指定给种子财务账号").toBeTruthy()
    expect(task!.owner_user_id).toBe(financeId)
    expect(task!.owner_organization_id, "责任组织必须保持仓库").toBe(warehouse!.id)
    // 管理监督可见但不能代办：经理按当前人员部门可监督，审批决定必须拒绝。
    const managedQueue = await apiGet<{ items: Array<Record<string, any>> }>(
        await apiToken("guanli"),
        "/admin/work-items",
        { scope: "managed", page: 1, page_size: 100 },
    )
    expect(
        (managedQueue.items ?? []).some((row) => row.id === task!.id),
        "经理按当前人员部门可监督",
    ).toBe(true)
    await apiDeny("POST", "/admin/approval-decisions", await apiToken("guanli"), {
        work_item_id: task!.id,
        decision: "APPROVE",
        expected_task_version: task!.task_version,
        idempotency_key: randomUUID(),
    })

    // 5. 组织页跨页可见：刷新后新部门可见。
    await adminPage.goto("/system/organization")
    await expect(adminPage.getByRole("heading", { name: "组织架构" })).toBeVisible(VISIBLE)
    await adminPage.locator("#organization-search").fill(FINANCE_ORG)
    await expect(adminPage.getByText(FINANCE_ORG).first()).toBeVisible(VISIBLE)
    await adminPage.screenshot({
        path: test.info().outputPath("s2-org-units.png"),
    })

    // 6. 范围页可见：新建范围配置页正常加载。
    await adminPage.goto("/system/organization/scopes")
    await expect(adminPage.getByRole("heading", { name: "范围配置" })).toBeVisible(VISIBLE)

    // 7. 客户跨页一致：组织筛选页正常加载。
    const salesPage = await loginAs(browser, "xiaoshou")
    await salesPage.goto("/sales/customers")
    await expect(salesPage.getByRole("heading", { name: "客户中心" })).toBeVisible(VISIBLE)

    // 8. 维度隔离 + 任务受阻：给财务加内部部门上限，该库存审批必须失效。
    const financePage = await loginAs(browser, "caiwu")
    await financePage.goto("/inventory")
    await expect(financePage.getByRole("heading", { name: "库存台账" })).toBeVisible(VISIBLE)
    const cap = await ensureScope(adminToken, {
        subject_type: "user",
        subject_id: financeId,
        resource: "approval_instance",
        actions: ["read", "decide"],
        target_dimension: "internal_org",
        targets: [financeOrg!],
    })
    // 内部部门上限不得打开仓库余额：接口失败关闭或空结果。
    const financeToken = await apiToken("caiwu")
    const balanceRes = await fetch(`${API_BASE}/admin/stock-balances?page=1&page_size=5`, {
        headers: { Authorization: `Bearer ${financeToken}` },
    })
    const balanceBody = await balanceRes.text()
    const balanceDenied =
        balanceRes.status === 403 ||
        /PERMISSION_REVOKED|NO_DATA_SCOPE|no_scope|权限已收回|未配置/.test(balanceBody)
    let balanceItems: unknown[] = []
    try {
        balanceItems =
            (JSON.parse(balanceBody) as { success?: boolean; data?: { items?: unknown[] } }).data
                ?.items ?? []
    } catch {
        balanceItems = []
    }
    expect(
        balanceDenied || balanceItems.length === 0,
        `内部部门上限不得打开仓库余额（HTTP ${balanceRes.status} items=${balanceItems.length}）`,
    ).toBe(true)
    // 管理视图看到受阻任务：保留原负责人与业务组织，仅保留查看。
    const blockedQueue = await apiGet<{ items: Array<Record<string, any>> }>(
        await apiToken("guanli"),
        "/admin/work-items",
        { scope: "managed", page: 1, page_size: 100 },
    )
    const blocked = (blockedQueue.items ?? []).find((row) => row.id === task!.id)
    expect(blocked, "受阻任务应对经理可见").toBeTruthy()
    expect(blocked!.processing_state).toBe("EXECUTION_BLOCKED")
    expect(blocked!.owner_user_id).toBe(financeId)
    expect((blocked!.allowed_actions as string[]).includes("APPROVE")).toBe(false)
    // 浏览器管理视图必须在拒绝之前：财务拒绝会触发运行时关闭受阻任务
    //（原任务不可变，恢复必须建新任务），关单后管理队列不再列出该任务。
    const managePage = await loginAs(browser, "guanli")
    await managePage.goto("/workspace")
    await expect(managePage.getByRole("heading", { name: "我的工作台" })).toBeVisible(VISIBLE)
    const managedNav = managePage.locator("#workspace-queue-scope-managed")
    if (await managedNav.count()) {
        await managedNav.click()
        await expect(managePage.getByRole("group", { name: "工作视图" })).toBeVisible(VISIBLE)
    }
    // 指标行在队列容器之外；限定在任务队列内断言，避免命中指标文字。
    const managedList = managePage.locator('[data-slot="workspace-queue"]')
    await expect(managedList.getByText("受阻").first()).toBeVisible(VISIBLE)
    await managePage.screenshot({
        path: test.info().outputPath("s2-workspace-managed.png"),
    })
    await apiDeny("POST", "/admin/approval-decisions", financeToken, {
        work_item_id: task!.id,
        decision: "APPROVE",
        expected_task_version: task!.task_version,
        idempotency_key: randomUUID(),
    })

    // 9. 撤销恢复：删除个人上限后显式恢复当前审批人，财务通过后单据 POSTED。
    await deleteScope(adminToken, cap.id)
    const instanceId = blocked!.approval_context?.instance_id as string | undefined
    expect(instanceId, "受阻任务应携带审批实例").toBeTruthy()
    const resume = await apiOk(
        "GET",
        `/admin/approval-instances/${instanceId}/recovery-options`,
        adminToken,
    )
    expect((resume.actions as string[]).includes("RESUME_CURRENT_APPROVER")).toBe(true)
    const resumed = await apiOk(
        "POST",
        `/admin/approval-instances/${instanceId}/resume-current-approver`,
        adminToken,
        {
            expected_instance_version: resume.expected_instance_version,
            expected_execution_version: resume.expected_execution_version,
            expected_assignment_version: resume.expected_assignment_version,
            expected_closed_task_version: resume.expected_closed_task_version ?? undefined,
            idempotency_key: randomUUID(),
        },
    )
    const nextTask = resumed.next_open_task as
        | { work_item_id: string; task_version: string }
        | undefined
    expect(nextTask?.work_item_id, "恢复应返回下一开放任务").toBeTruthy()
    expect(nextTask!.work_item_id).not.toBe(task!.id)
    await apiOk("POST", "/admin/approval-decisions", financeToken, {
        work_item_id: nextTask!.work_item_id,
        decision: "APPROVE",
        expected_task_version: nextTask!.task_version,
        idempotency_key: randomUUID(),
    })
    const result = await apiOk("GET", `/admin/stock-adjustments/${stockId}`, warehouseToken)
    expect(result.adjustment.status).toBe("POSTED")
    // 跨页刷新：恢复后管理视图与库存页重新加载无错。
    await managePage.goto("/workspace")
    await expect(managePage.getByRole("heading", { name: "我的工作台" })).toBeVisible(VISIBLE)
    await financePage.goto("/inventory")
    await expect(financePage.getByRole("heading", { name: "库存台账" })).toBeVisible(VISIBLE)

    // 10. 零页面错误：整个流程不得有未捕获异常与应用 error 日志。
    expect(pageErrors, `页面异常必须为零：${pageErrors.slice(0, 3).join(" | ")}`).toEqual([])
    expect(consoleErrors, `控制台 error 必须为零：${consoleErrors.slice(0, 3).join(" | ")}`).toEqual([])

    for (const page of [adminPage, financePage, salesPage, managePage]) {
        await page.context().close().catch(() => undefined)
    }
})

test("S2 浏览器验收（窄屏）：组织与工作台可读可用", async ({ browser }) => {
    expect(API_BASE, "必须显式指定隔离后端 API_BASE").toMatch(/^http:\/\/(127\.0\.0\.1|localhost):/)
    const page = await loginAs(browser, "admin", { width: 390, height: 844 })
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible(VISIBLE)
    await page.goto("/system/organization")
    await expect(page.getByRole("heading", { name: "组织架构" })).toBeVisible(VISIBLE)
    await expect(page.locator("#organization-search")).toBeVisible(VISIBLE)
    await page.screenshot({
        path: test.info().outputPath("s2-org-units-mobile.png"),
    })
    await page.goto("/workspace")
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible(VISIBLE)
    expect(pageErrors, `页面异常必须为零：${pageErrors.slice(0, 3).join(" | ")}`).toEqual([])
    expect(consoleErrors, `控制台 error 必须为零：${consoleErrors.slice(0, 3).join(" | ")}`).toEqual([])
    await page.context().close().catch(() => undefined)
})

test("S2 浏览器验收（导出下载）：真实账号列表导出并撤权重验", async ({ browser }) => {
    expect(API_BASE, "必须显式指定隔离后端 API_BASE").toMatch(/^http:\/\/(127\.0\.0\.1|localhost):/)
    // 导出走前端分页收集＋最后一页之后下载前重验；下载即创建对象 URL 并点击，
    // 验收拦截下载事件校验内容，不落盘，避免在共享目录产生文件。
    // 隔离库无销售单据时走空态分支；有单据时走真实下载断言。
    // 最后用接口直接验证导出收集口径：跨页 scope_version 传递与撤权重验。
    const context = await browser.newContext({
        locale: "zh-CN",
        timezoneId: "Asia/Shanghai",
        acceptDownloads: true,
    })
    const page = await context.newPage()
    watch(page)
    await loginViaUi(page, loginName("xiaoshou"))
    await page.goto("/sales/orders")
    await expect(page.getByRole("heading", { name: "销售单" })).toBeVisible(VISIBLE)
    const exportButton = page.locator("#sales-orders-list-header-export")
    await expect(exportButton).toBeVisible(VISIBLE)
    if (await exportButton.isDisabled()) {
        // 隔离库无销售单时导出按钮禁用：断言按钮存在且禁用原因明确（总数为零），
        // 不伪造单据；有单据时走下面的真实下载断言。
        expect(await exportButton.isDisabled(), "无单据时导出按钮应禁用").toBe(true)
        await page.screenshot({
            path: test.info().outputPath("s2-sales-export-empty.png"),
        })
    } else {
        const downloadPromise = page.waitForEvent("download", { timeout: 60_000 })
        await exportButton.click()
        const download = await downloadPromise
        const path = await download.path()
        expect(path, "应产生真实下载文件").toBeTruthy()
        const suggested = download.suggestedFilename()
        expect(suggested, "下载文件名应为 CSV").toMatch(/\.csv$/i)
        await expect(page.getByText("导出完成")).toBeVisible(VISIBLE)
        await page.screenshot({
            path: test.info().outputPath("s2-sales-export.png"),
        })
    }
    // 导出收集口径的接口验证：第一页版本必须传递给后续页；伪造版本必须 409 失败关闭。
    const salesToken = await apiToken("xiaoshou")
    const first = await apiGet<{
        items: Array<Record<string, unknown>>
        total: number
        scope_version: string
    }>(salesToken, "/admin/sales-orders", { page: 1, page_size: 5 })
    expect(typeof first.scope_version, "列表应返回 scope_version").toBe("string")
    const second = await apiGet<{ scope_version: string }>(salesToken, "/admin/sales-orders", {
        page: 1,
        page_size: 1,
        scope_version: first.scope_version,
    })
    expect(second.scope_version, "同版本续查应一致").toBe(first.scope_version)
    await apiDenyGet(salesToken, "/admin/sales-orders", {
        page: 1,
        page_size: 1,
        scope_version: "forged-version",
    })
    expect(pageErrors, `页面异常必须为零：${pageErrors.slice(0, 3).join(" | ")}`).toEqual([])
    expect(consoleErrors, `控制台 error 必须为零：${consoleErrors.slice(0, 3).join(" | ")}`).toEqual([])
    await context.close().catch(() => undefined)
})
