#!/usr/bin/env node
/**
 * 开发开单底座：在业务数据已清空、web-api 已就绪后，补齐开单所需底座。
 *
 * 不写入销售单/采购单/库存/票款。供应商、商品与公司商品池由 seed-dev-catalog.mjs 补齐。
 * 仓库在主数据重置后由本脚本重建。
 * 付款与销项开票任务必须先有启用的财务责任规则，否则生产任务会失败关闭。
 * 财务三人分责：caiwu 仍只做审批；fukuan 为默认付款负责人；kaipiao 为默认开票负责人。
 * 审批定义由 publish-approval-definitions.mjs 单独发布，审批人仍是 caiwu。
 *
 * 幂等：客户、合同、仓储账号、仓库、财务三人、默认财务责任规则均按固定标识查找；
 * 已存在则跳过或只补绑定/校正。
 * 客户用 xiaoshou 创建（不含银行账户：销售无权维护该字段），负责销售一开始就是开单账号。
 * 银行账户由 admin 后补（财务有该字段权限，但客户资料修订还要 customer:update）。
 *
 * 用法: node scripts/seed-dev-foundation.mjs
 * 环境变量: API_BASE（默认 http://127.0.0.1:10001）
 */
const API_BASE = process.env.API_BASE || "http://127.0.0.1:10001"

const ACCOUNTS = {
    admin: { account: "admin", password: "123456" },
    sales: { account: "xiaoshou", password: "123456" },
    warehouse: { account: "cangchu", password: "123456", name: "仓储" },
    finance: { account: "caiwu", password: "123456", name: "财务" },
    payment: { account: "fukuan", password: "123456", name: "付款人" },
    invoice: { account: "kaipiao", password: "123456", name: "开票人" },
}

const ROLE_WAREHOUSE = "role-warehouse"
const ROLE_FINANCE = "role-finance"
const DEFAULT_FINANCE_RULES = [
    { operation: "SUPPLIER_PAYMENT", label: "默认付款负责人", accountKey: "payment" },
    { operation: "SALES_INVOICE", label: "默认开票负责人", accountKey: "invoice" },
]
const WAREHOUSES = [
    {
        code: "BJ-TZ-01",
        name: "北京通州仓",
        address: "北京市通州区马驹桥物流基地兴贸一街 6 号",
        contact: "周强",
    },
    {
        code: "SH-JD-01",
        name: "上海嘉定仓",
        address: "上海市嘉定区江桥镇金沙江南路 1358 号",
        contact: "吴婷",
    },
]

const CUSTOMER = {
    legalName: "华润置地（北京）有限公司",
    shortName: "华润置地北京",
    unifiedCreditCode: "91110105MA00CRBJ0X",
    contactName: "赵敏",
    mobile: "13811062817",
    address: "北京市朝阳区建国路 91 号金地中心 A 座",
    bankName: "中国建设银行北京建国路支行",
    accountNumber: "1105012349000000128",
}

const CONTRACT_NO = "HT-2026-HRYD-WF-001"

function todayBusinessDate() {
    const date = new Date()
    const pad = (value) => String(value).padStart(2, "0")
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

function nextYearBusinessDate() {
    const date = new Date()
    date.setFullYear(date.getFullYear() + 1)
    const pad = (value) => String(value).padStart(2, "0")
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

function foundationPdf() {
    const source = "%PDF-1.4\n1 0 obj<</Type/Catalog>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF\n"
    return new Blob([source], { type: "application/pdf" })
}

async function call(method, path, { token, body, form } = {}) {
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

async function login(account, password) {
    const data = await call("POST", "/login", {
        body: { account, password, account_kind: "admin" },
    })
    return data.token
}

async function findCustomer(adminToken) {
    const page = await call(
        "GET",
        `/admin/customers/all-authorized?keyword=${encodeURIComponent(CUSTOMER.legalName)}&page=1&page_size=50`,
        { token: adminToken },
    )
    const items = page?.items ?? []
    return items.find((row) => row.legal_name === CUSTOMER.legalName) ?? null
}

function customerBankAccounts() {
    return [
        {
            account_name: CUSTOMER.legalName,
            bank_name: CUSTOMER.bankName,
            account_number: CUSTOMER.accountNumber,
            is_default: true,
        },
    ]
}

async function createCustomer(token, actor) {
    const today = todayBusinessDate()
    return call("POST", "/admin/customer-profiles", {
        token,
        body: {
            idempotency_key: `dev-foundation-customer-v1-${actor}`,
            legal_name: CUSTOMER.legalName,
            short_name: CUSTOMER.shortName,
            unified_credit_code: CUSTOMER.unifiedCreditCode,
            default_payment_term_id: "CONTRACT",
            status: "active",
            contacts: [
                {
                    contact_name: CUSTOMER.contactName,
                    title: "福利采购经理",
                    mobile: CUSTOMER.mobile,
                    is_default: true,
                },
            ],
            addresses: [
                {
                    address_type: "registered",
                    contact_name: CUSTOMER.contactName,
                    address: CUSTOMER.address,
                    is_default: false,
                },
                {
                    address_type: "fulfillment",
                    contact_name: CUSTOMER.contactName,
                    address: CUSTOMER.address,
                    is_default: true,
                },
            ],
            effective_from: today,
            change_reason: "主数据初始化：客户建档",
        },
    })
}

async function ensureCustomerBankAccount(adminToken, customerId) {
    const detail = await call("GET", `/admin/customer-profiles/${encodeURIComponent(customerId)}`, {
        token: adminToken,
    })
    if ((detail.bank_accounts ?? []).length > 0) {
        console.log("客户银行账户已存在，跳过")
        return
    }
    await call("PUT", `/admin/customer-profiles/${encodeURIComponent(customerId)}`, {
        token: adminToken,
        body: {
            idempotency_key: "dev-foundation-customer-bank-v1",
            expected_party_version: detail.party_version,
            expected_customer_version: detail.version,
            legal_name: detail.legal_name || CUSTOMER.legalName,
            bank_accounts: customerBankAccounts(),
            effective_from: todayBusinessDate(),
            change_reason: "主数据初始化：补录银行账户",
        },
    })
    console.log("已补录客户银行账户")
}

async function ensureSalesOwner(adminToken, customerId, salesUserId) {
    const detail = await call("GET", `/admin/customer-profiles/${encodeURIComponent(customerId)}`, {
        token: adminToken,
    })
    const today = todayBusinessDate()
    const currentOwner = (detail.assignments ?? []).find((row) => {
        if (row.assignment_role !== "OWNER") return false
        if (row.valid_from > today) return false
        return !row.valid_to || row.valid_to > today
    })
    if (currentOwner?.user_id === salesUserId) {
        console.log("负责销售已是 xiaoshou，跳过归属")
        return detail
    }
    if (currentOwner && currentOwner.valid_from >= today) {
        const collaborator = (detail.assignments ?? []).find((row) => {
            if (row.assignment_role !== "COLLABORATOR" || row.user_id !== salesUserId) return false
            if (row.valid_from > today) return false
            return !row.valid_to || row.valid_to > today
        })
        if (!collaborator) {
            await call("POST", `/admin/customers/${encodeURIComponent(customerId)}/assignments`, {
                token: adminToken,
                body: {
                    action: "assign",
                    user_id: salesUserId,
                    assignment_role: "COLLABORATOR",
                    valid_from: today,
                    change_reason: "主数据初始化：同一天不能换 OWNER，改为协作销售",
                },
            })
            console.warn(
                `当前负责销售不是 xiaoshou，同一天不能换 OWNER，已把 xiaoshou 设为协作销售`,
            )
        } else {
            console.warn(
                `当前负责销售不是 xiaoshou，且归属从 ${currentOwner.valid_from} 起生效，同一天不能换 OWNER`,
            )
        }
        return call("GET", `/admin/customer-profiles/${encodeURIComponent(customerId)}`, {
            token: adminToken,
        })
    }
    await call("POST", `/admin/customers/${encodeURIComponent(customerId)}/assignments`, {
        token: adminToken,
        body: {
            action: "assign",
            user_id: salesUserId,
            assignment_role: "OWNER",
            valid_from: today,
            change_reason: "主数据初始化：指定负责销售",
        },
    })
    console.log("已将负责销售换为 xiaoshou")
    return call("GET", `/admin/customer-profiles/${encodeURIComponent(customerId)}`, {
        token: adminToken,
    })
}

async function findContract(adminToken, customerId) {
    const page = await call(
        "GET",
        `/admin/contracts?customer_id=${encodeURIComponent(customerId)}&page=1&page_size=50`,
        { token: adminToken },
    )
    const items = page?.items ?? []
    return (
        items.find((row) => row.contract_no === CONTRACT_NO) ??
        items.find((row) => row.status === "EFFECTIVE") ??
        null
    )
}

async function uploadContract(token, customer) {
    const today = todayBusinessDate()
    const command = {
        contract_no: CONTRACT_NO,
        customer_id: customer.id,
        settlement_party_id: customer.party_id,
        customer_name: customer.legal_name || CUSTOMER.legalName,
        settlement_party_name: customer.legal_name || CUSTOMER.legalName,
        payment_term_code: "CONTRACT",
        payment_term_name: "按合同约定",
        invoice_type: "增值税专用发票",
        tax_point: "13",
        valid_from: today,
        valid_to: nextYearBusinessDate(),
        signed_at: today,
    }
    const form = new FormData()
    form.append("file", foundationPdf(), "HT-2026-HRYD-WF-001.pdf")
    form.append("command", JSON.stringify(command))
    return call("POST", "/admin/contracts/upload", { token, form })
}

async function listAdmins(adminToken) {
    const rows = await call("GET", "/admin/admins", { token: adminToken })
    return Array.isArray(rows) ? rows : []
}

async function ensureRoleBoundAdmin(adminToken, { credentials, roleId, label }) {
    const roles = await call("GET", "/admin/roles", { token: adminToken })
    const role = (Array.isArray(roles) ? roles : []).find((row) => row.id === roleId)
    if (!role) {
        throw new Error(`未找到预定义角色 ${roleId}，无法创建${label}账号`)
    }

    let account = (await listAdmins(adminToken)).find((row) => row.account === credentials.account)
    if (!account) {
        await call("POST", "/admin/admins", {
            token: adminToken,
            body: {
                account: credentials.account,
                password: credentials.password,
                name: credentials.name,
                role_ids: [roleId],
            },
        })
        account = (await listAdmins(adminToken)).find((row) => row.account === credentials.account)
        if (!account) throw new Error(`${label}账号已创建但列表中找不到 ${credentials.account}`)
        console.log(`${label}账号已创建: ${credentials.account}`)
    } else {
        console.log(`${label}账号已存在: ${credentials.account}`)
    }

    const roleIds = Array.isArray(account.role_ids) ? account.role_ids : []
    if (!roleIds.includes(roleId)) {
        await call("PUT", `/admin/admins/${encodeURIComponent(account.id)}/role`, {
            token: adminToken,
            body: { role_ids: [...roleIds, roleId] },
        })
        console.log(`已为 ${credentials.account} 补上${role.name}角色`)
    }
    return account
}

async function ensureWarehouseAccount(adminToken) {
    const account = await ensureRoleBoundAdmin(adminToken, {
        credentials: ACCOUNTS.warehouse,
        roleId: ROLE_WAREHOUSE,
        label: "仓储",
    })
    const options = await call("GET", "/admin/warehouse-fulfillment-handler-options", {
        token: adminToken,
    })
    const option = (Array.isArray(options) ? options : []).find((row) => row.user_id === account.id)
    if (!option?.inbound_eligible || !option?.outbound_eligible) {
        throw new Error("cangchu 不具备入库或仓发完整执行权限，无法绑定仓库")
    }
    return account
}

async function ensureFinancePeople(adminToken) {
    const finance = await ensureRoleBoundAdmin(adminToken, {
        credentials: ACCOUNTS.finance,
        roleId: ROLE_FINANCE,
        label: "财务",
    })
    const payment = await ensureRoleBoundAdmin(adminToken, {
        credentials: ACCOUNTS.payment,
        roleId: ROLE_FINANCE,
        label: "付款人",
    })
    const invoice = await ensureRoleBoundAdmin(adminToken, {
        credentials: ACCOUNTS.invoice,
        roleId: ROLE_FINANCE,
        label: "开票人",
    })
    if (new Set([finance.id, payment.id, invoice.id]).size !== 3) {
        throw new Error("财务审批人、付款人和开票人必须是三个不同账号")
    }

    const options = await call("GET", "/admin/finance-responsibility-owner-options", {
        token: adminToken,
    })
    const rows = Array.isArray(options) ? options : []
    const paymentOption = rows.find((row) => row.user_id === payment.id)
    if (!paymentOption?.supplier_payment_eligible) {
        throw new Error("fukuan 不具备付款完整执行权限，无法配置默认付款负责人")
    }
    const invoiceOption = rows.find((row) => row.user_id === invoice.id)
    if (!invoiceOption?.sales_invoice_eligible) {
        throw new Error("kaipiao 不具备销项开票完整执行权限，无法配置默认开票负责人")
    }
    return { finance, payment, invoice }
}

function findDefaultFinanceRule(rows, operation) {
    const matches = rows.filter((row) => row.operation === operation && row.scope === "DEFAULT")
    return matches.find((row) => row.status === "active") ?? matches[0] ?? null
}

async function ensureDefaultFinanceRules(adminToken, people) {
    const listed = await call("GET", "/admin/finance-responsibility-rules", { token: adminToken })
    const rows = Array.isArray(listed) ? listed : []
    for (const rule of DEFAULT_FINANCE_RULES) {
        const owner = people[rule.accountKey]
        const existing = findDefaultFinanceRule(rows, rule.operation)
        if (existing?.status === "active" && existing.owner_user_id === owner.id) {
            console.log(`${rule.label}已是 ${owner.account}，跳过`)
            continue
        }
        if (existing) {
            await call("PUT", `/admin/finance-responsibility-rules/${encodeURIComponent(existing.id)}`, {
                token: adminToken,
                body: {
                    version: existing.version,
                    operation: rule.operation,
                    scope: "DEFAULT",
                    owner_user_id: owner.id,
                    status: "active",
                },
            })
            console.log(`已将${rule.label}更新为 ${owner.account}`)
            continue
        }
        await call("POST", "/admin/finance-responsibility-rules", {
            token: adminToken,
            body: {
                operation: rule.operation,
                scope: "DEFAULT",
                owner_user_id: owner.id,
                status: "active",
            },
        })
        console.log(`已配置${rule.label}: ${owner.account}`)
    }
}

async function listWarehouses(adminToken) {
    const page = await call("GET", "/admin/warehouses?page=1&page_size=100&sort_by=warehouse_code&sort_dir=asc", {
        token: adminToken,
    })
    return page?.items ?? []
}

async function bindWarehouseHandlers(adminToken, warehouse, handlerUserId) {
    const inbound = warehouse.inbound_handler_user_id?.trim()
    const outbound = warehouse.outbound_handler_user_id?.trim()
    if (inbound === handlerUserId && outbound === handlerUserId) return warehouse
    return call("PUT", `/admin/warehouses/${encodeURIComponent(warehouse.id)}/fulfillment-handlers`, {
        token: adminToken,
        body: {
            version: warehouse.version,
            inbound_handler_user_id: handlerUserId,
            outbound_handler_user_id: handlerUserId,
        },
    })
}

async function ensureWarehouse(adminToken, handlerUserId, spec) {
    const warehouses = await listWarehouses(adminToken)
    let warehouse = warehouses.find((row) => row.warehouse_code === spec.code)
    if (!warehouse) {
        warehouse = await call("POST", "/admin/warehouses", {
            token: adminToken,
            body: {
                warehouse_code: spec.code,
                name: spec.name,
                address: spec.address,
                contact: spec.contact,
                effective_from: todayBusinessDate(),
                change_reason: "主数据初始化：仓库",
                status: "active",
                inbound_handler_user_id: handlerUserId,
                outbound_handler_user_id: handlerUserId,
            },
        })
        console.log("仓库已创建:", warehouse.warehouse_code, spec.name)
        return warehouse
    }
    console.log("仓库已存在:", warehouse.warehouse_code)
    return bindWarehouseHandlers(adminToken, warehouse, handlerUserId)
}

async function ensureWarehouses(adminToken, handlerUserId) {
    const created = []
    for (const spec of WAREHOUSES) {
        created.push(await ensureWarehouse(adminToken, handlerUserId, spec))
    }
    return created
}

async function main() {
    const adminToken = await login(ACCOUNTS.admin.account, ACCOUNTS.admin.password)
    const salesToken = await login(ACCOUNTS.sales.account, ACCOUNTS.sales.password)
    const salesProfile = await call("GET", "/account/profile", { token: salesToken })
    const salesUserId = salesProfile.userid
    console.log("登录成功；销售账号 id:", salesUserId)

    const warehouseAccount = await ensureWarehouseAccount(adminToken)
    const warehouses = await ensureWarehouses(adminToken, warehouseAccount.id)
    const financePeople = await ensureFinancePeople(adminToken)
    await ensureDefaultFinanceRules(adminToken, financePeople)

    let customer = await findCustomer(adminToken)
    if (customer) {
        console.log("客户已存在:", customer.customer_no, customer.legal_name)
    } else {
        let created
        try {
            created = await createCustomer(salesToken, "xiaoshou")
            console.log("客户已创建（xiaoshou）:", created.customer_no, CUSTOMER.legalName)
        } catch (error) {
            console.warn("xiaoshou 创建客户失败，改用 admin:", error.message)
            created = await createCustomer(adminToken, "admin")
            console.log("客户已创建（admin）:", created.customer_no, CUSTOMER.legalName)
        }
        customer = {
            id: created.customer_id,
            customer_no: created.customer_no,
            party_id: created.party_id,
            legal_name: CUSTOMER.legalName,
        }
    }

    const detail = await ensureSalesOwner(adminToken, customer.id, salesUserId)
    customer = {
        id: detail.id ?? customer.id,
        customer_no: detail.customer_no ?? customer.customer_no,
        party_id: detail.party_id ?? customer.party_id,
        legal_name:
            detail.legal_name ?? detail.current_revision?.legal_name ?? customer.legal_name,
    }
    await ensureCustomerBankAccount(adminToken, customer.id)

    let contract = await findContract(adminToken, customer.id)
    if (contract) {
        console.log("合同已存在:", contract.contract_no, contract.status)
    } else {
        contract = await uploadContract(adminToken, customer)
        console.log("合同已归档:", contract.contract_no)
    }

    console.log("")
    console.log("== 开发开单底座已就绪 ==")
    console.log(`客户: ${customer.legal_name}（${customer.customer_no}）`)
    console.log(`合同: ${contract.contract_no}`)
    console.log(
        `仓库: ${warehouses.map((row) => `${row.warehouse_code} ${row.name ?? ""}`.trim()).join("、")}（收发经办人 cangchu）`,
    )
    console.log("销售负责人: xiaoshou")
    console.log("财务审批人: caiwu")
    console.log("默认付款负责人: fukuan")
    console.log("默认开票负责人: kaipiao")
    console.log("登录账号: admin / xiaoshou / caigou / caiwu / fukuan / kaipiao / cangchu    密码: 123456")
    console.log("下一步: 用 xiaoshou 打开销售开单页，选择该合同")
}

main().catch((error) => {
    console.error("开发开单底座失败:", error.message)
    process.exit(1)
})
