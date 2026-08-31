#!/usr/bin/env node
/**
 * 开发开单目录：主数据清空、web-api 就绪后，写入可开单的供应商、字典、商品与供给。
 *
 * 公司商品池不是独立集合：启用 SKU、已上架、已维护销售可见价、且存在
 * 业务时点有效并当前可供的供给时，才进入销售查询视图。
 *
 * 幂等：公司主体、字典、供应商、商品、供给均按固定编号查找。
 * 每个种子供应商必须存在当前默认收款账户，且详情必须返回付款提交所需的账户 ID 与版本。
 *
 * 用法: node scripts/seed-dev-catalog.mjs
 * 环境变量: API_BASE（默认 http://127.0.0.1:10001）
 */
const API_BASE = process.env.API_BASE || "http://127.0.0.1:10001";

const ADMIN = { account: "admin", password: "123456" };

const COMPANY_PARTY = {
  partyNo: "FSY",
  legalName: "北京福尚云科技有限公司",
  shortName: "福尚云",
  unifiedCreditCode: "91110108MA01FSY01X",
};

const UNITS = [
  { unitCode: "JIAN", name: "件", symbol: "件", quantityScale: 0 },
  { unitCode: "HE", name: "盒", symbol: "盒", quantityScale: 0 },
  { unitCode: "张", name: "张", symbol: "张", quantityScale: 0 },
  { unitCode: "CI", name: "次", symbol: "次", quantityScale: 0 },
];

const BRANDS = [
  { brandCode: "FSY", name: "福尚云" },
  { brandCode: "SF", name: "狮峰" },
];

const CATEGORIES = [
  { categoryCode: "TEA", name: "茶叶礼盒", productKind: "PHYSICAL" },
  { categoryCode: "VOUCHER", name: "卡券", productKind: "VOUCHER" },
  { categoryCode: "SVC", name: "上门服务", productKind: "OFFLINE_SERVICE" },
];

const SUPPLIERS = [
  {
    supplierNo: "SUP-HZSF",
    partyNo: "PTY-HZSF",
    legalName: "杭州狮峰茶叶有限公司",
    shortName: "狮峰茶叶",
    unifiedCreditCode: "91330106MA2HSF001X",
    contactName: "陈建国",
    mobile: "13958102618",
    address: "杭州市西湖区龙井路 18 号",
    bankName: "招商银行杭州西湖支行",
    accountNumber: "5719056188108012",
    taxNo: "91330106MA2HSF001X",
    settlementMode: "prepayment",
    paymentTerm: "PREPAY_50",
    businessCategory: "茶叶、年节礼盒",
    invoiceType: "vat_special",
    invoiceTaxRate: "0.13",
    capabilityCodes: ["physical"],
    rating: "A",
    score: 92,
  },
  {
    supplierNo: "SUP-SHTK",
    partyNo: "PTY-SHTK",
    legalName: "上海通卡信息服务有限公司",
    shortName: "上海通卡",
    unifiedCreditCode: "91310115MA1TK0010X",
    contactName: "孙丽",
    mobile: "13601628910",
    address: "上海市浦东新区张江路 665 号",
    bankName: "中国工商银行上海张江支行",
    accountNumber: "1001264009006801234",
    taxNo: "91310115MA1TK0010X",
    settlementMode: "cash_settlement",
    paymentTerm: "CASH_ON_APPROVAL",
    businessCategory: "预付卡、电子卡券",
    invoiceType: "vat_special",
    invoiceTaxRate: "0.06",
    capabilityCodes: ["virtual"],
    rating: "A",
    score: 88,
  },
  {
    supplierNo: "SUP-BJAD",
    partyNo: "PTY-BJAD",
    legalName: "北京安达家电服务有限公司",
    shortName: "安达服务",
    unifiedCreditCode: "91110115MA01AD001X",
    contactName: "刘洋",
    mobile: "13520186742",
    address: "北京市大兴区旧宫镇宣颐家园 12 号",
    bankName: "北京银行旧宫支行",
    accountNumber: "2000001234567890123",
    taxNo: "91110115MA01AD001X",
    settlementMode: "pay_after_use",
    paymentTerm: "POSTPAY_NET15",
    businessCategory: "家电安装、礼包派送",
    invoiceType: "vat_special",
    invoiceTaxRate: "0.06",
    capabilityCodes: ["offline_service"],
    rating: "B",
    score: 81,
  },
];

const PRODUCTS = [
  {
    kind: "PHYSICAL",
    productNo: "TEA-SF-LJ-001",
    skuNo: "TEA-SF-LJ-250",
    name: "狮峰明前龙井礼盒",
    skuName: "狮峰明前龙井礼盒 250g",
    description: "西湖产区明前特级龙井，纸质礼盒装，适合作春节、开业及客户馈赠。",
    specification: "250g/盒，明前特级，纸质礼盒",
    categoryCode: "TEA",
    brandCode: "SF",
    unitCode: "HE",
    barcode: "6901234567892",
    weightKg: "0.45",
    salesPrice: "1288.00",
    marketPrice: "1580.00",
    dropshipPrice: "920.00",
    bulkPrice: "860.00",
    moq: "6",
    availableQty: "260",
    supplyRegion: ["全国"],
    supplierNo: "SUP-HZSF",
    supplierProductCode: "SF-LJ-250",
    supplierSkuCode: "SF-LJ-250",
    specEntries: [{ attribute_code: "净含量", attribute_value_code: "250g" }],
  },
  {
    kind: "PHYSICAL",
    productNo: "TEA-SF-PE-001",
    skuNo: "TEA-SF-PE-500",
    name: "狮峰陈皮普洱礼盒",
    skuName: "狮峰陈皮普洱礼盒 500g",
    description: "新会陈皮与云南熟普拼配，礼盒装，适合作中秋、春节员工福利。",
    specification: "500g/盒，熟普拼配，纸质礼盒",
    categoryCode: "TEA",
    brandCode: "SF",
    unitCode: "HE",
    barcode: "6901234567908",
    weightKg: "0.72",
    salesPrice: "568.00",
    marketPrice: "698.00",
    dropshipPrice: "360.00",
    bulkPrice: "328.00",
    moq: "8",
    availableQty: "180",
    supplyRegion: ["全国"],
    supplierNo: "SUP-HZSF",
    supplierProductCode: "SF-PE-500",
    supplierSkuCode: "SF-PE-500",
    specEntries: [{ attribute_code: "净含量", attribute_value_code: "500g" }],
  },
  {
    kind: "VOUCHER",
    productNo: "FST-100",
    skuNo: "FST-100",
    name: "福尚通 100 元",
    skuName: "福尚通 100 元",
    description: "面值 100 元电子卡，全国商超、餐饮门店可核销，有效期以卡面为准。",
    specification: "电子卡，面值 100 元",
    categoryCode: "VOUCHER",
    brandCode: "FSY",
    unitCode: "张",
    salesPrice: "100.00",
    marketPrice: "100.00",
    dropshipPrice: "97.00",
    bulkPrice: "95.00",
    moq: "100",
    availableQty: "20000",
    supplyRegion: ["全国"],
    supplierNo: "SUP-SHTK",
    supplierProductCode: "FST-100",
    supplierSkuCode: "FST-100",
    specEntries: [],
  },
  {
    kind: "VOUCHER",
    productNo: "FST-500",
    skuNo: "FST-500",
    name: "福尚通 500 元",
    skuName: "福尚通 500 元",
    description: "面值 500 元电子卡，全国商超、餐饮门店可核销，适合作节日配赠。",
    specification: "电子卡，面值 500 元",
    categoryCode: "VOUCHER",
    brandCode: "FSY",
    unitCode: "张",
    salesPrice: "500.00",
    marketPrice: "500.00",
    dropshipPrice: "485.00",
    bulkPrice: "475.00",
    moq: "50",
    availableQty: "8000",
    supplyRegion: ["全国"],
    supplierNo: "SUP-SHTK",
    supplierProductCode: "FST-500",
    supplierSkuCode: "FST-500",
    specEntries: [],
  },
  {
    kind: "VOUCHER",
    productNo: "SC-1000",
    skuNo: "SC-1000",
    name: "商超购物卡 1000 元",
    skuName: "商超购物卡 1000 元",
    description: "面值 1000 元商超购物卡，指定连锁商超使用，适合作年度福利发放。",
    specification: "电子卡，面值 1000 元",
    categoryCode: "VOUCHER",
    brandCode: "FSY",
    unitCode: "张",
    salesPrice: "1000.00",
    marketPrice: "1000.00",
    dropshipPrice: "970.00",
    bulkPrice: "950.00",
    moq: "20",
    availableQty: "3000",
    supplyRegion: ["全国"],
    supplierNo: "SUP-SHTK",
    supplierProductCode: "SC-1000",
    supplierSkuCode: "SC-1000",
    specEntries: [],
  },
  {
    kind: "OFFLINE_SERVICE",
    productNo: "SVC-INSTALL-01",
    skuNo: "SVC-INSTALL-01",
    name: "家电上门安装",
    skuName: "家电上门安装（标准台）",
    description: "指定型号家电拆箱、安装、通电调试；不含高层搬运与拆旧。服务范围北京市六环内。",
    specification: "按台计费，含基础安装调试",
    categoryCode: "SVC",
    brandCode: "FSY",
    unitCode: "CI",
    salesPrice: "180.00",
    marketPrice: "220.00",
    dropshipPrice: "95.00",
    bulkPrice: "80.00",
    moq: "1",
    availableQty: "400",
    supplyRegion: ["北京"],
    supplierNo: "SUP-BJAD",
    supplierProductCode: "AD-INS-01",
    supplierSkuCode: "AD-INS-01",
    specEntries: [],
  },
  {
    kind: "OFFLINE_SERVICE",
    productNo: "SVC-DELIVER-01",
    skuNo: "SVC-DELIVER-01",
    name: "年节礼包上门派送",
    skuName: "年节礼包上门派送（北京）",
    description: "按客户提供的收件地址派送年节礼包，含签收拍照回传。服务范围北京市六环内。",
    specification: "按件计费，含签收拍照",
    categoryCode: "SVC",
    brandCode: "FSY",
    unitCode: "CI",
    salesPrice: "68.00",
    marketPrice: "88.00",
    dropshipPrice: "38.00",
    bulkPrice: "32.00",
    moq: "10",
    availableQty: "800",
    supplyRegion: ["北京"],
    supplierNo: "SUP-BJAD",
    supplierProductCode: "AD-DLV-01",
    supplierSkuCode: "AD-DLV-01",
    specEntries: [],
  },
];

function todayBusinessDate() {
  const date = new Date();
  const pad = (value) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

async function call(method, path, { token, body } = {}) {
  const headers = {};
  if (token) headers.Authorization = `Bearer ${token}`;
  if (body !== undefined) headers["Content-Type"] = "application/json";
  let res;
  try {
    res = await fetch(`${API_BASE}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch (error) {
    throw new Error(`API ${method} ${path} 网络错误: ${error.message}`);
  }
  const text = await res.text();
  let parsed = null;
  try {
    parsed = text ? JSON.parse(text) : null;
  } catch {
    throw new Error(
      `API ${method} ${path} 返回非 JSON（HTTP ${res.status}）: ${text.slice(0, 300)}`,
    );
  }
  if (res.status === 401 || parsed?.status === 401) {
    throw new Error(`API ${method} ${path} 未授权`);
  }
  if (!res.ok || parsed?.success === false) {
    throw new Error(
      `API ${method} ${path} 失败（HTTP ${res.status}）: ${parsed?.errorMessage ?? text}`,
    );
  }
  return parsed.data;
}

async function login() {
  const data = await call("POST", "/login", {
    body: { account: ADMIN.account, password: ADMIN.password, account_kind: "admin" },
  });
  return data.token;
}

function pageItems(page) {
  return page?.items ?? [];
}

async function ensureCompanyParty(token) {
  const named = pageItems(
    await call(
      "GET",
      `/admin/parties?keyword=${encodeURIComponent(COMPANY_PARTY.partyNo)}&status=active&page=1&page_size=50&sort_by=party_no&sort_dir=asc`,
      { token },
    ),
  ).find((row) => row.party_no === COMPANY_PARTY.partyNo);
  if (named) {
    console.log("公司主体已存在:", named.party_no, COMPANY_PARTY.legalName);
    return named;
  }
  const created = await call("POST", "/admin/parties", {
    token,
    body: {
      party_no: COMPANY_PARTY.partyNo,
      legal_name: COMPANY_PARTY.legalName,
      short_name: COMPANY_PARTY.shortName,
      unified_credit_code: COMPANY_PARTY.unifiedCreditCode,
      change_reason: "主数据初始化：公司签约及付款主体",
      status: "active",
    },
  });
  console.log("公司主体已创建:", created.party_no, COMPANY_PARTY.legalName);
  return created;
}

async function ensureUnit(token, spec) {
  const exact = pageItems(
    await call(
      "GET",
      `/admin/unit-of-measures?unit_code=${encodeURIComponent(spec.unitCode)}&page=1&page_size=10`,
      { token },
    ),
  )[0];
  if (exact) return exact;
  const created = await call("POST", "/admin/unit-of-measures", {
    token,
    body: {
      unit_code: spec.unitCode,
      name: spec.name,
      symbol: spec.symbol,
      quantity_scale: spec.quantityScale,
      status: "active",
    },
  });
  console.log("计量单位已创建:", created.unit_code, created.name);
  return created;
}

async function ensureBrand(token, spec) {
  const exact = pageItems(
    await call(
      "GET",
      `/admin/product-brands?brand_code=${encodeURIComponent(spec.brandCode)}&page=1&page_size=10`,
      { token },
    ),
  )[0];
  if (exact) return exact;
  const created = await call("POST", "/admin/product-brands", {
    token,
    body: { brand_code: spec.brandCode, name: spec.name, status: "active" },
  });
  console.log("品牌已创建:", created.brand_code, created.name);
  return created;
}

async function ensureCategory(token, spec) {
  const exact = pageItems(
    await call(
      "GET",
      `/admin/product-categories?category_code=${encodeURIComponent(spec.categoryCode)}&page=1&page_size=10`,
      { token },
    ),
  )[0];
  if (exact) return exact;
  const created = await call("POST", "/admin/product-categories", {
    token,
    body: {
      category_code: spec.categoryCode,
      name: spec.name,
      product_kind: spec.productKind,
      status: "active",
    },
  });
  console.log("分类已创建:", created.category_code, created.name, spec.productKind);
  return created;
}

async function findSupplier(token, supplierNo) {
  const items = pageItems(
    await call(
      "GET",
      `/admin/suppliers?keyword=${encodeURIComponent(supplierNo)}&page=1&page_size=50`,
      { token },
    ),
  );
  return items.find((row) => row.supplier_no === supplierNo) ?? null;
}

async function ensureSupplier(token, spec, companyPartyId) {
  const existing = await findSupplier(token, spec.supplierNo);
  if (existing) {
    console.log("供应商已存在:", existing.supplier_no, spec.legalName);
    return existing;
  }
  const created = await call("POST", "/admin/supplier-profiles", {
    token,
    body: {
      idempotency_key: `seed-supplier-${spec.supplierNo}`,
      party_no: spec.partyNo,
      supplier_no: spec.supplierNo,
      expected_party_version: null,
      expected_supplier_version: null,
      legal_name: spec.legalName,
      short_name: spec.shortName,
      unified_credit_code: spec.unifiedCreditCode,
      contact: {
        contact_name: spec.contactName,
        mobile: spec.mobile,
        telephone: null,
        email: null,
      },
      clear_contact: false,
      address: {
        address: spec.address,
        contact_name: spec.contactName,
      },
      clear_address: false,
      tax_no: spec.taxNo,
      clear_tax_profile: false,
      bank_account: {
        bank_name: spec.bankName,
        account_number: spec.accountNumber,
      },
      clear_bank_account: false,
      settlement_mode: spec.settlementMode,
      reconciliation_cycle: "monthly",
      payment_term_snapshot: spec.paymentTerm,
      business_category: spec.businessCategory,
      invoice_type: spec.invoiceType,
      invoice_tax_rate: spec.invoiceTaxRate,
      signing_entity_party_id: companyPartyId,
      payment_entity_party_id: companyPartyId,
      capability_codes: spec.capabilityCodes,
      qualifications: [],
      rating: {
        initial_score: spec.score,
        rating: spec.rating,
        current_score: spec.score,
        valid_from: todayBusinessDate(),
      },
      effective_from: todayBusinessDate(),
      change_reason: "主数据初始化：供应商建档",
    },
  });
  console.log("供应商已创建:", created.supplier_no, spec.legalName);
  return { id: created.supplier_id, supplier_no: created.supplier_no };
}

/**
 * 校验供应商默认收款账户满足工作台展示与付款并发校验合同。
 */
async function verifySupplierPaymentRecipient(token, supplier, spec) {
  const detail = await call("GET", `/admin/suppliers/${encodeURIComponent(supplier.id)}`, {
    token,
  });
  const today = todayBusinessDate();
  const account = (detail.bank_accounts ?? []).find(
    (row) =>
      row.is_default &&
      row.status === "active" &&
      row.valid_from <= today &&
      (!row.valid_to || row.valid_to > today),
  );
  if (!account) {
    throw new Error(`供应商 ${spec.supplierNo} 缺少当前默认收款账户`);
  }
  if (!account.id || !Number.isSafeInteger(account.version) || account.version < 0) {
    throw new Error(`供应商 ${spec.supplierNo} 的默认收款账户缺少账户 ID 或版本`);
  }
  const expectedLast4 = spec.accountNumber.slice(-4);
  if (
    account.account_name !== spec.legalName ||
    account.bank_name !== spec.bankName ||
    !account.account_number_masked?.endsWith(expectedLast4)
  ) {
    throw new Error(`供应商 ${spec.supplierNo} 的默认收款账户与种子合同不一致`);
  }
  return account;
}

async function findProduct(token, productNo) {
  const items = pageItems(
    await call(
      "GET",
      `/admin/products?product_no=${encodeURIComponent(productNo)}&page=1&page_size=10`,
      { token },
    ),
  );
  return items.find((row) => row.product_no === productNo) ?? null;
}

async function findSku(token, { skuNo, productId }) {
  const query = productId
    ? `product_id=${encodeURIComponent(productId)}`
    : `sku_no=${encodeURIComponent(skuNo)}`;
  const items = pageItems(await call("GET", `/admin/skus?${query}&page=1&page_size=10`, { token }));
  return items.find((row) => row.sku_no === skuNo) ?? items[0] ?? null;
}

async function ensureProductListed(token, productId) {
  await call("PUT", `/admin/products/${encodeURIComponent(productId)}/listing-status`, {
    token,
    body: { listing_status: "listed" },
  });
}

async function createPhysicalOrServiceProduct(token, spec, refs) {
  const existing = await findProduct(token, spec.productNo);
  if (existing) {
    console.log("商品已存在:", existing.product_no, spec.name);
    await ensureProductListed(token, existing.id);
    const sku = await findSku(token, { skuNo: spec.skuNo, productId: existing.id });
    if (!sku) throw new Error(`商品 ${spec.productNo} 已存在但找不到 SKU ${spec.skuNo}`);
    return { productId: existing.id, skuId: sku.id, skuNo: sku.sku_no };
  }
  const created = await call("POST", "/admin/products", {
    token,
    body: {
      change_reason: "主数据初始化：商品建档",
      product_no: spec.productNo,
      product_kind: spec.kind,
      name: spec.name,
      description: spec.description,
      specification: spec.specification,
      category_id: refs.categoryId,
      brand_id: refs.brandId,
      status: "active",
      effective_from: todayBusinessDate(),
      carousel_media: [],
      detail_media: [],
      skus: [
        {
          sku_no: spec.skuNo,
          name: spec.skuName,
          base_unit_id: refs.unitId,
          barcode: spec.barcode ?? null,
          main_image_asset_id: null,
          weight_kg: spec.weightKg ?? null,
          volume_m3: null,
          sales_visible_price_gross: spec.salesPrice,
          market_price: spec.marketPrice,
          spec_entries: spec.specEntries,
        },
      ],
    },
  });
  console.log("商品已创建:", created.product_no, spec.name);
  await ensureProductListed(token, created.id);
  const sku = await findSku(token, { skuNo: spec.skuNo, productId: created.id });
  if (!sku) throw new Error(`商品 ${spec.productNo} 已创建但找不到 SKU ${spec.skuNo}`);
  return { productId: created.id, skuId: sku.id, skuNo: sku.sku_no };
}

async function createVoucherProduct(token, spec, refs) {
  const existingSku = await findSku(token, { skuNo: spec.skuNo });
  if (existingSku) {
    console.log("卡券类目已存在:", existingSku.sku_no, spec.name);
    await ensureProductListed(token, existingSku.product_id);
    return {
      productId: existingSku.product_id,
      skuId: existingSku.id,
      skuNo: existingSku.sku_no,
    };
  }
  await call("POST", "/admin/voucher-categories", {
    token,
    body: {
      voucher_no: spec.productNo,
      name: spec.name,
      description: spec.description,
      specification: spec.specification,
      category_id: refs.categoryId,
      brand_id: refs.brandId,
      sku: {
        base_unit_id: refs.unitId,
        sales_visible_price_gross: spec.salesPrice,
        market_price: spec.marketPrice,
      },
      status: "active",
      effective_from: todayBusinessDate(),
    },
  });
  console.log("卡券类目已创建:", spec.productNo, spec.name);
  const sku = await findSku(token, { skuNo: spec.skuNo });
  if (!sku) throw new Error(`卡券 ${spec.productNo} 已创建但找不到 SKU`);
  await ensureProductListed(token, sku.product_id);
  return { productId: sku.product_id, skuId: sku.id, skuNo: sku.sku_no };
}

async function ensureOffering(token, spec, sku, supplierId) {
  const existing = pageItems(
    await call(
      "GET",
      `/admin/supplier-offerings?sku_id=${encodeURIComponent(sku.skuId)}&supplier_id=${encodeURIComponent(supplierId)}&page=1&page_size=10`,
      { token },
    ),
  )[0];
  if (existing) {
    console.log("供给已存在:", spec.skuNo, existing.id);
    return existing;
  }
  const created = await call("POST", "/admin/supplier-offerings", {
    token,
    body: {
      sku_id: sku.skuId,
      supplier_id: supplierId,
      supplier_product_code: spec.supplierProductCode,
      supplier_sku_code: spec.supplierSkuCode,
      source_type: "MANUAL",
      terms: {
        dropship_supply_price_gross: spec.dropshipPrice,
        bulk_supply_price_gross: spec.bulkPrice,
        input_tax_rate: spec.kind === "PHYSICAL" ? "0.13" : "0.06",
        bulk_minimum_order_quantity: spec.moq,
        supply_region: spec.supplyRegion,
        product_capabilities: [],
        valid_from: todayBusinessDate(),
      },
      availability_status: "AVAILABLE",
      available_quantity: spec.availableQty,
      change_reason: "主数据初始化：登记供给",
      idempotency_key: `seed-offering-${spec.skuNo}`,
    },
  });
  console.log("供给已创建:", spec.skuNo, created.offering_id);
  return created;
}

async function verifySellable(token, spec) {
  const page = await call(
    "GET",
    `/admin/sellable-skus?product_kind=${encodeURIComponent(spec.kind)}&page=1&page_size=50`,
    { token },
  );
  const hit = pageItems(page).find((row) => row.sku_no === spec.skuNo);
  if (!hit) {
    throw new Error(`${spec.kind} SKU ${spec.skuNo} 未进入公司商品池`);
  }
  return hit;
}

async function main() {
  const token = await login();
  const companyParty = await ensureCompanyParty(token);

  const units = {};
  for (const spec of UNITS) {
    units[spec.unitCode] = await ensureUnit(token, spec);
  }
  const brands = {};
  for (const spec of BRANDS) {
    brands[spec.brandCode] = await ensureBrand(token, spec);
  }
  const categories = {};
  for (const spec of CATEGORIES) {
    categories[spec.categoryCode] = await ensureCategory(token, spec);
  }

  const suppliers = {};
  for (const spec of SUPPLIERS) {
    const row = await ensureSupplier(token, spec, companyParty.id);
    await verifySupplierPaymentRecipient(token, row, spec);
    suppliers[spec.supplierNo] = row;
  }

  const seeded = [];
  for (const spec of PRODUCTS) {
    const refs = {
      categoryId: categories[spec.categoryCode].id,
      brandId: brands[spec.brandCode].id,
      unitId: units[spec.unitCode].id,
    };
    const sku =
      spec.kind === "VOUCHER"
        ? await createVoucherProduct(token, spec, refs)
        : await createPhysicalOrServiceProduct(token, spec, refs);
    const supplier = suppliers[spec.supplierNo];
    if (!supplier) throw new Error(`商品 ${spec.productNo} 缺少供应商 ${spec.supplierNo}`);
    await ensureOffering(token, spec, sku, supplier.id);
    const sellable = await verifySellable(token, spec);
    seeded.push(`${spec.kind} ${sellable.sku_no} ${sellable.name}`);
  }

  console.log("");
  console.log("== 开发开单目录已就绪 ==");
  console.log(`公司主体: ${COMPANY_PARTY.legalName}`);
  console.log(
    `供应商（均已配置默认收款账户）: ${SUPPLIERS.map((row) => row.shortName).join("、")}`,
  );
  console.log("公司商品池:");
  for (const line of seeded) {
    console.log(`  ${line}`);
  }
}

main().catch((error) => {
  console.error("开发开单目录失败:", error.message);
  process.exit(1);
});
