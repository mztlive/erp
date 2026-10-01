import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { SUPPLIERS, PRODUCTS, createPhysicalOrServiceProduct, createVoucherProduct, verifyVoucherProcurementAccess } from "./seed-dev-catalog.mjs";
import { FOUNDATION } from "./dev-seed-lib.mjs";
import { COMPANY_PARTY, SUPPLIER_SCENARIOS, ensureCompanyParty, ensureSupplier, resolveSupplierMaintainer, supplierCommand, supplierContract, verifySupplier, verifyOffering } from "./dev-supplier-seed.mjs";

const today = "2026-09-10";
const maintainerUserId = "user-procurement";
const company = { id: "company-1", party_no: "FSY", version: 2, status: "active",
  legal_name: COMPANY_PARTY.legalName, short_name: COMPANY_PARTY.shortName,
  aliases: COMPANY_PARTY.aliases, unified_credit_code: COMPANY_PARTY.unifiedCreditCode };
const specs = [...SUPPLIERS, ...SUPPLIER_SCENARIOS];

function supplierDetail(spec, date = today) {
  const command = supplierCommand(spec, company.id, maintainerUserId, date);
  return { id: "supplier-1", supplier_no: spec.supplierNo, status: "active", party_status: "active", maintainer_user_id: maintainerUserId,
    current_profile: command,
    capabilities: spec.capabilityCodes.map((code) => ({ id: `cap-${code}`, capability_code: code, owner_user_id: maintainerUserId, status: "active" })),
    qualifications: [{ ...command.qualifications[0], status: "active", capability_ids: spec.capabilityCodes.map((code) => `cap-${code}`) }],
  };
}

test("维护人按统一岗位账号目录解析真实 ID，缺失不回退为 admin", () => {
  const account = FOUNDATION.supplier_maintainer_account;
  assert.equal(resolveSupplierMaintainer([
    { account: "admin", id: "user-admin" }, { account, id: ` ${maintainerUserId} ` },
  ], account), maintainerUserId);
  for (const rows of [[], [{ account: "admin", id: "user-admin" }], [{ account, id: " " }]]) {
    assert.throws(() => resolveSupplierMaintainer(rows, account), /请先完成岗位账号初始化/);
  }
});

test("供应商整体维护人和每项能力负责人均显式指定，不发送岗位账号字符串", () => {
  const spec = { ...SUPPLIERS[0], capabilityCodes: ["physical", "virtual", "offline_service"] };
  const body = supplierCommand(spec, company.id, maintainerUserId, today);
  assert.equal(body.maintainer_user_id, maintainerUserId);
  assert.deepEqual(body.capability_owners, spec.capabilityCodes.map(code => ({
    capability_code: code, owner_user_id: maintainerUserId,
  })));
  for (const id of [undefined, "", " "]) {
    assert.throws(() => supplierCommand(spec, company.id, id, today), /不能为空/);
  }
});

test("公司首次通过专用接口创建；包含别名、不混入旧接口字段", async () => {
  const calls = [];
  const result = await ensureCompanyParty(async (method, path, options) => {
    calls.push({ method, path, body: options.body });
    return method === "GET" ? { items: [] } : company;
  }, "token");
  assert.equal(result.id, company.id);
  const write = calls.find((call) => call.method === "POST");
  assert.equal(write.path, "/admin/companies");
  assert.deepEqual(write.body.aliases, ["福尚云开发示例", "朱太帅", "科技"]);
  assert.equal(write.body.change_reason, undefined);
  assert.equal(write.body.version, null);
});

test("公司重跑只回读；补种子别名时保留现有别名和版本", async () => {
  await ensureCompanyParty(async (method) => {
    assert.equal(method, "GET"); return { items: [company] };
  }, "token");
  await ensureCompanyParty(async (method, path, options) => {
    if (method === "GET") return { items: [{ ...company, aliases: ["人工别名"] }] };
    assert.equal(method, "PUT");
    assert.equal(options.body.version, company.version);
    assert.deepEqual(options.body.aliases, ["人工别名", ...COMPANY_PARTY.aliases]);
    return company;
  }, "token");
});

test("旧种子公司补齐模板主体别名后重跑零写入，保留原身份和人工资料", async () => {
  let saved = { ...company, short_name: "人工简称", aliases: ["福尚云开发示例", "人工别名"] };
  let writes = 0;
  const call = async (method, path, options) => {
    if (method === "GET") return { items: [saved] };
    assert.equal(method, "PUT");
    assert.equal(path, `/admin/companies/${company.id}`);
    assert.deepEqual(options.body, {
      party_no: company.party_no, version: company.version,
      legal_name: company.legal_name, short_name: "人工简称",
      aliases: ["福尚云开发示例", "人工别名", "朱太帅", "科技"],
      unified_credit_code: company.unified_credit_code, status: "active",
    });
    writes++;
    saved = { ...saved, ...options.body, version: saved.version + 1 };
    return saved;
  };
  const first = await ensureCompanyParty(call, "token");
  assert.equal(first.id, company.id);
  assert.deepEqual(await ensureCompanyParty(call, "token"), first);
  assert.equal(writes, 1);
});

test("旧普通主体、停用公司或资料冲突均停止，不创建重复身份", async () => {
  await assert.rejects(ensureCompanyParty(async (method, path) => {
    assert.equal(method, "GET");
    return { items: path.startsWith("/admin/parties") ? [{ party_no: "FSY" }] : [] };
  }, "token"), /公司角色映射/);
  for (const patch of [{ status: "disabled" }, { unified_credit_code: "changed" }]) {
    await assert.rejects(ensureCompanyParty(async (method) => {
      assert.equal(method, "GET"); return { items: [{ ...company, ...patch }] };
    }, "token"), /不会覆盖/);
  }
});

test("种子覆盖五个自然周期、预付、现结，以及合同有效/未核实/过期", () => {
  assert.deepEqual(new Set(specs.map((row) => row.settlementMode)), new Set(["prepayment", "cash_settlement", "weekly", "monthly", "quarterly", "half_yearly", "yearly"]));
  assert.deepEqual(new Set(specs.map((row) => row.contractState)), new Set(["valid", "unverified", "expired"]));
  for (const spec of specs) {
    const body = supplierCommand(spec, company.id, maintainerUserId, today);
    assert.equal(body.invoice_tax_rate, undefined);
    assert.deepEqual(body.invoice_tax_rates, spec.invoiceTaxRates);
    assert.equal(body.signing_entity_party_id, company.id);
    assert.equal(body.payment_entity_party_id, company.id);
    assert.deepEqual(body.qualifications[0].capability_codes, spec.capabilityCodes);
    if (spec.contractState === "unverified") assert.equal(body.qualifications[0].valid_from, null);
    if (spec.contractState === "expired") assert.ok(body.qualifications[0].valid_to < today);
    if (spec.contractState === "valid") assert.ok(body.qualifications[0].valid_from <= today && body.qualifications[0].valid_to > today);
  }
  assert.equal(new Set(specs.map((row) => row.supplierNo)).size, specs.length);
  const tea = PRODUCTS.filter((row) => row.supplierNo === "SUP-HZSF");
  assert.deepEqual(tea.map((row) => row.inputTaxRate ?? "0.13"), ["0.09", "0.13"]);
});

test("合同日期偏移覆盖跨年、闰年，未知起始日期保持为空", () => {
  assert.equal(supplierContract({ ...SUPPLIERS[0], contractState: "expired" }, "2024-03-01").valid_to, "2024-02-29");
  assert.equal(supplierContract({ ...SUPPLIERS[0], contractState: "expired" }, "2026-01-01").valid_to, "2025-12-31");
});

test("已有供应商逐项核对商务和合同状态，不同资料不得假报成功", async () => {
  for (const spec of specs) await verifySupplier(async () => supplierDetail(spec), "token", { id: "supplier-1" }, spec, company.id, maintainerUserId, today);
  for (const mutate of [
    (row) => { row.maintainer_user_id = "user-admin"; },
    (row) => { row.capabilities[0].owner_user_id = "user-admin"; },
    (row) => { delete row.capabilities[0].owner_user_id; },
    (row) => { row.current_profile.payment_term_snapshot = "POSTPAY_NET15"; },
    (row) => { row.current_profile.invoice_tax_rates = ["0.06"]; },
    (row) => { row.current_profile.signing_entity_party_id = "wrong"; },
    (row) => { row.qualifications = []; },
    (row) => { row.qualifications[0].capability_ids = []; },
    (row) => { row.qualifications[0].valid_to = "2025-01-01"; },
  ]) {
    const detail = supplierDetail(SUPPLIERS[0]); mutate(detail);
    await assert.rejects(verifySupplier(async () => detail, "token", { id: "supplier-1" }, SUPPLIERS[0], company.id, maintainerUserId, today), /最新种子不一致/);
  }
});

test("供应商首次新建后核对，重复运行零新增，HTTP 失败保留失败", async () => {
  const spec = SUPPLIERS[0]; let detail; let writes = 0;
  const call = async (method, path, options) => {
    if (path.startsWith("/admin/suppliers?")) return { items: detail ? [{ id: detail.id, supplier_no: spec.supplierNo }] : [] };
    if (method === "POST") {
      assert.equal(options.token, "token");
      assert.equal(options.body.maintainer_user_id, maintainerUserId);
      assert.deepEqual(options.body.capability_owners, [{ capability_code: "physical", owner_user_id: maintainerUserId }]);
      writes++; detail = supplierDetail(spec, options.body.effective_from); return { supplier_id: detail.id, supplier_no: spec.supplierNo };
    }
    return detail;
  };
  const first = await ensureSupplier(call, "token", spec, company.id, maintainerUserId);
  assert.deepEqual(await ensureSupplier(call, "token", spec, company.id, maintainerUserId), first);
  assert.equal(writes, 1);
  await assert.rejects(ensureSupplier(async () => { throw new Error("读取失败"); }, "token", spec, company.id, maintainerUserId), /读取失败/);
});

test("保存的种子请求夹具与实际生成器一致", async () => {
  const fixture = JSON.parse(await readFile(new URL("./fixtures/dev-supplier-profiles.json", import.meta.url), "utf8"));
  assert.deepEqual(fixture, specs.map((spec) => supplierCommand(spec, company.id, maintainerUserId, today)));
});

test("实物和服务商品由 admin 操作但明确归属真实商品维护人", async () => {
  for (const spec of PRODUCTS.filter(row => row.kind !== "VOUCHER")) {
    const requests = [];
    const result = await createPhysicalOrServiceProduct("admin-token", spec,
      { categoryId: "category-1", brandId: "brand-1", unitId: "unit-1" }, maintainerUserId,
      async (method, path, options) => {
        requests.push({ method, path, ...options });
        if (path.startsWith("/admin/products?")) return { items: [] };
        if (method === "POST") return { id: "product-1", product_no: spec.productNo };
        if (path.startsWith("/admin/skus?")) return { items: [{ id: "sku-1", sku_no: spec.skuNo }] };
        return null;
      });
    assert.deepEqual(result, { productId: "product-1", skuId: "sku-1", skuNo: spec.skuNo });
    const created = requests.find(row => row.method === "POST");
    assert.equal(created.token, "admin-token");
    assert.equal(created.body.maintainer_user_id, maintainerUserId);
    assert.equal(created.body.product_kind, spec.kind);
    assert.equal(created.body.skus[0].sales_visible_price_gross, spec.salesPrice);
  }
});

test("已有商品继续复用身份，不隐式转移维护人", async () => {
  const spec = PRODUCTS[0];
  const writes = [];
  await createPhysicalOrServiceProduct("admin-token", spec, {}, maintainerUserId,
    async (method, path, options) => {
      if (path.startsWith("/admin/products?")) return { items: [{ id: "product-existing", product_no: spec.productNo, maintainer_user_id: "user-admin" }] };
      if (path.startsWith("/admin/skus?")) return { items: [{ id: "sku-existing", sku_no: spec.skuNo }] };
      writes.push({ method, path, body: options.body });
    });
  assert.deepEqual(writes, [{ method: "PUT", path: "/admin/products/product-existing/listing-status", body: { listing_status: "listed" } }]);
});

test("新卡券由 admin 合法创建并维护，重跑复用身份且不增加交接命令", async () => {
  const spec = PRODUCTS.find(row => row.kind === "VOUCHER");
  let sku;
  const requests = [];
  const request = async (method, path, options) => {
    requests.push({ method, path, ...options });
    if (path.startsWith("/admin/skus?")) return { items: sku ? [sku] : [] };
    if (path === "/admin/voucher-categories") {
      sku = { id: "voucher-sku", product_id: "voucher-product", sku_no: spec.skuNo };
      return { product_id: "voucher-product", product_version: 3 };
    }
    return null;
  };
  const args = ["admin-token", spec, { categoryId: "category-1", brandId: "brand-1", unitId: "unit-1" }, request];
  const first = await createVoucherProduct(...args);
  assert.deepEqual(first, { productId: "voucher-product", skuId: "voucher-sku", skuNo: spec.skuNo });
  assert.deepEqual(requests.filter(row => row.method === "POST").map(row => row.path), ["/admin/voucher-categories"]);
  assert.ok(requests.every(row => row.token === "admin-token"));
  assert.deepEqual(await createVoucherProduct(...args), first);
  assert.equal(requests.filter(row => row.method === "POST").length, 1);
  assert.equal(requests.filter(row => row.path.includes("/handover")).length, 0);
});

test("admin 维护的卡券必须按采购 token 回读实际界面三项目录", async () => {
  const spec = PRODUCTS.find(row => row.kind === "VOUCHER");
  const sku = { skuId: "voucher-sku" };
  const requests = [];
  await verifyVoucherProcurementAccess("procurement-token", spec, sku, async (method, path, options) => {
    requests.push({ method, path, ...options });
    if (path.startsWith("/admin/skus?")) return { items: [{ id: sku.skuId, sku_no: spec.skuNo }] };
    if (path.startsWith("/admin/voucher-category-profiles?")) return { items: [{ sku_id: sku.skuId, status: "active" }] };
    if (path.startsWith("/admin/sellable-skus?")) return { items: [{ sku_no: spec.skuNo }] };
    throw new Error(`意外接口 ${path}`);
  });
  assert.equal(requests.length, 3);
  assert.ok(requests.every(row => row.method === "GET" && row.token === "procurement-token"));
});

test("采购不可读取或选择卡券时停止，不回退 admin 或增加权限", async () => {
  const spec = PRODUCTS.find(row => row.kind === "VOUCHER");
  const sku = { skuId: "voucher-sku" };
  for (const denied of ["/admin/skus?", "/admin/voucher-category-profiles?", "/admin/sellable-skus?"]) {
    await assert.rejects(verifyVoucherProcurementAccess("procurement-token", spec, sku, async (method, path, options) => {
      assert.equal(method, "GET");
      assert.equal(options.token, "procurement-token");
      if (path.startsWith(denied)) throw new Error("HTTP 403");
      if (path.startsWith("/admin/skus?")) return { items: [{ id: sku.skuId, sku_no: spec.skuNo }] };
      if (path.startsWith("/admin/voucher-category-profiles?")) return { items: [{ sku_id: sku.skuId, status: "active" }] };
    }), /HTTP 403/);
  }
  await assert.rejects(verifyVoucherProcurementAccess("procurement-token", spec, sku, async () => ({ items: [] })), /无法选择卡券 SKU/);
  await assert.rejects(verifyVoucherProcurementAccess("procurement-token", spec, sku, async (method, path) => {
    if (path.startsWith("/admin/skus?")) return { items: [{ id: sku.skuId, sku_no: spec.skuNo }] };
    return { items: [{ sku_id: sku.skuId, status: "active", revision_no: 1 }, { sku_id: sku.skuId, status: "disabled", revision_no: 2 }] };
  }), /无法读取卡券类目/);
});

test("供给税率必须按 SKU 回读一致，不能把旧 13% 当作新 9% 样例", () => {
  const row = { supplier_id: "supplier-1", sku_id: "sku-1", status: "ACTIVE", availability_status: "AVAILABLE",
    valid_from: "2026-01-01", valid_to: null, input_tax_rate: "0.090000" };
  assert.equal(verifyOffering(row, PRODUCTS[0], "supplier-1", "sku-1", today), row);
  for (const patch of [{ input_tax_rate: "0.130000" }, { availability_status: "UNAVAILABLE" }, { valid_to: "2025-01-01" }]) {
    assert.throws(() => verifyOffering({ ...row, ...patch }, PRODUCTS[0], "supplier-1", "sku-1", today), /最新种子不一致/);
  }
  assert.doesNotThrow(() => verifyOffering({ ...row, input_tax_rate: "0.000000" }, { ...PRODUCTS[0], inputTaxRate: "0" }, "supplier-1", "sku-1", today));
});
