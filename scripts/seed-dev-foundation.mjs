#!/usr/bin/env node
/**
 * 开发开单底座：在业务数据已清空、web-api 已就绪后，补齐开单所需底座。
 *
 * 不写入销售单/采购单/库存/票款。账号、供应商、商品/SKU 由 reset 保留。
 * 仓库主数据也会保留，但空库或未绑定收发经办人时本脚本会补齐。
 * 审批定义由 publish-approval-definitions.mjs 单独发布。
 *
 * 幂等：客户、合同、仓储账号、开发仓均按固定标识查找；已存在则跳过或只补绑定。
 * 客户用 xiaoshou 创建，负责销售一开始就是开单账号（同一天不能把 OWNER 换人）。
 *
 * 用法: node scripts/seed-dev-foundation.mjs
 * 环境变量: API_BASE（默认 http://127.0.0.1:10001）
 */
const API_BASE = process.env.API_BASE || "http://127.0.0.1:10001"

const ACCOUNTS = {
    admin: { account: "admin", password: "123456" },
    sales: { account: "xiaoshou", password: "123456" },
    warehouse: { account: "cangchu", password: "123456", name: "仓储" },
}

const ROLE_WAREHOUSE = "role-warehouse"
const WAREHOUSE = {
    code: "DEV-WH-01",
    name: "开发开单仓",
    address: "北京市朝阳区开发路 1 号仓库",
    contact: "仓储",
}

const CUSTOMER = {
    legalName: "开发开单客户",
    shortName: "开发客户",
    unifiedCreditCode: "91110000DEVSEED001",
    contactName: "开发联系人",
    mobile: "13800000001",
    address: "北京市朝阳区开发路 1 号",
    bankName: "招商银行",
    accountNumber: "1101010000000000001",
}

const CONTRACT_NO = "DEV-SO-BASE-001"

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
                    title: "对接人",
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
            bank_accounts: [
                {
                    account_name: CUSTOMER.legalName,
                    bank_name: CUSTOMER.bankName,
                    account_number: CUSTOMER.accountNumber,
                    is_default: true,
                },
            ],
            effective_from: today,
            change_reason: "开发开单底座首版建档",
        },
    })
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
                    change_reason: "开发开单底座：同一天不能换 OWNER，改为协作销售",
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
            change_reason: "开发开单底座：销售负责",
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
    form.append("file", foundationPdf(), "dev-foundation-contract.pdf")
    form.append("command", JSON.stringify(command))
    return call("POST", "/admin/contracts/upload", { token, form })
}

async function listAdmins(adminToken) {
    const rows = await call("GET", "/admin/admins", { token: adminToken })
    return Array.isArray(rows) ? rows : []
}

async function ensureWarehouseAccount(adminToken) {
    const roles = await call("GET", "/admin/roles", { token: adminToken })
    const warehouseRole = (Array.isArray(roles) ? roles : []).find((row) => row.id === ROLE_WAREHOUSE)
    if (!warehouseRole) {
        throw new Error("未找到预定义角色 role-warehouse，无法创建仓储账号")
    }

    let account = (await listAdmins(adminToken)).find((row) => row.account === ACCOUNTS.warehouse.account)
    if (!account) {
        await call("POST", "/admin/admins", {
            token: adminToken,
            body: {
                account: ACCOUNTS.warehouse.account,
                password: ACCOUNTS.warehouse.password,
                name: ACCOUNTS.warehouse.name,
                role_ids: [ROLE_WAREHOUSE],
            },
        })
        account = (await listAdmins(adminToken)).find((row) => row.account === ACCOUNTS.warehouse.account)
        if (!account) throw new Error("仓储账号已创建但列表中找不到 cangchu")
        console.log("仓储账号已创建: cangchu")
    } else {
        console.log("仓储账号已存在: cangchu")
    }

    const roleIds = Array.isArray(account.role_ids) ? account.role_ids : []
    if (!roleIds.includes(ROLE_WAREHOUSE)) {
        await call("PUT", `/admin/admins/${encodeURIComponent(account.id)}/role`, {
            token: adminToken,
            body: { role_ids: [...roleIds, ROLE_WAREHOUSE] },
        })
        console.log("已为 cangchu 补上仓储角色")
    }

    const options = await call("GET", "/admin/warehouse-fulfillment-handler-options", {
        token: adminToken,
    })
    const option = (Array.isArray(options) ? options : []).find((row) => row.user_id === account.id)
    if (!option?.inbound_eligible || !option?.outbound_eligible) {
        throw new Error("cangchu 不具备入库或仓发完整执行权限，无法绑定仓库")
    }
    return account
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

async function ensureWarehouse(adminToken, handlerUserId) {
    let warehouses = await listWarehouses(adminToken)
    let warehouse = warehouses.find((row) => row.warehouse_code === WAREHOUSE.code)
    if (!warehouse) {
        warehouse = await call("POST", "/admin/warehouses", {
            token: adminToken,
            body: {
                warehouse_code: WAREHOUSE.code,
                name: WAREHOUSE.name,
                address: WAREHOUSE.address,
                contact: WAREHOUSE.contact,
                effective_from: todayBusinessDate(),
                change_reason: "开发开单底座",
                status: "active",
                inbound_handler_user_id: handlerUserId,
                outbound_handler_user_id: handlerUserId,
            },
        })
        console.log("仓库已创建:", warehouse.warehouse_code, WAREHOUSE.name)
        warehouses = await listWarehouses(adminToken)
        warehouse = warehouses.find((row) => row.id === warehouse.id) ?? warehouse
    } else {
        console.log("仓库已存在:", warehouse.warehouse_code)
        const bound = await bindWarehouseHandlers(adminToken, warehouse, handlerUserId)
        if (bound.inbound_handler_user_id !== warehouse.inbound_handler_user_id) {
            console.log("已将", warehouse.warehouse_code, "收发经办人绑定为 cangchu")
        }
        warehouse = bound
    }

    warehouses = await listWarehouses(adminToken)
    for (const row of warehouses) {
        if (row.id === warehouse.id) continue
        if (row.status && row.status !== "active") continue
        if (row.inbound_handler_user_id?.trim() && row.outbound_handler_user_id?.trim()) continue
        const bound = await bindWarehouseHandlers(adminToken, row, handlerUserId)
        console.log("已为已有仓库补绑定收发经办人:", bound.warehouse_code)
    }
    return warehouse
}

async function skuCount(adminToken, productKind) {
    try {
        const page = await call(
            "GET",
            `/admin/sellable-skus?product_kind=${encodeURIComponent(productKind)}&page=1&page_size=1`,
            { token: adminToken },
        )
        return page?.total ?? (page?.items ?? []).length
    } catch (error) {
        console.warn(`可售 SKU（${productKind}）查询失败: ${error.message}`)
        return null
    }
}

async function main() {
    const adminToken = await login(ACCOUNTS.admin.account, ACCOUNTS.admin.password)
    const salesToken = await login(ACCOUNTS.sales.account, ACCOUNTS.sales.password)
    const salesProfile = await call("GET", "/account/profile", { token: salesToken })
    const salesUserId = salesProfile.userid
    console.log("登录成功；销售账号 id:", salesUserId)

    const warehouseAccount = await ensureWarehouseAccount(adminToken)
    const warehouse = await ensureWarehouse(adminToken, warehouseAccount.id)

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

    let contract = await findContract(adminToken, customer.id)
    if (contract) {
        console.log("合同已存在:", contract.contract_no, contract.status)
    } else {
        contract = await uploadContract(adminToken, customer)
        console.log("合同已归档:", contract.contract_no)
    }

    const [physical, voucher, service] = await Promise.all([
        skuCount(adminToken, "PHYSICAL"),
        skuCount(adminToken, "VOUCHER"),
        skuCount(adminToken, "OFFLINE_SERVICE"),
    ])

    console.log("")
    console.log("== 开发开单底座已就绪 ==")
    console.log(`客户: ${customer.legal_name}（${customer.customer_no}）`)
    console.log(`合同: ${contract.contract_no}`)
    console.log(`仓库: ${warehouse.warehouse_code}（收发经办人 cangchu）`)
    console.log("销售负责人: xiaoshou")
    console.log(
        `可售 SKU: PHYSICAL ${physical ?? "?"} / VOUCHER ${voucher ?? "?"} / OFFLINE_SERVICE ${service ?? "?"}`,
    )
    console.log("登录账号: admin / xiaoshou / caigou / cangchu    密码: 123456")
    console.log("下一步: 用 xiaoshou 打开销售开单页，选择该合同")
    if (service === 0) {
        console.log("线下服务开单另需: node scripts/seed-service-sku.mjs")
    }
}

main().catch((error) => {
    console.error("开发开单底座失败:", error.message)
    process.exit(1)
})
