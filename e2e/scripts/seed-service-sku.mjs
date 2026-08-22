#!/usr/bin/env node
/**
 * 一次性主数据准备：为 flow-04（线下服务履约）创建 OFFLINE_SERVICE 可售 SKU。
 *
 * 主数据由 reset 脚本保留（跨流程轮次存在），因此只需创建一次；
 * 脚本幂等：已存在同名 SKU 时跳过。全部走应用自身 API（admin 账号）。
 *
 * 用法: node scripts/seed-service-sku.mjs
 */
const BASE = "http://127.0.0.1:10001"

async function api(path, { method = "GET", token, body } = {}) {
    const res = await fetch(`${BASE}${path}`, {
        method,
        headers: {
            "Content-Type": "application/json",
            ...(token ? { Authorization: `Bearer ${token}` } : {}),
        },
        body: body === undefined ? undefined : JSON.stringify(body),
    })
    const text = await res.text()
    const parsed = text ? JSON.parse(text) : null
    if (!res.ok || parsed?.status !== 200) {
        throw new Error(`${method} ${path} -> ${res.status}: ${text.slice(0, 500)}`)
    }
    return parsed.data
}

async function main() {
    const login = await api("/login", {
        method: "POST",
        body: { account: "admin", password: "123456", account_kind: "admin" },
    })
    const token = login.token

    // 固定字典（与现有 PHYSICAL SKU 一致，主数据保留）
    const CATEGORY_ID = "daf52eafd7934a908579081c09bb3e6f"
    const BRAND_ID = "1fca5d4dec004735a8e3fd0bff898e13"
    const BASE_UNIT_ID = "9b76adcc5f2e42b8acf294ee48b5b7a4"
    // 主太帅：唯一启用 offline_service 能力的供应商
    const SUPPLIER_ID = "139363a7d70b47c4bd8ed6b72c5e0d74"

    // 幂等：已存在 OFFLINE_SERVICE 可售 SKU 则跳过
    const existing = await api(
        "/admin/sellable-skus?product_kind=OFFLINE_SERVICE&page=1&page_size=5",
        { token },
    )
    if ((existing.items ?? []).length > 0) {
        console.log("已存在 OFFLINE_SERVICE 可售 SKU:", existing.items[0].sku_no, existing.items[0].name)
        return
    }

    const stamp = Date.now().toString(36).toUpperCase()
    const productNo = `E2ESVC${stamp.slice(-4)}`
    const skuNo = `SKU-SVC-${stamp.slice(-4)}`
    const today = new Date()
    const effectiveFrom = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, "0")}-${String(today.getDate()).padStart(2, "0")}`

    // 服务类分类（现有分类只允许 PHYSICAL/VOUCHER）
    let categoryId = CATEGORY_ID
    const cats = await api("/admin/product-categories?page=1&page_size=50", { token })
    const svcCat = (cats.items ?? []).find((c) => c.product_kind === "OFFLINE_SERVICE")
    if (!svcCat) {
        const cat = await api("/admin/product-categories", {
            method: "POST",
            token,
            body: {
                category_code: `SVC${stamp.slice(-4)}`,
                name: "服务",
                product_kind: "OFFLINE_SERVICE",
                status: "active",
            },
        })
        categoryId = cat.id
        console.log("分类已创建:", categoryId, "服务")
    } else {
        categoryId = svcCat.id
    }

    const product = await api("/admin/products", {
        method: "POST",
        token,
        body: {
            change_reason: "E2E flow-04 前置主数据：线下服务 SKU",
            product_no: productNo,
            product_kind: "OFFLINE_SERVICE",
            name: "线下安装服务",
            description: "E2E 线下服务履约测试商品",
            specification: "上门安装调试",
            category_id: categoryId,
            brand_id: BRAND_ID,
            status: "active",
            effective_from: effectiveFrom,
            carousel_media: [],
            detail_media: [],
            skus: [
                {
                    sku_no: skuNo,
                    name: "线下安装服务-标准",
                    base_unit_id: BASE_UNIT_ID,
                    barcode: null,
                    main_image_asset_id: null,
                    weight_kg: null,
                    volume_m3: null,
                    sales_visible_price_gross: "1000.00",
                    market_price: null,
                    spec_entries: [],
                },
            ],
        },
    })
    console.log("商品已创建:", productNo, "product id:", product.id)
    // 商品创建接口返回商品视图（无 SKU 行），从 SKU 列表取 SKU id
    const skus = await api(
        `/admin/skus?product_id=${encodeURIComponent(product.id)}&page=1&page_size=10`,
        { token },
    )
    const sku = (skus.items ?? []).find((s) => s.sku_no === skuNo)
    if (!sku) throw new Error(`SKU ${skuNo} 未找到`)
    console.log("SKU:", skuNo, "id:", sku.id, "version:", sku.version)

    // 上架
    const listed = await api(`/admin/skus/${encodeURIComponent(sku.id)}/listing-status`, {
        method: "PUT",
        token,
        body: { version: sku.version ?? 1, listing_status: "listed" },
    })
    console.log("上架完成:", JSON.stringify(listed))

    // 供应商供给（一件代发/集采条款）
    const offering = await api("/admin/supplier-offerings", {
        method: "POST",
        token,
        body: {
            sku_id: sku.id,
            supplier_id: SUPPLIER_ID,
            supplier_product_code: "E2ESVC",
            supplier_sku_code: skuNo,
            source_type: "MANUAL",
            terms: {
                dropship_supply_price_gross: "100.00",
                bulk_supply_price_gross: "80.00",
                input_tax_rate: "0.13",
                bulk_minimum_order_quantity: "10",
                supply_region: ["全国"],
                product_capabilities: [],
                valid_from: effectiveFrom,
            },
            availability_status: "AVAILABLE",
            available_quantity: "100",
            change_reason: "E2E flow-04 前置主数据",
            idempotency_key: `seed-svc-${stamp}`,
        },
    })
    console.log("供应商供给已创建:", offering.offering_id ?? JSON.stringify(offering).slice(0, 200))

    // 验证可售
    const verify = await api(
        "/admin/sellable-skus?product_kind=OFFLINE_SERVICE&page=1&page_size=5",
        { token },
    )
    console.log("可售列表:", (verify.items ?? []).map((i) => `${i.sku_no} ${i.name} suppliers=${i.supplier_count}`).join(", "))
    if ((verify.items ?? []).length === 0) {
        throw new Error("OFFLINE_SERVICE SKU 未出现在可售列表")
    }
    console.log("完成：flow-04 前置主数据就绪")
}

main().catch((e) => {
    console.error("失败:", e.message)
    process.exit(1)
})
