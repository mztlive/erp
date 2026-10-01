/**
 * S2 组织与人员授权浏览器验收：真实账号密码登录真实 HTTP，覆盖组织/成员变更、
 * 人员管理委派、跨页刷新、仓库维度隔离与审批失资格恢复。
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
import { ensureZeroBalanceDimension } from "../helpers/inventory"
import { loginViaUi } from "../helpers/login"
import { ensurePersonWarehouseScope, type PersonScopeView } from "../helpers/person-data-scope"

test.describe.configure({ mode: "serial" })

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

test("S2 浏览器验收：组织变更跨页可见、维度隔离、任务受阻", async ({ browser }) => {
    expect(API_BASE, "必须显式指定隔离后端 API_BASE").toMatch(/^http:\/\/(127\.0\.0\.1|localhost):/)
    // 1. 真实密码登录：管理员走登录页，token 全部经真实登录接口取得。
    const adminPage = await loginAs(browser, "admin")
    await expect(adminPage.getByRole("heading", { name: "我的工作台" })).toBeVisible(VISIBLE)
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
        permissions: ["stock_adjustment:approval_runtime_admin"],
    }) as string
    expect(typeof managementRuntimeRoleId, "临时库存审批监督角色应创建成功").toBe("string")
    cleanTemporaryRoleAfterTest(adminToken, manageId, managementRuntimeRoleId, manageAccount.role_ids)
    await apiOk("PUT", `/admin/admins/${manageId}/role`, adminToken, {
        role_ids: [...manageAccount.role_ids, managementRuntimeRoleId],
    })
    // 管理监督可见但不能代办：部门委派、类型资格、仓库来源范围齐备，审批决定仍必须拒绝。
    const managedQueue = await apiGet<{ items: Array<Record<string, any>> }>(
        await apiToken("guanli"),
        "/admin/work-items",
        { scope: "managed", page: 1, page_size: 100 },
    )
    expect(
        (managedQueue.items ?? []).some((row) => row.id === task!.id),
        "经理在部门委派和库存审批管理资格范围内可监督",
    ).toBe(true)
    const managerDecision = await apiCall("POST", "/admin/approval-decisions", await apiToken("guanli"), {
        work_item_id: task!.id,
        decision: "APPROVE",
        expected_task_version: task!.task_version,
        idempotency_key: randomUUID(),
    })
    expect(managerDecision.status, "库存审批监督资格不得授予代办决定权").toBe(403)

    // 5. 组织页跨页可见：刷新后新部门可见。
    await adminPage.goto("/system/organization")
    await expect(adminPage.getByRole("heading", { name: "组织与人员" })).toBeVisible(VISIBLE)
    const financeOrgButton = adminPage.getByRole("button", { name: FINANCE_ORG })
    await financeOrgButton.scrollIntoViewIfNeeded()
    await expect(financeOrgButton).toBeVisible(VISIBLE)
    await adminPage.screenshot({
        path: test.info().outputPath("s2-org-units.png"),
    })

    // 6. 人员范围入口可见：按人员配置数据范围，不提供角色主体范围编辑。
    await adminPage.goto("/system/organization/scopes")
    await expect(adminPage.getByRole("heading", { name: "人员数据范围" })).toBeVisible(VISIBLE)
    await expect(adminPage.locator("#person-scope-accounts-entry")).toBeVisible(VISIBLE)

    // 7. 客户跨页一致：组织筛选页正常加载。
    const salesPage = await loginAs(browser, "xiaoshou")
    await salesPage.goto("/sales/customers")
    await expect(salesPage.getByRole("heading", { name: "客户中心" })).toBeVisible(VISIBLE)

    // 8. 仓库维度隔离：内部部门不能保存为库存余额授权，也不能改变已配置的仓库范围。
    const financePage = await loginAs(browser, "caiwu")
    await financePage.goto("/inventory")
    await expect(financePage.getByRole("heading", { name: "库存台账" })).toBeVisible(VISIBLE)
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
    expect(invalidWarehouseGrant.parsed.errorMessage).toContain("每项附加授权均须配置此业务全部必需维度")
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
    // 审批不接受独立人员范围；撤去读取资格、保留决定入口以验收写时重验与受阻提交。
    const financeScopes = await apiGet<{
        businesses: Array<{ resource: string; authorization_policy: string; configurable_actions: string[] }>
    }>(adminToken, `/admin/person-data-scopes/${financeId}`)
    const approvalPolicy = financeScopes.businesses.find((row) => row.resource === "approval_instance")
    expect(approvalPolicy?.authorization_policy, "审批必须采用任务参与授权").toBe("task")
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
    // 浏览器管理视图必须在决定提交之前：写时重验失败会关闭原受阻任务
    //（原任务不可变，恢复必须建新任务），关单后管理队列不再列出该任务。
    const managePage = await loginAs(browser, "guanli")
    await managePage.goto("/workspace")
    await expect(managePage.getByRole("heading", { name: "我的工作台" })).toBeVisible(VISIBLE)
    const managedNav = managePage.locator("#workspace-queue-scope-managed")
    await expect(managedNav, "已委派管理范围必须提供范围内待办视图").toBeVisible(VISIBLE)
    await managedNav.click()
    await expect(managePage.getByRole("group", { name: "工作视图" })).toBeVisible(VISIBLE)
    // 指标行在队列容器之外；限定在任务队列内断言，避免命中指标文字。
    const managedList = managePage.locator('[data-slot="workspace-queue"]')
    await expect(managedList.getByText("受阻").first()).toBeVisible(VISIBLE)
    await managePage.screenshot({
        path: test.info().outputPath("s2-workspace-managed.png"),
    })
    const blockedDecision = await apiCall("POST", "/admin/approval-decisions", financeToken, {
        work_item_id: task!.id,
        decision: "APPROVE",
        expected_task_version: task!.task_version,
        idempotency_key: randomUUID(),
    })
    expect(blockedDecision.status, "失资格审批决定必须提交受阻事实后返回409").toBe(409)
    expect(blockedDecision.parsed.code).toBe("APPROVAL_INSTANCE_BLOCKED")

    // 9. 资格恢复：恢复原角色后显式恢复当前审批人，生成新任务，财务通过后单据 POSTED。
    await apiOk("PUT", `/admin/admins/${financeId}/role`, adminToken, {
        role_ids: financeAccount.role_ids,
    })
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
    await expect(page.getByRole("heading", { name: "组织与人员" })).toBeVisible(VISIBLE)
    await expect(page.getByRole("navigation", { name: "组织树" })).toBeVisible(VISIBLE)
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
    const forgedVersion = await apiGetCall(salesToken, "/admin/sales-orders", {
        page: 1,
        page_size: 1,
        scope_version: "forged-version",
    })
    expect(forgedVersion.status, "伪造范围版本必须409失败关闭").toBe(409)
    expect(forgedVersion.parsed.code).toBe("DATA_SCOPE_CHANGED")
    expect(pageErrors, `页面异常必须为零：${pageErrors.slice(0, 3).join(" | ")}`).toEqual([])
    expect(consoleErrors, `控制台 error 必须为零：${consoleErrors.slice(0, 3).join(" | ")}`).toEqual([])
    await context.close().catch(() => undefined)
})
