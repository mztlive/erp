/**
 * S2 组织与人员授权浏览器验收：真实账号密码登录真实 HTTP，覆盖组织/成员变更、
 * 人员管理委派、跨页刷新、仓库维度隔离与审批资格撤销后的原任务续办。
 *
 * 环境（全部隔离，不碰共享开发库与生产库）：
 * - 后端：独立 shard 的 web-api、配置、端口与临时副本集数据库。
 * - 前端：当前生产构建，浏览器真实 API 请求转发到上述 shard。
 * - 数据：开发库只读复制，再补齐固定岗位、仓库、商品、审批定义。
 * - 运行：先 bash scripts/ensure-services.sh，再
 *   python3 scripts/run-e2e-parallel.py e2e/tests/s2-org-data-scope-browser.spec.ts。
 *
 * 断言边界：合成验收单据不等同生产数据量与完整业务流程；只核销 S2 §8.3 的
 * “浏览器真实账号验收”一行，不替代生产规模与代表性业务数据验收。
 */
import { randomUUID } from "node:crypto"

import { expect, test, type Browser, type Page } from "../helpers/test"

import { ACCOUNTS } from "../helpers/accounts"
import { API_BASE, apiGet, apiToken } from "../helpers/api"
import { analyticsDate, uploadAnalyticsEvidence } from "../helpers/financial-analytics"
import { ensureZeroBalanceDimension } from "../helpers/inventory"
import { loginViaUi } from "../helpers/login"
import { ensurePersonWarehouseScope, type PersonScopeView } from "../helpers/person-data-scope"
import { workPage } from "../helpers/workspace-page"

test.beforeEach(() => {
    expect(process.env.ERP_E2E_ISOLATED, "S2 必须通过隔离 runner 运行").toBe("1")
    expect(API_BASE, "必须显式指定隔离后端 API_BASE").toMatch(/^http:\/\/(127\.0\.0\.1|localhost):/)
    const sourceOrigin = new URL(process.env.ERP_E2E_SOURCE_API_BASE ?? "http://127.0.0.1:10001").origin
    expect(new URL(API_BASE).origin, "S2 不得使用前端原有 API 服务").not.toBe(sourceOrigin)
})

const VISIBLE = { timeout: 30_000 } as const
const RUN = `${Date.now().toString(36)}${Math.floor(Math.random() * 0xffff).toString(16)}`
const FINANCE_ORG = `S2浏览器财务-${RUN}`
const BIZ_ORG = `S2浏览器业务-${RUN}`
const SKU_NO = "TEA-SF-LJ-250"
const WAREHOUSE_CODE = "BJ-TZ-01"

const pageErrors: string[] = []
const consoleErrors: string[] = []
const temporaryRoleCleanups: Array<() => Promise<void>> = []

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

async function apiCall(
    method: string,
    path: string,
    token: string,
    body?: unknown,
): Promise<{ status: number; parsed: { success?: boolean; errorMessage?: string; code?: string; data?: any } }> {
    const res = await fetch(`${API_BASE}${path}`, {
        method,
        headers: {
            ...(token ? { Authorization: `Bearer ${token}` } : {}),
            ...(body === undefined ? {} : { "Content-Type": "application/json" }),
        },
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: AbortSignal.timeout(15_000),
    })
    const parsed = (await res.json()) as {
        success?: boolean
        errorMessage?: string
        code?: string
        data?: any
    }
    return { status: res.status, parsed }
}

async function apiGetCall(
    token: string,
    path: string,
    query?: Record<string, unknown>,
): Promise<{ status: number; parsed: { success?: boolean; errorMessage?: string; code?: string; data?: any } }> {
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

function cleanTemporaryRoleAfterTest(
    token: string,
    userId: string,
    roleId: string,
    originalRoleIds: string[],
): void {
    temporaryRoleCleanups.push(async () => {
        await apiOk("PUT", `/admin/admins/${userId}/role`, token, { role_ids: originalRoleIds })
        await apiOk("DELETE", `/admin/roles/${roleId}`, token)
    })
}

test.afterEach(async () => {
    pageErrors.length = 0
    consoleErrors.length = 0
    const failures: unknown[] = []
    for (const cleanup of temporaryRoleCleanups.splice(0).reverse()) {
        try {
            await cleanup()
        } catch (error) {
            failures.push(error)
        }
    }
    if (failures.length) throw new AggregateError(failures, "S2 临时角色恢复或删除失败")
})

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

type AdminAccount = { id: string; account: string; role_ids: string[] }
type AdminRole = { id: string; permissions: string[] }

async function adminAccountOf(token: string, account: string): Promise<AdminAccount> {
    const rows = await apiGet<AdminAccount[]>(token, "/admin/admins")
    const found = rows.find((row) => row.account === account)
    if (!found) throw new Error(`种子账号缺失 ${account}`)
    return found
}

test("S2 浏览器验收：组织变更跨页可见、维度隔离、审批撤权与续办", async ({ browser }) => {
    // 1. 真实密码登录：管理员走登录页，token 全部经真实登录接口取得。
    const adminPage = await loginAs(browser, "admin")
    const adminToken = await apiToken("admin")
    const financeAccount = await adminAccountOf(adminToken, loginName("caiwu"))
    const financeId = financeAccount.id
    const warehouseId = (await adminAccountOf(adminToken, loginName("cangchu"))).id
    const salesId = (await adminAccountOf(adminToken, loginName("xiaoshou"))).id
    const manageAccount = await adminAccountOf(adminToken, loginName("guanli"))
    const manageId = manageAccount.id
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
    // 管理监督独立配置人员治理范围；组织成员关系不授予管理或审批决定资格。
    const manageScope = await apiGet<PersonScopeView>(
        adminToken,
        `/admin/person-data-scopes/${manageId}`,
    )
    await apiOk("PUT", `/admin/person-data-scopes/${manageId}`, adminToken, {
        resource: "work_item",
        actions: ["manage"],
        grants: [{
            actions: ["manage"],
            terms: [{
                scope_type: "organization",
                target_dimension: "internal_org",
                scope_targets: [financeOrg!],
                target_mode: "explicit",
                include_descendants: false,
            }],
        }],
        replace_legacy: true,
        expected_policy_version: manageScope.policy_version,
    })

    // 3. 按实际经办人配置库存仓库范围；审批读取由精确流程参与关系授予。
    const warehouseToken = await apiToken("cangchu")
    const warehouseList = await apiGet<{ items: Array<{ id: string; warehouse_code: string }> }>(
        warehouseToken,
        "/admin/warehouses",
        { page: 1, page_size: 100 },
    )
    const warehouse = warehouseList.items.find((row) => row.warehouse_code === WAREHOUSE_CODE)
    expect(warehouse, `未找到仓库 ${WAREHOUSE_CODE}`).toBeTruthy()
    await ensurePersonWarehouseScope(adminToken, warehouseId, [
        { resource: "stock_balance", actions: ["list", "detail"] },
        { resource: "stock_movement", actions: ["list"] },
        { resource: "stock_reservation", actions: ["list"] },
        { resource: "stock_adjustment", actions: ["list", "detail", "create", "update", "submit"] },
    ], warehouse!.id)
    for (const userId of [financeId, manageId]) {
        await ensurePersonWarehouseScope(adminToken, userId, [
            { resource: "stock_adjustment", actions: ["list", "detail"] },
        ], warehouse!.id)
    }
    const skuList = await apiGet<{ items: Array<{ id: string; sku_no: string }> }>(
        warehouseToken,
        "/admin/skus",
        { q: SKU_NO, page: 1, page_size: 20 },
    )
    const sku = skuList.items.find((row) => row.sku_no === SKU_NO)
    expect(sku, `未找到固定库存验收 SKU ${SKU_NO}`).toBeTruthy()
    // 每个隔离 shard 独立准备初始库存维度，不依赖其他 spec 先创建余额。
    await ensureZeroBalanceDimension(WAREHOUSE_CODE, SKU_NO)
    const balanceList = await apiGet<{
        items: Array<{
            id: string
            warehouse_id: string
            sku_id: string
            version: string
            on_hand_quantity: string
            reserved_quantity: string
            available_quantity: string
        }>
    }>(warehouseToken, "/admin/stock-balances", { page: 1, page_size: 100 })
    const balance = balanceList.items.find(
        (row) => row.warehouse_id === warehouse!.id && row.sku_id === sku!.id,
    )
    expect(balance, "未找到零数量占位余额，请先准备库存维度").toBeTruthy()
    for (const field of ["on_hand_quantity", "reserved_quantity", "available_quantity"] as const) {
        expect(balance![field], `库存验收初始 ${field} 必须为零`).toMatch(/^0(?:\.0+)?$/)
    }

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
    expect(task!.processing_state).toBe("READY")
    expect(task!.allowed_actions).toEqual(expect.arrayContaining(["APPROVE", "REJECT"]))
    // 审批监督还要求类型运行管理资格和真实库存来源范围；部门管理范围不能补齐类型资格。
    const managementRoles = await apiGet<AdminRole[]>(adminToken, "/admin/roles")
    const managerPermissions = [...new Set(managementRoles
        .filter((row) => manageAccount.role_ids.includes(row.id))
        .flatMap((row) => row.permissions))]
    for (const permission of ["approval_instance:read", "stock_adjustment:detail", "work_item:manage"]) {
        expect(managerPermissions, `经理原角色应具备 ${permission}`).toContain(permission)
    }
    expect(managerPermissions.some((permission) => ["approval_instance:decide", "approval_instance:*", "*:*"].includes(permission)),
        "监督账号不得具备审批决定权限").toBe(false)
    const managementRuntimeRoleId = await apiOk("POST", "/admin/roles", adminToken, {
        name: `S2库存审批监督-${RUN}`,
        permissions: ["approval_instance:read", "stock_adjustment:approval_runtime_admin"],
    }) as string
    expect(typeof managementRuntimeRoleId, "临时库存审批监督角色应创建成功").toBe("string")
    cleanTemporaryRoleAfterTest(adminToken, manageId, managementRuntimeRoleId, manageAccount.role_ids)
    await apiOk("PUT", `/admin/admins/${manageId}/role`, adminToken, {
        role_ids: [...manageAccount.role_ids, managementRuntimeRoleId],
    })
    // 管理监督可见但不能代办：部门委派、类型资格、仓库来源范围齐备，审批决定仍必须拒绝。
    const managerToken = await apiToken("guanli")
    const managedQueue = await apiGet<{ items: Array<Record<string, any>> }>(
        managerToken,
        "/admin/work-items",
        { scope: "managed", page: 1, page_size: 100 },
    )
    expect(
        (managedQueue.items ?? []).some((row) => row.id === task!.id),
        "经理在部门委派和库存审批管理资格范围内可监督",
    ).toBe(true)
    const managerDecision = await apiCall("POST", "/admin/approval-decisions", managerToken, {
        work_item_id: task!.id,
        decision: "APPROVE",
        expected_task_version: task!.task_version,
        idempotency_key: randomUUID(),
    })
    expect(managerDecision.status, "库存审批监督资格不得授予代办决定权").toBe(403)

    // 5. 组织页跨页可见：刷新后新部门可见。
    await adminPage.goto("/system/organization")
    await expect(workPage(adminPage).getByRole("heading", { name: "组织与人员" })).toBeVisible(VISIBLE)
    const financeOrgButton = workPage(adminPage).getByRole("button", { name: FINANCE_ORG })
    await financeOrgButton.scrollIntoViewIfNeeded()
    await expect(financeOrgButton).toBeVisible(VISIBLE)
    await adminPage.screenshot({
        path: test.info().outputPath("s2-org-units.png"),
    })

    // 6. 人员范围入口可见：按人员配置数据范围，不提供角色主体范围编辑。
    await adminPage.goto("/system/organization/scopes")
    await expect(workPage(adminPage).getByRole("heading", { name: "人员数据范围" })).toBeVisible(VISIBLE)
    await expect(workPage(adminPage).locator("#person-scope-accounts-entry")).toBeVisible(VISIBLE)

    // 7. 客户跨页一致：组织筛选页正常加载。
    const salesPage = await loginAs(browser, "xiaoshou")
    await salesPage.goto("/sales/customers")
    await expect(workPage(salesPage).getByRole("heading", { name: "客户中心" })).toBeVisible(VISIBLE)

    // 8. 仓库维度隔离：内部部门不能保存为库存余额授权，也不能改变已配置的仓库范围。
    const financePage = await loginAs(browser, "caiwu")
    await financePage.goto("/inventory")
    await expect(workPage(financePage).getByRole("heading", { name: "库存台账" })).toBeVisible(VISIBLE)
    const warehouseScope = await apiGet<PersonScopeView>(
        adminToken,
        `/admin/person-data-scopes/${warehouseId}`,
    )
    const invalidWarehouseGrant = await apiCall(
        "PUT",
        `/admin/person-data-scopes/${warehouseId}`,
        adminToken,
        {
            resource: "stock_balance",
            actions: ["list"],
            grants: [{
                actions: ["list"],
                terms: [{
                    scope_type: "organization",
                    target_dimension: "internal_org",
                    scope_targets: [financeOrg!],
                    target_mode: "explicit",
                    include_descendants: false,
                }],
            }],
            replace_legacy: true,
            expected_policy_version: warehouseScope.policy_version,
        },
    )
    await test.info().attach("s2-invalid-warehouse-grant-response", {
        body: JSON.stringify(invalidWarehouseGrant),
        contentType: "application/json",
    })
    expect(invalidWarehouseGrant.status, "库存余额必须以请求校验错误拒绝缺失仓库维度的内部部门授权").toBe(400)
    expect(invalidWarehouseGrant.parsed.success).toBe(false)
    expect(invalidWarehouseGrant.parsed.code).toBe("INVALID_REQUEST")
    expect(invalidWarehouseGrant.parsed.errorMessage).toContain("所选操作不支持按内部组织设置数据范围")
    const unchangedBalances = await apiGet<{ items: Array<{ id: string }> }>(
        warehouseToken,
        "/admin/stock-balances",
        { page: 1, page_size: 100 },
    )
    expect(unchangedBalances.items.some((row) => row.id === balance!.id), "非法维度不得覆盖有效仓库授权").toBe(true)
    // 财务参与库存审批不授予库存余额业务范围；精确审批读取与普通列表权限分别校验。
    const financeToken = await apiToken("caiwu")
    const balanceResult = await apiGetCall(financeToken, "/admin/stock-balances", {
        page: 1,
        page_size: 5,
    })
    const balanceItems = balanceResult.parsed.data?.items as unknown[] | undefined
    const balanceDenied = balanceResult.status === 403
    const balanceEmpty = balanceResult.status === 200 && balanceResult.parsed.success !== false &&
        Array.isArray(balanceItems) && balanceItems.length === 0
    expect(
        balanceDenied || balanceEmpty,
        `审批参与关系不得打开无业务范围的仓库余额（HTTP ${balanceResult.status} items=${balanceItems?.length}）`,
    ).toBe(true)
    // 审批不接受独立人员范围；只撤读取资格，保留决定权限以验收写时重验。
    const financeScopes = await apiGet<{
        businesses: Array<{ resource: string; policy_description: string; configurable_actions: string[] }>
    }>(adminToken, `/admin/person-data-scopes/${financeId}`)
    const approvalPolicy = financeScopes.businesses.find((row) => row.resource === "approval_instance")
    expect(approvalPolicy?.policy_description, "审批必须采用任务参与授权").toContain("流程任务")
    expect(approvalPolicy?.configurable_actions, "审批不得配置独立数据范围").toEqual([])
    const roles = await apiGet<AdminRole[]>(adminToken, "/admin/roles")
    const originalPermissions = [...new Set(roles
        .filter((row) => financeAccount.role_ids.includes(row.id))
        .flatMap((row) => row.permissions))]
    expect(originalPermissions, "财务原角色应具备审批读取资格").toContain("approval_instance:read")
    expect(originalPermissions, "财务原角色应具备审批决定资格").toContain("approval_instance:decide")
    const ineligibleRoleId = await apiOk("POST", "/admin/roles", adminToken, {
        name: `S2审批失资格-${RUN}`,
        permissions: originalPermissions.filter((permission) => permission !== "approval_instance:read"),
    }) as string
    expect(typeof ineligibleRoleId, "临时资格角色应创建成功").toBe("string")
    cleanTemporaryRoleAfterTest(adminToken, financeId, ineligibleRoleId, financeAccount.role_ids)
    await apiOk("PUT", `/admin/admins/${financeId}/role`, adminToken, {
        role_ids: [ineligibleRoleId],
    })
    const revokedProfile = await apiGet<{ permissions: string[] }>(financeToken, "/account/profile")
    expect(revokedProfile.permissions).not.toContain("approval_instance:read")
    expect(revokedProfile.permissions).toContain("approval_instance:decide")

    // 队列动态标记失资格，审批实例和原任务仍在运行。
    const blockedQueue = await apiGet<{ items: Array<Record<string, any>> }>(
        managerToken,
        "/admin/work-items",
        { scope: "managed", page: 1, page_size: 100 },
    )
    const blocked = blockedQueue.items.find((row) => row.id === task!.id)
    expect(blocked, "失资格任务应继续对经理可见").toMatchObject({
        status: "OPEN",
        task_version: task!.task_version,
        owner_user_id: financeId,
        owner_organization_id: warehouse!.id,
        processing_state: "EXECUTION_BLOCKED",
        processing_blocker: { code: "APPROVER_NOT_ELIGIBLE" },
        approval_context: { status: "RUNNING" },
        allowed_actions: ["VIEW"],
    })
    const revokedQueue = await apiGet<{ items: Array<Record<string, any>> }>(
        financeToken,
        "/admin/work-items",
        { scope: "mine", page: 1, page_size: 100 },
    )
    const revokedTask = revokedQueue.items.find((row) => row.id === task!.id)
    expect(revokedTask).toMatchObject({
        status: "OPEN",
        task_version: task!.task_version,
        processing_state: "EXECUTION_BLOCKED",
    })
    expect(revokedTask!.allowed_actions).toContain("VIEW")
    expect(revokedTask!.allowed_actions).not.toContain("APPROVE")
    expect(revokedTask!.allowed_actions).not.toContain("REJECT")
    const managePage = await loginAs(browser, "guanli")
    await managePage.goto("/workspace/tasks")
    await workPage(managePage).locator("#workspace-task-scope").click()
    await workPage(managePage).locator("#workspace-task-scope-option-managed").click()
    await expect(workPage(managePage).getByRole("heading", { name: "范围内待办", exact: true })).toBeVisible(VISIBLE)
    const managedTask = workPage(managePage).getByRole("region", { name: "任务队列" })
        .locator(`[data-row-id="${task!.id}"]`)
    await expect(managedTask.getByText("受阻", { exact: true })).toBeVisible(VISIBLE)
    await workPage(managePage).locator("#workspace-query-more").click()
    await workPage(managePage).getByRole("checkbox", { name: "仅看受阻", exact: true }).click()
    const filteredQueue = managePage.waitForResponse((response) => {
        const url = new URL(response.url())
        return response.request().method() === "GET" && url.pathname === "/admin/work-items" &&
            url.searchParams.get("scope") === "managed" &&
            url.searchParams.get("filters")?.includes('"blocked":true') === true
    })
    await workPage(managePage).locator("#workspace-query-more-apply").click()
    const filteredResponse = await filteredQueue
    expect(filteredResponse.ok()).toBe(true)
    const filtered = (await filteredResponse.json()).data.items as Array<Record<string, unknown>>
    expect(filtered.find((row) => row.id === task!.id)).toMatchObject({ processing_state: "EXECUTION_BLOCKED" })
    await expect(managedTask.getByText("受阻", { exact: true })).toBeVisible(VISIBLE)
    await managePage.screenshot({
        path: test.info().outputPath("s2-workspace-managed.png"),
    })

    const stockPath = `/admin/stock-adjustments/${stockId}`
    const balancePath = `/admin/stock-balances/${balance!.id}`
    const taskPath = `/admin/work-items/${task!.id}`
    const [beforeDecision, beforeBalance, beforeTask] = await Promise.all([
        apiOk("GET", stockPath, warehouseToken),
        apiOk("GET", balancePath, warehouseToken),
        apiOk("GET", taskPath, adminToken),
    ])
    expect(beforeDecision).toMatchObject({
        adjustment: { status: "IN_APPROVAL" },
        approval: {
            instance: { status: "RUNNING", current_task_id: task!.id, current_task_version: task!.task_version },
            recent_history: [{ execution_id: task!.approval_node_execution_id, result: "ACTIVE" }],
        },
        posted_movements: [],
    })
    expect(beforeTask).toMatchObject({ status: "OPEN", close_reason: null, completed_at: null, closed_at: null })
    const decisionBody = {
        work_item_id: task!.id,
        decision: "APPROVE",
        expected_task_version: task!.task_version,
        idempotency_key: randomUUID(),
    }
    const deniedDecision = await apiCall("POST", "/admin/approval-decisions", financeToken, decisionBody)
    expect(deniedDecision.status, "失去读取资格的审批决定必须直接返回403").toBe(403)
    expect(deniedDecision.parsed).toMatchObject({ success: false, code: "PERMISSION_DENIED" })
    const [afterDecision, afterBalance, afterTask] = await Promise.all([
        apiOk("GET", stockPath, warehouseToken),
        apiOk("GET", balancePath, warehouseToken),
        apiOk("GET", taskPath, adminToken),
    ])
    expect(afterDecision.approval.instance, "拒绝不得改变审批实例或当前执行及任务版本")
        .toEqual(beforeDecision.approval.instance)
    expect(afterDecision.approval.recent_history, "拒绝不得新增或改写审批历史")
        .toEqual(beforeDecision.approval.recent_history)
    expect(afterDecision.adjustment, "拒绝不得改写库存调整单").toEqual(beforeDecision.adjustment)
    expect(afterDecision.lines).toEqual(beforeDecision.lines)
    expect(afterDecision.posted_movements, "拒绝不得生成库存流水").toEqual([])
    expect(afterBalance, "拒绝不得改变库存余额、流水或预占").toEqual(beforeBalance)
    expect(afterTask, "拒绝不得关闭或改写原任务").toEqual(beforeTask)

    // 9. 恢复原角色后，同一任务、版本和决定命令可直接办理。
    await apiOk("PUT", `/admin/admins/${financeId}/role`, adminToken, {
        role_ids: financeAccount.role_ids,
    })
    const restoredQueue = await apiGet<{ items: Array<Record<string, any>> }>(
        financeToken,
        "/admin/work-items",
        { scope: "mine", page: 1, page_size: 100 },
    )
    expect(restoredQueue.items.filter((row) => row.business_object_id === stockId)).toMatchObject([{
        id: task!.id,
        status: "OPEN",
        task_version: task!.task_version,
        processing_state: "READY",
        processing_blocker: null,
        allowed_actions: expect.arrayContaining(["APPROVE", "REJECT"]),
    }])
    const approved = await apiOk("POST", "/admin/approval-decisions", financeToken, decisionBody)
    expect(approved).toMatchObject({
        instance_id: beforeDecision.approval.instance.id,
        instance_status: "APPROVED",
        outcome: "APPLIED",
        next_open_task: null,
    })
    const [result, completedTask] = await Promise.all([
        apiOk("GET", stockPath, warehouseToken),
        apiOk("GET", taskPath, financeToken),
    ])
    expect(result.adjustment.status).toBe("POSTED")
    expect(result.posted_movements).toMatchObject([{ source_document_id: stockId, movement_type: "STOCK_GAIN" }])
    expect(completedTask).toMatchObject({ id: task!.id, status: "COMPLETED", completed_by: financeId })
    // 跨页刷新：恢复后管理视图与库存页重新加载无错。
    await managePage.goto("/workspace/tasks?view=managed")
    await expect(workPage(managePage).getByRole("heading", { name: "范围内待办", exact: true })).toBeVisible(VISIBLE)
    await financePage.goto("/inventory")
    await expect(workPage(financePage).getByRole("heading", { name: "库存台账" })).toBeVisible(VISIBLE)

    // 10. 零页面错误：整个流程不得有未捕获异常与应用 error 日志。
    expect(pageErrors, `页面异常必须为零：${pageErrors.slice(0, 3).join(" | ")}`).toEqual([])
    expect(consoleErrors, `控制台 error 必须为零：${consoleErrors.slice(0, 3).join(" | ")}`).toEqual([])

    for (const page of [adminPage, financePage, salesPage, managePage]) {
        await page.context().close().catch(() => undefined)
    }
})

test("S2 浏览器验收（窄屏）：组织与工作台可读可用", async ({ browser }) => {
    const page = await loginAs(browser, "admin", { width: 390, height: 844 })
    await page.goto("/system/organization")
    await expect(workPage(page).getByRole("heading", { name: "组织与人员" })).toBeVisible(VISIBLE)
    await expect(workPage(page).getByRole("navigation", { name: "组织树" })).toBeVisible(VISIBLE)
    await page.screenshot({
        path: test.info().outputPath("s2-org-units-mobile.png"),
    })
    await page.goto("/workspace/tasks")
    await expect(workPage(page).getByRole("heading", { name: "待我处理", exact: true })).toBeVisible(VISIBLE)
    expect(pageErrors, `页面异常必须为零：${pageErrors.slice(0, 3).join(" | ")}`).toEqual([])
    expect(consoleErrors, `控制台 error 必须为零：${consoleErrors.slice(0, 3).join(" | ")}`).toEqual([])
    await page.context().close().catch(() => undefined)
})

async function createExportOrder(adminToken: string, salesToken: string, seller: AdminAccount): Promise<string> {
    const [companies, skus, customer, evidenceId] = await Promise.all([
        apiGet<{ items: Array<{ id: string; party_no: string }> }>(adminToken, "/admin/companies", {
            status: "active", page_size: 100,
        }),
        apiGet<{ items: Array<{ sku_id: string; sku_revision_id: string; sku_no: string }> }>(
            salesToken, "/admin/sellable-skus", { q: SKU_NO, page_size: 20 },
        ),
        apiOk("POST", "/admin/customer-profiles", salesToken, {
            legal_name: `S2销售导出客户-${RUN}`,
            default_payment_term_id: "POSTPAY_NET15",
            effective_from: analyticsDate(),
            change_reason: "S2 销售导出验收",
            idempotency_key: randomUUID(),
        }) as Promise<{ customer_id: string }>,
        uploadAnalyticsEvidence(salesToken),
    ])
    const company = companies.items.find((row) => row.party_no === "FSY")
    const sku = skus.items.find((row) => row.sku_no === SKU_NO)
    expect(company, "固定种子缺少我方公司主体").toBeTruthy()
    expect(sku, `固定种子缺少销售商品 ${SKU_NO}`).toBeTruthy()
    const orderNo = `S2-EXPORT-${RUN}`
    const created = await apiOk("POST", "/admin/sales-orders", salesToken, {
        order_no: orderNo,
        business_type: "GOODS_SERVICE",
        customer_id: customer.customer_id,
        intent: "SAVE_DRAFT",
        evidence_file_asset_ids: [evidenceId],
        idempotency_key: randomUUID(),
        draft: {
            editor_user_id: seller.id,
            no_contract_terms: {
                settlement_party_id: company!.id,
                payment_term_code: "POSTPAY_NET15",
                payment_term_name: "货到 15 天",
                invoice_type: "增值税专用发票",
                tax_point: "0",
            },
            lines: [{
                line_no: 1,
                line_type: "GOODS_SERVICE",
                sales_tax_rate: "0",
                item_name_snapshot: "狮峰明前龙井礼盒 250g",
                spec_snapshot: "250g/盒",
                unit_snapshot: "盒",
                goods: {
                    sku_id: sku!.sku_id,
                    sku_revision_id: sku!.sku_revision_id,
                    fulfillment_due_at: Math.floor(Date.now() / 1000) + 30 * 86400,
                    quantity: "1",
                    base_unit_code: "HE",
                    unit_price_gross: "100.00",
                    pricing_mode: "MANUAL",
                },
            }],
        },
    }) as { order_no: string; commercial_status: string }
    expect(created).toMatchObject({ order_no: orderNo, commercial_status: "DRAFT" })
    return orderNo
}

test("S2 浏览器验收（导出下载）：真实账号列表导出并撤权重验", async ({ browser }) => {
    const context = await browser.newContext({
        locale: "zh-CN",
        timezoneId: "Asia/Shanghai",
        acceptDownloads: true,
    })
    const page = await context.newPage()
    watch(page)
    await loginViaUi(page, loginName("xiaoshou"))
    const adminToken = await apiToken("admin")
    const salesToken = await apiToken("xiaoshou")
    const seller = await adminAccountOf(adminToken, loginName("xiaoshou"))
    const orderNo = await createExportOrder(adminToken, salesToken, seller)
    await page.goto(`/sales/orders?q=${encodeURIComponent(orderNo)}`)
    const orders = workPage(page)
    await expect(orders.getByRole("heading", { name: "销售单" })).toBeVisible(VISIBLE)
    await expect(orders.getByText(orderNo, { exact: true })).toBeVisible(VISIBLE)
    const exportButton = orders.locator("#sales-orders-list-header-export")
    await expect(exportButton).toBeEnabled(VISIBLE)
    const [reply, download] = await Promise.all([
        page.waitForResponse((response) =>
            response.request().method() === "GET" &&
            /^\/account\/data-transfers\/[^/]+\/result$/.test(new URL(response.url()).pathname),
            { timeout: 90_000 },
        ),
        page.waitForEvent("download", { timeout: 90_000 }),
        exportButton.click(),
    ])
    expect(reply.status()).toBe(200)
    const resultPath = new URL(reply.url()).pathname
    expect(download.suggestedFilename()).toMatch(/\.csv$/i)
    const stream = await download.createReadStream()
    const chunks: Buffer[] = []
    for await (const chunk of stream) chunks.push(chunk)
    const csv = Buffer.concat(chunks).toString("utf8")
    const [header, ...rows] = csv.trim().split(/\r?\n/)
    expect(header).toContain('"销售单号"')
    expect(rows).toHaveLength(1)
    expect(rows[0]).toContain(`"${orderNo}"`)
    await expect(orders.getByText("导出完成", { exact: true })).toBeVisible(VISIBLE)
    await page.screenshot({ path: test.info().outputPath("s2-sales-export.png") })

    const first = await apiGet<{ scope_version: string }>(salesToken, "/admin/sales-orders", { page: 1, page_size: 1 })
    expect(typeof first.scope_version, "列表应返回 scope_version").toBe("string")
    const second = await apiGet<{ scope_version: string }>(salesToken, "/admin/sales-orders", {
        page: 2,
        page_size: 1,
        scope_version: first.scope_version,
    })
    expect(second.scope_version, "同版本续查应一致").toBe(first.scope_version)
    const forgedVersion = await apiGetCall(salesToken, "/admin/sales-orders", {
        page: 2,
        page_size: 1,
        scope_version: "forged-version",
    })
    expect(forgedVersion.status, "伪造范围版本必须409失败关闭").toBe(409)
    expect(forgedVersion.parsed.code).toBe("DATA_SCOPE_CHANGED")
    const restrictedRoleId = await apiOk("POST", "/admin/roles", adminToken, {
        name: `S2销售导出撤权-${RUN}`,
        permissions: ["customer:list"],
    }) as string
    cleanTemporaryRoleAfterTest(adminToken, seller.id, restrictedRoleId, seller.role_ids)
    await apiOk("PUT", `/admin/admins/${seller.id}/role`, adminToken, { role_ids: [restrictedRoleId] })
    const listDenied = await apiGetCall(salesToken, "/admin/sales-orders")
    expect(listDenied.status).toBe(403)
    expect(listDenied.parsed.code).toBe("PERMISSION_DENIED")
    const downloadDenied = await apiGetCall(salesToken, resultPath)
    expect(downloadDenied.status).toBe(403)
    expect(downloadDenied.parsed.code).toBe("PERMISSION_DENIED")
    expect(pageErrors, `页面异常必须为零：${pageErrors.slice(0, 3).join(" | ")}`).toEqual([])
    expect(consoleErrors, `控制台 error 必须为零：${consoleErrors.slice(0, 3).join(" | ")}`).toEqual([])
    await context.close().catch(() => undefined)
})
