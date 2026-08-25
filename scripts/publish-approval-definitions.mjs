#!/usr/bin/env node
/**
 * 审批流程定义发布脚本（E2E 前置步骤，幂等）。
 *
 * 数据库 reset 会删除全部审批定义（approval_process_definitions 等），
 * 按合同（approval-workflow-contract.md §4.3/§4.4）每个 PROCESS_REQUIRED 类型
 * 必须先创建并发布定义，单据才能进入审批；否则创建返回 APPROVAL_PROCESS_NOT_CONFIGURED。
 *
 * 本脚本以 admin 登录，为下列类型创建「空源草稿 + 线性单人节点」并发布：
 *   sales_order / voucher_sales_order / sales_change_order / purchase_order /
 *   purchase_change_order / stock_adjustment / customer_receipt / supplier_payment /
 *   customer_refund / supplier_refund / receipt_reversal / payment_reversal
 *
 * 审批人选择约束（代码事实）：
 *   - 审批人账号必须 active 且具备 approval_instance:decide（全部角色都有）；
 *   - 主体读取校验按类型实现但当前均放行（organization/assignee 非空即可）；
 *   - 岗位分离：提交人不得审批自己的单据（ForbidSubmitterAsApprover）。
 * 默认审批人按部门职责分配，且与常见提交人（销售/采购/财务）不同。
 *
 * 幂等策略：
 *   - 已发布 -> 跳过；
 *   - 存在草稿（上次失败残留）-> 复用该草稿继续编辑/发布；
 *   - 无草稿 -> 新建。
 *
 * 用法: node scripts/publish-approval-definitions.mjs
 * 环境变量: API_BASE（默认 http://127.0.0.1:10001）
 */
const API_BASE = process.env.API_BASE || "http://127.0.0.1:10001"

const DEFINITIONS = [
    { type: "sales_order", name: "销售单审批（E2E）", nodes: [
        { node_name: "采购确认", display_order: 1, assignee: "procurement" },
    ]},
    { type: "voucher_sales_order", name: "卡券销售单审批（E2E）", nodes: [
        { node_name: "销售总监审批", display_order: 1, assignee: "salesLeader" },
        { node_name: "运营审批", display_order: 2, assignee: "operations" },
        { node_name: "财务审批", display_order: 3, assignee: "finance" },
    ]},
    { type: "sales_change_order", name: "销售变更单审批（E2E）", nodes: [
        { node_name: "采购确认", display_order: 1, assignee: "procurement" },
        { node_name: "销售领导复核", display_order: 2, assignee: "salesLeader" },
    ]},
    { type: "purchase_order", name: "采购单审批（E2E）", nodes: [
        { node_name: "财务审核", display_order: 1, assignee: "finance" },
    ]},
    { type: "purchase_change_order", name: "采购变更单审批（E2E）", nodes: [
        { node_name: "财务复核", display_order: 1, assignee: "finance" },
    ]},
    { type: "stock_adjustment", name: "库存调整单审批（E2E）", nodes: [
        { node_name: "财务审批", display_order: 1, assignee: "finance" },
    ]},
    { type: "customer_receipt", name: "客户回款单审批（E2E）", nodes: [
        { node_name: "销售领导复核", display_order: 1, assignee: "salesLeader" },
    ]},
    { type: "supplier_payment", name: "供应商付款单审批（E2E）", nodes: [
        { node_name: "采购复核", display_order: 1, assignee: "procurement" },
    ]},
    { type: "customer_refund", name: "客户退款单审批（E2E）", nodes: [
        { node_name: "销售领导复核", display_order: 1, assignee: "salesLeader" },
    ]},
    { type: "supplier_refund", name: "供应商退款单审批（E2E）", nodes: [
        { node_name: "采购复核", display_order: 1, assignee: "procurement" },
    ]},
    { type: "receipt_reversal", name: "回款冲正单审批（E2E）", nodes: [
        { node_name: "销售领导复核", display_order: 1, assignee: "salesLeader" },
    ]},
    { type: "payment_reversal", name: "付款冲正单审批（E2E）", nodes: [
        { node_name: "采购复核", display_order: 1, assignee: "procurement" },
    ]},
]

const ACCOUNTS = {
    admin: { account: "admin", password: "123456" },
    sales: { account: "xiaoshou", password: "123456" },
    procurement: { account: "caigou", password: "123456" },
    operations: { account: "yunying", password: "123456" },
    finance: { account: "caiwu", password: "123456" },
    salesLeader: { account: "lisiyong", password: "123456" },
}

async function call(method, path, { token, body } = {}) {
    const headers = { "Content-Type": "application/json" }
    if (token) headers.Authorization = `Bearer ${token}`
    let res
    try {
        res = await fetch(`${API_BASE}${path}`, {
            method,
            headers,
            body: body === undefined ? undefined : JSON.stringify(body),
        })
    } catch (error) {
        throw new Error(`API ${method} ${path} 网络错误: ${error.message}`)
    }
    const text = await res.text()
    let parsed = null
    try {
        parsed = text ? JSON.parse(text) : null
    } catch {
        throw new Error(`API ${method} ${path} 返回非 JSON（HTTP ${res.status}）: ${text.slice(0, 300)}`)
    }
    if (res.status === 401 || (parsed && parsed.status === 401)) {
        throw new Error(`API ${method} ${path} 未授权`)
    }
    if (!res.ok || (parsed && parsed.success === false)) {
        throw new Error(
            `API ${method} ${path} 失败（HTTP ${res.status}）: ${parsed?.errorMessage ?? text}`,
        )
    }
    return parsed.data
}

async function login(account, password) {
    const data = await call("POST", "/login", {
        body: { account, password, account_kind: "admin" },
    })
    return data.token
}

async function findDraftId(adminToken, documentType) {
    const versions = await call(
        "GET",
        `/admin/approval-processes/${documentType}/versions`,
        { token: adminToken },
    )
    if (!Array.isArray(versions)) return null
    const draft = versions.find((v) => v && v.status === "DRAFT")
    return draft ? draft.definition_id : null
}

async function main() {
    const adminToken = await login(ACCOUNTS.admin.account, ACCOUNTS.admin.password)
    console.log("admin 登录成功")

    // 解析审批人账号 id（动态获取，避免硬编码漂移）
    const userIds = {}
    for (const [key, acc] of Object.entries(ACCOUNTS)) {
        const token = key === "admin" ? adminToken : await login(acc.account, acc.password)
        const profile = await call("GET", "/account/profile", { token })
        userIds[key] = profile.userid
    }
    console.log("账号 id:", JSON.stringify(userIds))

    const catalog = await call("GET", "/admin/approval-processes/catalog", { token: adminToken })
    const byType = new Map(catalog.map((row) => [row.document_type, row]))

    let created = 0
    let skipped = 0
    for (const def of DEFINITIONS) {
        const row = byType.get(def.type)
        if (!row) {
            console.warn(`跳过: 目录中不存在类型 ${def.type}`)
            continue
        }
        if (row.configuration_status === "PUBLISHED") {
            console.log(`跳过: ${def.type} 已有已发布定义（版本 ${row.published_version}）`)
            skipped += 1
            continue
        }

        // 复用上次失败残留的草稿，保证幂等
        let definitionId = await findDraftId(adminToken, def.type)
        let lockVersion
        if (definitionId) {
            const detail = await call(
                "GET",
                `/admin/approval-process-definitions/${definitionId}`,
                { token: adminToken },
            )
            lockVersion = detail.definition_lock_version
            console.log(`复用草稿: ${def.type}（${definitionId}，lock=${lockVersion}）`)
        } else {
            const draft = await call("POST", "/admin/approval-process-definitions/drafts", {
                token: adminToken,
                body: {
                    document_type: def.type,
                    name: def.name,
                    draft_source: "EMPTY",
                    idempotency_key: `e2e-${def.type}-${Date.now()}`,
                },
            })
            definitionId = draft.definition_id
            lockVersion = draft.definition_lock_version
            console.log(`新建草稿: ${def.type}（${definitionId}，lock=${lockVersion}）`)
        }

        const nodes = def.nodes.map((n) => ({
            node_name: n.node_name,
            display_order: n.display_order,
            assignee_user_id: userIds[n.assignee],
        }))
        // 注意：expected_definition_lock_version 必须是字符串（后端 422 已实测）
        const updated = await call(
            "PUT",
            `/admin/approval-process-definitions/${definitionId}/nodes`,
            {
                token: adminToken,
                body: { expected_definition_lock_version: String(lockVersion), nodes },
            },
        )
        lockVersion = updated.definition_lock_version

        await call(
            "POST",
            `/admin/approval-process-definitions/${definitionId}/publish`,
            {
                token: adminToken,
                body: {
                    expected_definition_lock_version: String(lockVersion),
                    idempotency_key: `e2e-${def.type}-publish-${Date.now()}`,
                },
            },
        )
        console.log(`已发布: ${def.type}（${def.nodes.map((n) => n.node_name).join(" → ")}）`)
        created += 1
    }
    console.log(`完成: 新建 ${created} 个定义，跳过 ${skipped} 个已存在定义`)
}

main().catch((error) => {
    console.error("发布审批定义失败:", error.message)
    process.exit(1)
})
