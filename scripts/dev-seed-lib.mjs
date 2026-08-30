#!/usr/bin/env node
/**
 * 开发种子共享库：HTTP 调用、岗位账号目录与幂等建号。
 *
 * 账号对齐 docs/erp-phase-1.md §11 部门职责与预定义角色（role-sales 等）。
 * 财务按岗位分离拆成总监 / 出纳 / 开票人三个账号，共用 role-finance。
 * 超级管理员 admin 不在此创建，由 CLI init-admin 或本入口的 reset-db.sh 修复。
 *
 * 用法: 由 seed-dev-foundation.mjs / publish-approval-definitions.mjs 导入
 */
export const API_BASE = process.env.API_BASE || "http://127.0.0.1:10001"

export const DEV_PASSWORD = "123456"

export const ADMIN = {
    account: "admin",
    password: DEV_PASSWORD,
    name: "系统管理员",
}

/**
 * 开发环境全部业务岗位账号。
 *
 * key 同时用于审批定义的 assignee 引用。roleId 必须是启动时写入的预定义角色。
 */
export const ACCOUNTS = {
    sales: {
        account: "xiaoshou",
        password: DEV_PASSWORD,
        name: "销售",
        roleId: "role-sales",
        label: "销售",
    },
    salesLeader: {
        account: "lisiyong",
        password: DEV_PASSWORD,
        name: "销售领导",
        roleId: "role-sales-leader",
        label: "销售领导",
    },
    procurement: {
        account: "caigou",
        password: DEV_PASSWORD,
        name: "采购",
        roleId: "role-procurement",
        label: "采购",
    },
    operations: {
        account: "yunying",
        password: DEV_PASSWORD,
        name: "运营",
        roleId: "role-operations",
        label: "运营",
    },
    warehouse: {
        account: "cangchu",
        password: DEV_PASSWORD,
        name: "仓储",
        roleId: "role-warehouse",
        label: "仓储",
    },
    finance: {
        account: "caiwu",
        password: DEV_PASSWORD,
        name: "财务总监",
        roleId: "role-finance",
        label: "财务总监",
    },
    payment: {
        account: "fukuan",
        password: DEV_PASSWORD,
        name: "出纳",
        roleId: "role-finance",
        label: "出纳",
    },
    invoice: {
        account: "kaipiao",
        password: DEV_PASSWORD,
        name: "开票人",
        roleId: "role-finance",
        label: "开票人",
    },
    management: {
        account: "guanli",
        password: DEV_PASSWORD,
        name: "管理层",
        roleId: "role-management",
        label: "管理层",
    },
    sysadmin: {
        account: "xitong",
        password: DEV_PASSWORD,
        name: "系统管理员",
        roleId: "role-sysadmin",
        label: "系统管理员",
    },
}

/**
 * 调用 web-api 并解析统一响应信封。
 *
 * @param {string} method HTTP 方法
 * @param {string} path 以 / 开头的路径
 * @param {{ token?: string, body?: unknown, form?: FormData }} [options]
 * @returns {Promise<any>} `data` 字段
 */
export async function call(method, path, { token, body, form } = {}) {
    const headers = {}
    if (token) headers.Authorization = `Bearer ${token}`
    if (body !== undefined) headers["Content-Type"] = "application/json"
    let res
    try {
        res = await fetch(`${API_BASE}${path}`, {
            method,
            headers,
            body: form ?? (body === undefined ? undefined : JSON.stringify(body)),
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
    if (res.status === 401 || parsed?.status === 401) {
        throw new Error(`API ${method} ${path} 未授权`)
    }
    if (!res.ok || parsed?.success === false) {
        throw new Error(
            `API ${method} ${path} 失败（HTTP ${res.status}）: ${parsed?.errorMessage ?? text}`,
        )
    }
    return parsed.data
}

/**
 * 以后台账号登录并返回 JWT。
 *
 * @param {string} account 登录账号
 * @param {string} password 明文密码
 * @returns {Promise<string>} token
 */
export async function login(account, password) {
    const data = await call("POST", "/login", {
        body: { account, password, account_kind: "admin" },
    })
    return data.token
}

/**
 * 列出全部后台账号。
 *
 * @param {string} adminToken 超级管理员 token
 * @returns {Promise<Array<{ id: string, account: string, name: string, role_ids: string[] }>>}
 */
export async function listAdmins(adminToken) {
    const rows = await call("GET", "/admin/admins", { token: adminToken })
    return Array.isArray(rows) ? rows : []
}

/**
 * 判断种子账号能否用开发密码登录。
 *
 * @param {{ account: string, password: string }} credentials
 * @returns {Promise<boolean>}
 */
async function accountCanLogin(credentials) {
    try {
        await login(credentials.account, credentials.password)
        return true
    } catch {
        return false
    }
}

/**
 * 读取预定义角色列表。
 *
 * @param {string} adminToken 超级管理员 token
 * @returns {Promise<Array<{ id: string, name: string }>>}
 */
async function listRoles(adminToken) {
    const roles = await call("GET", "/admin/roles", { token: adminToken })
    return Array.isArray(roles) ? roles : []
}

/**
 * 幂等创建或校正一个岗位账号：补角色、校正姓名，开发密码无法登录时重置密码。
 *
 * @param {string} adminToken 超级管理员 token
 * @param {{ account: string, password: string, name: string, roleId: string, label: string }} spec
 * @param {Array<{ id: string, name: string }>} [roles] 已读取的角色列表；缺省时现查
 * @param {{ checkPassword?: boolean }} [options] `checkPassword` 为 false 时不探测/重置开发密码
 * @returns {Promise<{ id: string, account: string, name: string, role_ids: string[] }>}
 */
export async function ensureRoleBoundAdmin(adminToken, spec, roles, options = {}) {
    const roleRows = roles ?? (await listRoles(adminToken))
    const role = roleRows.find((row) => row.id === spec.roleId)
    if (!role) {
        throw new Error(`未找到预定义角色 ${spec.roleId}，无法创建${spec.label}账号`)
    }

    let account = (await listAdmins(adminToken)).find((row) => row.account === spec.account)
    let created = false
    if (!account) {
        await call("POST", "/admin/admins", {
            token: adminToken,
            body: {
                account: spec.account,
                password: spec.password,
                name: spec.name,
                role_ids: [spec.roleId],
            },
        })
        account = (await listAdmins(adminToken)).find((row) => row.account === spec.account)
        if (!account) throw new Error(`${spec.label}账号已创建但列表中找不到 ${spec.account}`)
        created = true
        console.log(`${spec.label}账号已创建: ${spec.account}`)
    } else {
        console.log(`${spec.label}账号已存在: ${spec.account}`)
    }

    if (account.name !== spec.name) {
        await call("PUT", `/admin/admins/${encodeURIComponent(account.id)}`, {
            token: adminToken,
            body: { name: spec.name },
        })
        account = { ...account, name: spec.name }
        console.log(`已将 ${spec.account} 的姓名校正为${spec.name}`)
    }

    if (options.checkPassword !== false && !created && !(await accountCanLogin(spec))) {
        await call("PUT", `/admin/admins/${encodeURIComponent(account.id)}`, {
            token: adminToken,
            body: { password: spec.password },
        })
        console.log(`已将 ${spec.account} 的密码重置为开发密码`)
        if (!(await accountCanLogin(spec))) {
            throw new Error(`${spec.label}账号 ${spec.account} 仍无法用开发密码登录`)
        }
    }

    const roleIds = Array.isArray(account.role_ids) ? account.role_ids : []
    if (!roleIds.includes(spec.roleId)) {
        await call("PUT", `/admin/admins/${encodeURIComponent(account.id)}/role`, {
            token: adminToken,
            body: { role_ids: [...roleIds, spec.roleId] },
        })
        account = { ...account, role_ids: [...roleIds, spec.roleId] }
        console.log(`已为 ${spec.account} 补上${role.name}角色`)
    }
    return account
}

/**
 * 幂等补齐全部开发岗位账号，并校验财务三人、销售/采购等互不为同一人。
 *
 * @param {string} adminToken 超级管理员 token
 * @param {{ checkPassword?: boolean }} [options] 传给每个账号；审批发布只需身份，可关闭密码探测
 * @returns {Promise<Record<string, { id: string, account: string, name: string, role_ids: string[] }>>}
 */
export async function ensureDevAccounts(adminToken, options = {}) {
    const roles = await listRoles(adminToken)
    const seeded = {}
    for (const [key, spec] of Object.entries(ACCOUNTS)) {
        seeded[key] = await ensureRoleBoundAdmin(adminToken, spec, roles, options)
    }

    const financeIds = [seeded.finance.id, seeded.payment.id, seeded.invoice.id]
    if (new Set(financeIds).size !== 3) {
        throw new Error("财务总监、出纳和开票人必须是三个不同账号")
    }
    const businessIds = [
        seeded.sales.id,
        seeded.salesLeader.id,
        seeded.procurement.id,
        seeded.operations.id,
        seeded.warehouse.id,
    ]
    if (new Set(businessIds).size !== businessIds.length) {
        throw new Error("销售、销售领导、采购、运营、仓储必须是不同账号")
    }
    return seeded
}

/**
 * 打印开发登录目录，不含密码以外的密钥。
 */
export function printAccountDirectory() {
    console.log("登录账号（密码均为 123456）:")
    console.log("  admin        超级管理员")
    for (const spec of Object.values(ACCOUNTS)) {
        const pad = spec.account.padEnd(12, " ")
        console.log(`  ${pad}${spec.label}`)
    }
}
